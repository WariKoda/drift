//! OpenSSH host CA verification and Go-compatible raw-key TOFU.
use crate::error::{Error, Result};
use fs2::FileExt;
use hmac::{Hmac, Mac};
use russh::keys::{
    Algorithm, HashAlg, PublicKey, PublicKeyOrCertificate,
    ssh_key::{
        Certificate,
        certificate::CertType,
        known_hosts::{Entry as KnownEntry, HostPatterns, Marker},
    },
};
use sha1::Sha1;
use std::{
    fs::OpenOptions,
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

pub fn verify(
    path: &Path,
    names: &[String],
    hostname: &str,
    presented: &PublicKeyOrCertificate,
) -> Result<()> {
    let mut options = OpenOptions::new();
    options.read(true).custom_flags(libc::O_NONBLOCK);
    if presented.certificate().is_none() {
        if let Some(parent) = path.parent() {
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(parent)?;
        }
        options.append(true).create(true).mode(0o600);
    }
    let mut file = options.open(path).map_err(|error| {
        if presented.certificate().is_some() && error.kind() == std::io::ErrorKind::NotFound {
            Error::Invalid("SSH host certificate has no trusted authority for this endpoint".into())
        } else {
            Error::Io(error)
        }
    })?;
    if !file.metadata()?.is_file() {
        return Err(Error::Invalid("known_hosts must be a regular file".into()));
    }
    file.try_lock_exclusive()?;
    let mut text = String::new();
    file.read_to_string(&mut text)?;
    // Parse everything before accepting or appending, including lines after a pin.
    let entries = parse_entries(&text)?;
    let key = presented.public_key();
    let cert = presented.certificate();
    let mut known = false;
    let mut matching = false;
    let mut authority = None;
    for entry in &entries {
        // Certificate, leaf and signing-key revocations are not endpoint-scoped.
        if entry.marker() == Some(&Marker::Revoked) {
            if entry.public_key().key_data() == key.key_data()
                || cert.is_some_and(|cert| entry.public_key().key_data() == cert.signature_key())
            {
                return Err(Error::Invalid(
                    "SSH host key or certificate authority is revoked in known_hosts".into(),
                ));
            }
            continue;
        }
        if !names
            .iter()
            .any(|name| matches_host(entry.host_patterns(), name))
        {
            continue;
        }
        known = true;
        if entry.marker() == Some(&Marker::CertAuthority) {
            if entry.certificate.is_none()
                && cert.is_some_and(|cert| entry.public_key().key_data() == cert.signature_key())
            {
                authority = Some(entry.public_key().fingerprint(HashAlg::Sha256));
            }
        }
        if entry.certificate.is_none() {
            matching |= entry.public_key().key_data() == key.key_data();
        }
    }
    if let Some(cert) = cert {
        let authority = authority.ok_or_else(|| {
            Error::Invalid("SSH host certificate has no trusted authority for this endpoint".into())
        })?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| {
                Error::Invalid("SSH certificate verification requires a valid clock".into())
            })?
            .as_secs();
        check_certificate(cert, hostname, now, &authority)?;
        FileExt::unlock(&file)?;
        return Ok(());
    }
    if known && !matching {
        return Err(Error::Invalid(format!(
            "SSH host identification has changed for {}; check {}",
            names[0],
            path.display()
        )));
    }
    if !known {
        if !text.is_empty() && !text.ends_with('\n') {
            file.write_all(b"\n")?;
        }
        writeln!(
            file,
            "{} {}",
            names[0],
            key.to_openssh()
                .map_err(|e| Error::Invalid(format!("encode SSH key: {e}")))?
        )?;
        file.sync_all()?;
    }
    FileExt::unlock(&file)?;
    Ok(())
}
fn matches_host(patterns: &HostPatterns, name: &str) -> bool {
    match patterns {
        HostPatterns::Patterns(patterns) => {
            let mut matched = false;
            for pattern in patterns {
                let (negative, pattern) = pattern
                    .strip_prefix('!')
                    .map_or((false, pattern.as_str()), |p| (true, p));
                let (Ok((pattern_host, pattern_port)), Ok((host, port))) =
                    (split_endpoint(pattern), split_endpoint(name))
                else {
                    continue;
                };
                if pattern_port == port && wildcard(pattern_host.as_bytes(), host.as_bytes()) {
                    if negative {
                        return false;
                    }
                    matched = true;
                }
            }
            matched
        }
        HostPatterns::HashedName { salt, hash } => {
            Hmac::<Sha1>::new_from_slice(salt).is_ok_and(|hmac| {
                hmac.chain_update(name.as_bytes())
                    .verify_slice(hash)
                    .is_ok()
            })
        }
    }
}
pub fn endpoint(host: &str, port: u16) -> String {
    let host = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);
    if port == 22 {
        host.into()
    } else {
        format!("[{host}]:{port}")
    }
}

