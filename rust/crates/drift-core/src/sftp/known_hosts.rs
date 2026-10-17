//! Go-compatible TOFU with OpenSSH patterns, hashed names and revoked keys.
use crate::error::{Error, Result};
use fs2::FileExt;
use hmac::{Hmac, Mac};
use russh::keys::{
    PublicKey,
    ssh_key::known_hosts::{HostPatterns, KnownHosts, Marker},
};
use sha1::Sha1;
use std::{
    fs::OpenOptions,
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::Path,
};

pub fn verify(path: &Path, names: &[String], key: &PublicKey) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)?;
    }
    let mut file = OpenOptions::new()
        .read(true)
        .append(true)
        .create(true)
        .mode(0o600)
        .open(path)?;
    file.try_lock_exclusive()?;
    let mut text = String::new();
    file.read_to_string(&mut text)?;
    let mut known = false;
    let mut matching = false;
    let normalized = text
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join("\n");
    for entry in KnownHosts::new(&normalized) {
        let entry = entry.map_err(|e| Error::Invalid(format!("parse known_hosts: {e}")))?;
        // Revocation is key-wide, as in Go's known_hosts checker.
        if entry.marker() == Some(&Marker::Revoked) {
            if entry.public_key().key_data() == key.key_data() {
                return Err(Error::Invalid(
                    "SSH host key is revoked in known_hosts".into(),
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
            return Err(Error::Invalid("SSH host certificates require certificate-authority verification, which is not implemented yet".into()));
        }
        matching |= entry.marker().is_none() && entry.public_key().key_data() == key.key_data();
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
                if wildcard(pattern.as_bytes(), name.as_bytes()) {
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
    while p < pattern.len() && pattern[p] == b'*' {
        p += 1;
    }
    p == pattern.len()
}

pub fn preferred(path: &Path, names: &[String]) -> Result<Vec<russh::keys::Algorithm>> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e.into()),
    };
    let normalized = text
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join("\n");
    let mut algorithms = vec![];
    for entry in KnownHosts::new(&normalized) {
        let entry = entry.map_err(|e| Error::Invalid(format!("parse known_hosts: {e}")))?;
        if entry.marker().is_none()
            && names
                .iter()
                .any(|name| matches_host(entry.host_patterns(), name))
        {
            let algorithm = entry.public_key().algorithm();
            if algorithm.clone().is_rsa() {
                algorithms.push(russh::keys::Algorithm::Rsa {
                    hash: Some(russh::keys::HashAlg::Sha512),
                });
                algorithms.push(russh::keys::Algorithm::Rsa {
                    hash: Some(russh::keys::HashAlg::Sha256),
                });
            }
            if !algorithms.contains(&algorithm) {
                algorithms.push(algorithm);
            }
        }
    }
    Ok(algorithms)
}
