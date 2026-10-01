//! Endpoint-scoped FTPS trust, shared formats and immutable handshake policy.
mod verify;
use crate::{
    error::{Error, Result},
    project::now,
    store::Store,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use toml::value::Datetime;
pub(crate) use verify::Verifier;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Endpoint {
    pub hostname: String,
    pub port: u16,
}
impl Endpoint {
    pub fn new(hostname: &str, port: u16) -> Result<Self> {
        let hostname = hostname.trim();
        let hostname = match hostname
            .trim_matches(['[', ']'])
            .parse::<std::net::IpAddr>()
        {
            Ok(address) => address.to_string(),
            Err(_) => hostname
                .strip_suffix('.')
                .unwrap_or(hostname)
                .to_lowercase(),
        };
        if hostname.is_empty() || hostname.chars().any(char::is_control) {
            return Err(Error::Invalid(
                "FTPS hostname is required and cannot contain controls".into(),
            ));
        }
        Ok(Self {
            hostname,
            port: if port == 0 { 21 } else { port },
        })
    }
    pub fn address(&self) -> String {
        if self.hostname.contains(':') {
            format!("[{}]:{}", self.hostname, self.port)
        } else {
            format!("{}:{}", self.hostname, self.port)
        }
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Problem {
    UnknownAuthority,
    HostnameMismatch,
    Expired,
    NotYetValid,
    CertificateChanged,
}
impl Problem {
    pub fn label(self) -> &'static str {
        match self {
            Self::UnknownAuthority => "Unknown certificate authority",
            Self::HostnameMismatch => "Hostname mismatch",
            Self::Expired => "Certificate expired",
            Self::NotYetValid => "Certificate not yet valid",
            Self::CertificateChanged => "Certificate changed",
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct TrustedCertificate {
    pub protocol: String,
    pub hostname: String,
    pub port: u16,
    pub fingerprint: String,
    pub problems: Vec<Problem>,
    pub trusted_at: Datetime,
}
impl TrustedCertificate {
    pub fn endpoint(&self) -> Endpoint {
        Endpoint {
            hostname: self.hostname.clone(),
            port: self.port,
        }
    }
    pub fn validate(&self) -> Result<()> {
        if self.protocol != "ftps"
            || self.port == 0
            || Endpoint::new(&self.hostname, self.port)?.hostname != self.hostname
        {
            return Err(Error::Invalid(
                "trusted certificate endpoint must be normalized FTPS".into(),
            ));
        }
        let bytes: Vec<_> = self.fingerprint.split(':').collect();
        if bytes.len() != 32
            || bytes.iter().any(|byte| {
                byte.len() != 2
                    || !byte
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b))
            })
        {
            return Err(Error::Invalid(
                "fingerprint must be uppercase colon-separated SHA-256".into(),
            ));
        }
        if self.problems.is_empty()
            || self.problems.contains(&Problem::CertificateChanged)
            || self
                .problems
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != self.problems.len()
        {
            return Err(Error::Invalid(
                "trusted certificate problems must be unique, nonempty and approvable".into(),
            ));
        }
        let date = chrono::DateTime::parse_from_rfc3339(&self.trusted_at.to_string())
            .map_err(|_| Error::Invalid("trusted_at must be an offset timestamp".into()))?;
        if date.timestamp() == -62135596800 {
            return Err(Error::Invalid("trusted_at cannot be zero".into()));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Challenge {
    pub endpoint: Endpoint,
    pub fingerprint: String,
    pub problems: Vec<Problem>,
    pub subject: String,
    pub issuer: String,
    pub names: Vec<String>,
    pub not_before: String,
    pub not_after: String,
    pub previous_fingerprint: Option<String>,
    pub(crate) expected: Option<TrustedCertificate>,
}
impl std::fmt::Display for Challenge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "verify FTPS certificate for {}: {}",
            self.endpoint.address(),
            self.problems
                .iter()
                .map(|p| p.label())
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}
impl Challenge {
    pub fn trust(&self) -> Option<TrustedCertificate> {
        let mut problems: Vec<_> = self
            .problems
            .iter()
            .copied()
            .filter(|p| *p != Problem::CertificateChanged)
            .collect();
        problems.sort();
        problems.dedup();
        (!problems.is_empty()).then(|| TrustedCertificate {
            protocol: "ftps".into(),
            hostname: self.endpoint.hostname.clone(),
            port: self.endpoint.port,
            fingerprint: self.fingerprint.clone(),
            problems,
            trusted_at: now(),
        })
    }
}
#[derive(Clone, Debug)]
pub struct Policy {
    entries: BTreeMap<Endpoint, TrustedCertificate>,
    persistent: BTreeMap<Endpoint, TrustedCertificate>,
    roots: Arc<rustls::RootCertStore>,
    required: Option<Challenge>,
}
impl Policy {
    pub fn require(mut self, challenge: Challenge) -> Self {
        self.required = Some(challenge);
        self
    }
    pub(crate) fn verifier(&self, endpoint: Endpoint) -> Result<Arc<verify::Verifier>> {
        if let Some(required) = &self.required
            && required.endpoint != endpoint
        {
            return Err(Error::Invalid(format!(
                "FTPS retry certificate belongs to {}, not {}",
                required.endpoint.address(),
                endpoint.address()
            )));
        }
        Ok(Arc::new(verify::Verifier::new(self.clone(), endpoint)))
    }
}
pub struct Manager {
    store: Store,
    session: Mutex<BTreeMap<Endpoint, TrustedCertificate>>,
    roots: Option<Arc<rustls::RootCertStore>>,
}
/// Immutable confirmation snapshot. A reset rejects changes at this endpoint.
#[derive(Clone, Debug)]
pub struct TrustSnapshot {
    pub endpoint: Endpoint,
    pub persistent: Option<TrustedCertificate>,
    pub session: Option<TrustedCertificate>,
}
impl Manager {
    pub fn new(store: Store) -> Self {
        Self {
            store,
            session: Mutex::new(BTreeMap::new()),
            roots: None,
        }
    }
    /// Custom real CA roots for embedded deployments and protocol tests.
    pub fn with_roots(store: Store, roots: rustls::RootCertStore) -> Self {
        Self {
            roots: Some(Arc::new(roots)),
            ..Self::new(store)
        }
    }
    /// May read disk/native roots: call from bounded background work.
    pub fn policy(&self) -> Result<Policy> {
        let persistent: BTreeMap<_, _> = self
            .store
            .trusted_certificates()?
            .into_iter()
            .map(|entry| (entry.endpoint(), entry))
            .collect();
        let mut entries = persistent.clone();
        entries.extend(self.session.lock().unwrap().clone());
        let roots = match &self.roots {
            Some(roots) => roots.clone(),
            None => {
                let certificates = rustls_native_certs::load_native_certs();
                if !certificates.errors.is_empty() {
                    return Err(Error::Invalid(format!(
                        "cannot load native certificate roots: {:?}",
                        certificates.errors
                    )));
                }
                let mut roots = rustls::RootCertStore::empty();
                roots.add_parsable_certificates(certificates.certs);
                Arc::new(roots)
            }
        };
        Ok(Policy {
            entries,
            persistent,
            roots,
            required: None,
        })
    }
    pub fn grant(&self, challenge: &Challenge, permanent: bool) -> Result<()> {
        let Some(entry) = challenge.trust() else {
            return Ok(());
        };
        entry.validate()?;
        let mut session = self.session.lock().unwrap();
        if permanent {
            self.store
                .save_trusted_certificate(challenge.expected.as_ref(), entry.clone())?;
        }
        session.insert(entry.endpoint(), entry);
        Ok(())
    }
    pub fn inspect(&self, endpoint: Endpoint) -> Result<TrustSnapshot> {
        let session = self.session.lock().unwrap();
        let persistent = self
            .store
            .trusted_certificates()?
            .into_iter()
            .find(|entry| entry.endpoint() == endpoint);
        Ok(TrustSnapshot {
            session: session.get(&endpoint).cloned(),
            persistent,
            endpoint,
        })
    }
    /// Existing connections keep their immutable policy. New handshakes require
    /// verification again; reset never reconnects or repeats a transfer.
    pub fn reset(&self, expected: &TrustSnapshot) -> Result<()> {
        let mut session = self.session.lock().unwrap();
        if session.get(&expected.endpoint) != expected.session.as_ref() {
            return Err(Error::Conflict(format!(
                "session trust for {}",
                expected.endpoint.address()
            )));
        }
        self.store
            .delete_trusted_certificate(&expected.endpoint, expected.persistent.as_ref())?;
        session.remove(&expected.endpoint);
        Ok(())
    }
}