// OpenSSH host patterns only give '*' and '?' special meaning. In particular,
// brackets around a non-default port are literal, not glob character classes.
fn wildcard(pattern: &[u8], name: &[u8]) -> bool {
    let (mut p, mut n, mut star, mut retry) = (0, 0, None, 0);
    while n < name.len() {
        if p < pattern.len() && (pattern[p] == b'?' || pattern[p] == name[n]) {
            p += 1;
            n += 1;
        } else if p < pattern.len() && pattern[p] == b'*' {
            star = Some(p);
            p += 1;
            retry = n;
        } else if let Some(index) = star {
            p = index + 1;
            retry += 1;
            n = retry;
        } else {
            return false;
        }
    }
    // Go does not consume a trailing wildcard once the hostname is exhausted.
    p == pattern.len()
}

pub fn preferred(path: &Path, names: &[String]) -> Result<russh::Preferred> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.into()),
    };
    let entries = parse_entries(&text)?;
    let mut preferred = russh::Preferred::default();
    let supported = preferred.key.to_vec();
    let mut algorithms = vec![];
    let mut authority = false;
    let mut raw_pin = false;
    for entry in &entries {
        if entry.marker() == Some(&Marker::Revoked)
            || entries.iter().any(|revoked| {
                revoked.marker() == Some(&Marker::Revoked)
                    && revoked.public_key().key_data() == entry.public_key().key_data()
            })
            || !names
                .iter()
                .any(|name| matches_host(entry.host_patterns(), name))
        {
            continue;
        }
        if entry.marker() == Some(&Marker::CertAuthority) {
            authority |= entry.certificate.is_none();
            continue;
        }
        // An ordinary certificate pin cannot substitute for a host CA or raw pin.
        if entry.certificate.is_some() {
            continue;
        }
        raw_pin = true;
        let algorithm = entry.public_key().algorithm();
        // Retain the existing raw RSA preference; certificate negotiation excludes SHA-1.
        for candidate in supported.iter().filter(|candidate| {
            if matches!(algorithm, Algorithm::Rsa { .. }) {
                matches!(candidate, Algorithm::Rsa { .. })
            } else {
                **candidate == algorithm
            }
        }) {
            if !algorithms.contains(candidate) {
                algorithms.push(candidate.clone());
            }
        }
    }
    if authority || !raw_pin {
        // Unknown certificates must reach verification, not fall through to raw TOFU.
        // A CA's algorithm says nothing about the leaf key it may sign.
        preferred.host_key_certificates = std::borrow::Cow::Owned(
            supported
                .iter()
                .filter(|algorithm| !matches!(algorithm, Algorithm::Rsa { hash: None }))
                .cloned()
                .collect(),
        );
    }
    for algorithm in supported {
        if !algorithms.contains(&algorithm) {
            algorithms.push(algorithm);
        }
    }
    preferred.key = std::borrow::Cow::Owned(algorithms);
    Ok(preferred)
}

