use super::{Challenge, Endpoint, Policy, Problem};
use rustls::{
    CertificateError, DigitallySignedStruct,
    client::{
        danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
        verify_server_cert_signed_by_trust_anchor, verify_server_name,
    },
    pki_types::{CertificateDer, ServerName, UnixTime},
    server::ParsedCertificate,
};
use sha2::{Digest, Sha256};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use x509_parser::{extensions::GeneralName, prelude::*};

#[derive(Debug)]
pub(crate) struct Verifier {
    policy: Policy,
    endpoint: Endpoint,
    control: Mutex<Option<String>>,
    challenge: Mutex<Option<Challenge>>,
}
impl Verifier {
    pub(super) fn new(policy: Policy, endpoint: Endpoint) -> Self {
        Self {
            policy,
            endpoint,
            control: Mutex::new(None),
            challenge: Mutex::new(None),
        }
    }
    pub(crate) fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }
    pub(crate) fn take_challenge(&self) -> Option<Challenge> {
        self.challenge.lock().unwrap().take()
    }
    pub(crate) fn config(self: &Arc<Self>) -> crate::error::Result<rustls::ClientConfig> {
        let mut config = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_protocol_versions(&[&rustls::version::TLS12])
        .map_err(|e| crate::error::Error::Invalid(format!("FTPS TLS configuration: {e}")))?
        .dangerous()
        .with_custom_certificate_verifier(self.clone())
        .with_no_client_auth();
        config.resumption = rustls::client::Resumption::disabled();
        Ok(config)
    }
}
fn bounded(value: impl ToString) -> String {
    value
        .to_string()
        .chars()
        .filter(|c| !c.is_control())
        .take(512)
        .collect()
}
impl ServerCertVerifier for Verifier {
    fn verify_server_cert(
        &self,
        leaf: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        name: &ServerName<'_>,
        _ocsp: &[u8],
        now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        let bad_encoding = || rustls::Error::InvalidCertificate(CertificateError::BadEncoding);
        let (remaining, cert) =
            X509Certificate::from_der(leaf.as_ref()).map_err(|_| bad_encoding())?;
        if !remaining.is_empty() {
            return Err(bad_encoding());
        }
        let parsed = ParsedCertificate::try_from(leaf)?;
        let fingerprint = Sha256::digest(leaf.as_ref())
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect::<Vec<_>>()
            .join(":");
        let mut problems = vec![];
        let now_seconds = i64::try_from(now.as_secs()).map_err(|_| bad_encoding())?;
        let mut latest = cert.validity().not_before.timestamp();
        let mut earliest = cert.validity().not_after.timestamp();
        for der in std::iter::once(leaf).chain(intermediates) {
            let (rest, certificate) =
                X509Certificate::from_der(der.as_ref()).map_err(|_| bad_encoding())?;
            if !rest.is_empty() {
                return Err(bad_encoding());
            }
            let start = certificate.validity().not_before.timestamp();
            let end = certificate.validity().not_after.timestamp();
            latest = latest.max(start);
            earliest = earliest.min(end);
            if now_seconds < start {
                problems.push(Problem::NotYetValid);
            }
            if now_seconds > end {
                problems.push(Problem::Expired);
            }
        }
        if latest >= earliest {
            return Err(rustls::Error::General(
                "FTPS certificate chain has no common validity period".into(),
            ));
        }
        match verify_server_name(&parsed, name) {
            Ok(()) => {}
            Err(rustls::Error::InvalidCertificate(
                CertificateError::NotValidForName | CertificateError::NotValidForNameContext { .. },
            )) => problems.push(Problem::HostnameMismatch),
            Err(error) => return Err(error),
        }
        let seconds = if now_seconds < latest || now_seconds > earliest {
            latest + (earliest - latest) / 2
        } else {
            now_seconds
        };
        let at = UnixTime::since_unix_epoch(Duration::from_secs(
            u64::try_from(seconds).map_err(|_| bad_encoding())?,
        ));
        let algorithms = rustls::crypto::ring::default_provider().signature_verification_algorithms;
        match verify_server_cert_signed_by_trust_anchor(
            &parsed,
            &self.policy.roots,
            intermediates,
            at,
            algorithms.all,
        ) {
            Ok(()) => {}
            Err(rustls::Error::InvalidCertificate(CertificateError::UnknownIssuer)) => {
                // Approval cannot bypass signatures, usage, constraints or
                // malformed chains. Revalidate with the final certificate as
                // the temporary anchor, at a common valid time, like Go.
                let mut temporary = rustls::RootCertStore::empty();
                temporary.add(intermediates.last().unwrap_or(leaf).clone())?;
                verify_server_cert_signed_by_trust_anchor(
                    &parsed,
                    &temporary,
                    intermediates,
                    at,
                    algorithms.all,
                )?;
                problems.push(Problem::UnknownAuthority);
            }
            Err(error) => return Err(error),
        }
        let mut control = self.control.lock().unwrap();
        let approved = self.policy.entries.get(&self.endpoint);
        let required = control.as_ref().or_else(|| {
            self.policy
                .required
                .as_ref()
                .filter(|c| c.endpoint == self.endpoint)
                .map(|c| &c.fingerprint)
        });
        let previous = if required.is_some_and(|previous| previous != &fingerprint) {
            problems.push(Problem::CertificateChanged);
            required.cloned()
        } else if !problems.is_empty()
            && approved.is_some_and(|entry| entry.fingerprint != fingerprint)
        {
            problems.push(Problem::CertificateChanged);
            approved.map(|entry| entry.fingerprint.clone())
        } else {
            None
        };
        problems.sort();
        problems.dedup();
        let accepted = problems.is_empty()
            || approved.is_some_and(|entry| {
                let mut approved_problems = entry.problems.clone();
                approved_problems.sort();
                entry.fingerprint == fingerprint && approved_problems == problems
            });
        if accepted {
            if control.is_none() {
                *control = Some(fingerprint);
            }
            return Ok(ServerCertVerified::assertion());
        }
        let names = cert
            .subject_alternative_name()
            .map_err(|_| bad_encoding())?
            .map(|san| {
                san.value
                    .general_names
                    .iter()
                    .filter_map(|name| match name {
                        GeneralName::DNSName(name) => Some(bounded(name)),
                        GeneralName::IPAddress(bytes) => match bytes.len() {
                            4 => Some(
                                std::net::Ipv4Addr::from(<[u8; 4]>::try_from(*bytes).unwrap())
                                    .to_string(),
                            ),
                            16 => Some(
                                std::net::Ipv6Addr::from(<[u8; 16]>::try_from(*bytes).unwrap())
                                    .to_string(),
                            ),
                            _ => None,
                        },
                        _ => None,
                    })
                    .take(32)
                    .collect()
            })
            .unwrap_or_default();
        let challenge = Challenge {
            endpoint: self.endpoint.clone(),
            fingerprint,
            problems,
            subject: bounded(cert.subject()),
            issuer: bounded(cert.issuer()),
            names,
            not_before: bounded(cert.validity().not_before),
            not_after: bounded(cert.validity().not_after),
            previous_fingerprint: previous,
            expected: self.policy.persistent.get(&self.endpoint).cloned(),
        };
        let message = challenge.to_string();
        *self.challenge.lock().unwrap() = Some(challenge);
        Err(rustls::Error::General(message))
    }
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            signature,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            signature,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }
    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}