struct Entry {
    record: KnownEntry,
    certificate: Option<Certificate>,
}
impl Entry {
    fn marker(&self) -> Option<&Marker> {
        self.record.marker()
    }
    fn host_patterns(&self) -> &HostPatterns {
        self.record.host_patterns()
    }
    fn public_key(&self) -> &PublicKey {
        self.record.public_key()
    }
}
fn parse_entries(text: &str) -> Result<Vec<Entry>> {
    let mut entries = vec![];
    for line in text.lines() {
        let line = line.split('#').next().unwrap().trim();
        if line.is_empty() {
            continue;
        }
        let fields = line.split_whitespace().collect::<Vec<_>>();
        let offset = usize::from(fields[0].starts_with('@'));
        let malformed = || Error::Invalid("parse known_hosts: malformed entry".into());
        if fields.len() < offset + 3 {
            return Err(malformed());
        }
        let certificate = if fields[offset + 1].ends_with("-cert-v01@openssh.com") {
            Some(
                Certificate::from_openssh(&format!(
                    "{} {}",
                    fields[offset + 1],
                    fields[offset + 2]
                ))
                .map_err(|_| malformed())?,
            )
        } else {
            None
        };
        let normalized = if let Some(cert) = &certificate {
            // Reuse the SDK's marker/pattern parser without treating a certificate as a raw pin.
            let raw = PublicKey::new(cert.public_key().clone(), "")
                .to_openssh()
                .map_err(|_| malformed())?;
            format!(
                "{}{} {raw}",
                if offset == 1 {
                    format!("{} ", fields[0])
                } else {
                    String::new()
                },
                fields[offset]
            )
        } else {
            fields.join(" ")
        };
        entries.push(Entry {
            record: normalized.parse().map_err(|_| malformed())?,
            certificate,
        });
    }
    for entry in &entries {
        if let HostPatterns::Patterns(patterns) = entry.host_patterns() {
            for pattern in patterns {
                if pattern.is_empty() {
                    continue;
                }
                let pattern = pattern.strip_prefix('!').unwrap_or(pattern);
                if pattern.is_empty() || pattern.starts_with('|') {
                    return Err(Error::Invalid(
                        "parse known_hosts: malformed host pattern".into(),
                    ));
                }
                split_endpoint(pattern)?;
            }
        }
    }
    Ok(entries)
}

fn split_endpoint(name: &str) -> Result<(&str, &str)> {
    if let Some(name) = name.strip_prefix('[') {
        if let Some((host, port)) = name.split_once("]:") {
            if !host.contains(['[', ']']) && !port.contains([':', '[', ']']) {
                return Ok((host, port));
            }
        }
        return Err(Error::Invalid(
            "parse known_hosts: malformed bracketed endpoint".into(),
        ));
    }
    if name.contains(['[', ']']) {
        return Err(Error::Invalid(
            "parse known_hosts: malformed endpoint".into(),
        ));
    }
    if name.bytes().filter(|byte| *byte == b':').count() == 1 {
        return Ok(name.split_once(':').expect("one colon"));
    }
    Ok((name, "22"))
}

fn check_certificate(
    cert: &Certificate,
    hostname: &str,
    now: u64,
    authority: &russh::keys::ssh_key::Fingerprint,
) -> Result<()> {
    if cert.cert_type() != CertType::Host {
        return Err(Error::Invalid(
            "SSH host certificate must have host type".into(),
        ));
    }
    // No host critical option is enforced by this transport, including source-address.
    if !cert.critical_options().is_empty() {
        return Err(Error::Invalid(
            "SSH host certificate has unsupported critical options".into(),
        ));
    }
    if !cert.valid_principals().is_empty()
        && !cert
            .valid_principals()
            .iter()
            .any(|principal| principal == hostname)
    {
        return Err(Error::Invalid(
            "SSH host certificate principal does not match requested hostname".into(),
        ));
    }
    // Go rejects timestamps outside int64, except its infinity sentinel.
    if cert.valid_after() > i64::MAX as u64 || now < cert.valid_after() {
        return Err(Error::Invalid(
            "SSH host certificate is not yet valid".into(),
        ));
    }
    if cert.valid_before() != u64::MAX
        && (cert.valid_before() > i64::MAX as u64 || now >= cert.valid_before())
    {
        return Err(Error::Invalid("SSH host certificate has expired".into()));
    }
    cert.validate_at(now, [authority]).map_err(|_| {
        Error::Invalid("SSH host certificate signature or validity check failed".into())
    })
}

#[cfg(test)]
mod tests;
