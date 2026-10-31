use super::*;
mod compatibility;
use russh::keys::ssh_key::{
    PrivateKey,
    certificate::Builder,
    rand_core::{TryCryptoRng, TryRng, UnwrapErr},
};
use tempfile::TempDir;

// Use the existing getrandom dependency with ssh-key's re-exported RNG traits.
struct OsRng;
impl TryRng for OsRng {
    type Error = getrandom::Error;
    fn try_next_u32(&mut self) -> std::result::Result<u32, Self::Error> {
        let mut bytes = [0; 4];
        getrandom::fill(&mut bytes)?;
        Ok(u32::from_ne_bytes(bytes))
    }
    fn try_next_u64(&mut self) -> std::result::Result<u64, Self::Error> {
        let mut bytes = [0; 8];
        getrandom::fill(&mut bytes)?;
        Ok(u64::from_ne_bytes(bytes))
    }
    fn try_fill_bytes(&mut self, bytes: &mut [u8]) -> std::result::Result<(), Self::Error> {
        getrandom::fill(bytes)
    }
}
impl TryCryptoRng for OsRng {}

struct Fixture {
    dir: TempDir,
    ca: PrivateKey,
    leaf: PrivateKey,
    other: PrivateKey,
}
impl Fixture {
    fn new() -> Self {
        let mut rng = UnwrapErr(OsRng);
        Self {
            dir: tempfile::tempdir().unwrap(),
            ca: PrivateKey::random(&mut rng, Algorithm::Ed25519).unwrap(),
            leaf: PrivateKey::random(&mut rng, Algorithm::Ed25519).unwrap(),
            other: PrivateKey::random(&mut rng, Algorithm::Ed25519).unwrap(),
        }
    }
    fn path(&self) -> std::path::PathBuf {
        self.dir.path().join(".ssh/known_hosts")
    }
    fn write(&self, text: &str) {
        std::fs::create_dir_all(self.path().parent().unwrap()).unwrap();
        std::fs::write(self.path(), text).unwrap();
    }
    fn line(&self, marker: &str, pattern: &str, key: &PrivateKey) -> String {
        format!(
            "{marker}{pattern} {}\n",
            key.public_key().to_openssh().unwrap()
        )
    }
    fn builder(&self, principals: &[&str], after: u64, before: u64) -> Builder {
        let mut builder = Builder::new_with_random_nonce(
            &mut UnwrapErr(OsRng),
            self.leaf.public_key().key_data().clone(),
            after,
            before,
        )
        .unwrap();
        builder.cert_type(CertType::Host).unwrap();
        if principals.is_empty() {
            builder.all_principals_valid().unwrap();
        } else {
            for principal in principals {
                builder.valid_principal(*principal).unwrap();
            }
        }
        builder
    }
    fn cert(&self, principals: &[&str]) -> Certificate {
        self.builder(principals, 0, u64::MAX)
            .sign(&self.ca)
            .unwrap()
    }
    fn verify(&self, host: &str, port: u16, presented: PublicKeyOrCertificate) -> Result<()> {
        let hostname = host.trim_matches(['[', ']']);
        verify(&self.path(), &[endpoint(host, port)], hostname, &presented)
    }
    fn rejected_unchanged(&self, host: &str, port: u16, presented: PublicKeyOrCertificate) {
        let before = std::fs::read(self.path()).unwrap();
        assert!(self.verify(host, port, presented).is_err());
        assert_eq!(std::fs::read(self.path()).unwrap(), before);
    }
    fn check(&self, cert: &Certificate, host: &str, now: u64) -> Result<()> {
        check_certificate(
            cert,
            host,
            now,
            &self.ca.public_key().fingerprint(HashAlg::Sha256),
        )
    }
}

#[test]
fn ca_accepts_real_host_certificate_without_writing() {
    let f = Fixture::new();
    let text = f.line("@cert-authority ", "host.example", &f.ca);
    f.write(&text);
    f.verify("host.example", 22, f.cert(&["host.example"]).into())
        .unwrap();
    assert_eq!(std::fs::read_to_string(f.path()).unwrap(), text);
}

#[test]
fn certificates_never_tofu_or_fall_back_to_leaf_pins() {
    let f = Fixture::new();
    let cert = f.cert(&["host.example"]);
    for text in [
        String::new(),
        f.line("", "host.example", &f.leaf),
        f.line("@cert-authority ", "host.example", &f.other),
        f.line("@cert-authority ", "elsewhere.example", &f.ca),
        f.line("", "host.example", &f.ca),
    ] {
        f.write(&text);
        f.rejected_unchanged("host.example", 22, cert.clone().into());
    }
    std::fs::remove_file(f.path()).unwrap();
    assert!(f.verify("host.example", 22, cert.into()).is_err());
    assert!(!f.path().exists());
}

#[test]
fn raw_tofu_preserves_permissions_newline_and_changed_key_rejection() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    f.verify("host.example", 2200, f.leaf.public_key().clone().into())
        .unwrap();
    assert_eq!(
        std::fs::metadata(f.path()).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        std::fs::metadata(f.path().parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    let text = f.line("", "[host.example]:2200", &f.leaf);
    assert_eq!(std::fs::read_to_string(f.path()).unwrap(), text);
    f.verify("host.example", 2200, f.leaf.public_key().clone().into())
        .unwrap();
    f.rejected_unchanged("host.example", 2200, f.other.public_key().clone().into());
    f.write("# existing comment without newline");
    f.verify("host.example", 22, f.leaf.public_key().clone().into())
        .unwrap();
    assert!(
        std::fs::read_to_string(f.path())
            .unwrap()
            .starts_with("# existing comment without newline\nhost.example ")
    );
}

#[test]
fn raw_ca_endpoint_rejects_other_keys_but_preserves_exact_key_and_explicit_pins() {
    let f = Fixture::new();
    let ca = f.line("@cert-authority ", "host.example", &f.ca);
    f.write(&ca);
    f.rejected_unchanged("host.example", 22, f.leaf.public_key().clone().into());
    f.verify("host.example", 22, f.ca.public_key().clone().into())
        .unwrap();
    assert_eq!(std::fs::read_to_string(f.path()).unwrap(), ca);
    let pin = f.line("", "host.example", &f.leaf);
    for text in [format!("{ca}{pin}"), format!("{pin}{ca}")] {
        f.write(&text);
        f.verify("host.example", 22, f.leaf.public_key().clone().into())
            .unwrap();
        assert_eq!(std::fs::read_to_string(f.path()).unwrap(), text);
    }
}

#[test]
fn revocations_are_global_and_override_earlier_pins_and_ca_trust() {
    let f = Fixture::new();
    let cert = f.cert(&["host.example"]);
    for revoked in [&f.ca, &f.leaf] {
        let text = format!(
            "{}{}{}",
            f.line("@cert-authority ", "host.example", &f.ca),
            f.line("", "host.example", &f.leaf),
            f.line("@revoked ", "unrelated.example", revoked)
        );
        f.write(&text);
        f.rejected_unchanged("host.example", 22, cert.clone().into());
    }
    f.write(&format!(
        "{}{}",
        f.line("", "host.example", &f.leaf),
        f.line("@revoked ", "unrelated.example", &f.leaf)
    ));
    f.rejected_unchanged("host.example", 22, f.leaf.public_key().clone().into());
    f.write(&f.line("@revoked ", "unrelated.example", &f.leaf));
    f.rejected_unchanged("host.example", 22, f.leaf.public_key().clone().into());
}

#[test]
fn entire_file_must_parse_even_after_an_accepting_entry() {
    let f = Fixture::new();
    for bad in [
        "malformed",
        "@unknown host.example ssh-ed25519 bad",
        "host.example ssh-ed25519 bad",
    ] {
        for good in [
            String::new(),
            f.line("", "host.example", &f.leaf),
            f.line("@cert-authority ", "host.example", &f.ca),
            f.line("@revoked ", "unrelated.example", &f.other),
        ] {
            f.write(&format!("{good}{bad}\n"));
            f.rejected_unchanged("host.example", 22, f.leaf.public_key().clone().into());
            f.rejected_unchanged("host.example", 22, f.cert(&["host.example"]).into());
            assert!(preferred(&f.path(), &[endpoint("host.example", 22)]).is_err());
        }
    }
    for bad_pattern in ["!", "[broken", "[broken]", "|2|salt|hash"] {
        f.write(&f.line("", bad_pattern, &f.other));
        f.rejected_unchanged("host.example", 22, f.leaf.public_key().clone().into());
    }
}

#[test]
fn wildcard_and_negation_ca_scopes_have_exact_ports() {
    let f = Fixture::new();
    let cert = f.cert(&[]);
    f.write(&f.line("@cert-authority ", "*.example,!denied.example", &f.ca));
    f.verify("allowed.example", 22, cert.clone().into())
        .unwrap();
    f.rejected_unchanged("denied.example", 22, cert.clone().into());
    f.rejected_unchanged("allowed.example", 2200, cert.clone().into());
    f.write(&f.line(
        "@cert-authority ",
        "[*.example]:2200,![denied.example]:2200",
        &f.ca,
    ));
    f.verify("allowed.example", 2200, cert.clone().into())
        .unwrap();
    f.rejected_unchanged("denied.example", 2200, cert.clone().into());
    f.rejected_unchanged("allowed.example", 22, cert.clone().into());
    f.rejected_unchanged("allowed.example", 2201, cert.into());
    f.write(&f.line("@cert-authority ", "host?.example:2200", &f.ca));
    f.verify("host1.example", 2200, f.cert(&["host1.example"]).into())
        .unwrap();
}

#[test]
fn hashed_ca_names_use_normalized_endpoint_including_nondefault_port() {
    let f = Fixture::new();
    for port in [22, 2200] {
        let name = endpoint("host.example", port);
        let salt = vec![42; 20];
        let hash = Hmac::<Sha1>::new_from_slice(&salt)
            .unwrap()
            .chain_update(name.as_bytes())
            .finalize()
            .into_bytes()
            .into();
        let pattern = HostPatterns::HashedName { salt, hash }.to_string();
        f.write(&f.line("@cert-authority ", &pattern, &f.ca));
        f.verify("host.example", port, f.cert(&["host.example"]).into())
            .unwrap();
        f.rejected_unchanged(
            "host.example",
            if port == 22 { 2200 } else { 22 },
            f.cert(&["host.example"]).into(),
        );
        f.rejected_unchanged("other.example", port, f.cert(&[]).into());
    }
}

#[test]
fn principals_are_exact_requested_hostnames_not_ports_ips_or_wildcards() {
    let f = Fixture::new();
    f.write(&f.line("@cert-authority ", "[host.example]:2200", &f.ca));
    f.verify("host.example", 2200, f.cert(&["host.example"]).into())
        .unwrap();
    for principal in [
        "other.example",
        "127.0.0.1",
        "*.example",
        "HOST.EXAMPLE",
        "host.example:2200",
        "[host.example]:2200",
        "login-user",
    ] {
        f.rejected_unchanged("host.example", 2200, f.cert(&[principal]).into());
    }
    f.write(&f.line("@cert-authority ", "127.0.0.1", &f.ca));
    f.rejected_unchanged("host.example", 22, f.cert(&["host.example"]).into());
}

#[test]
fn ipv6_brackets_and_ports_are_not_part_of_certificate_principal() {
    let f = Fixture::new();
    for port in [22, 2200] {
        assert_eq!(
            endpoint("[2001:db8::1]", port),
            endpoint("2001:db8::1", port)
        );
        f.write(&f.line("@cert-authority ", &endpoint("2001:db8::1", port), &f.ca));
        f.verify("[2001:db8::1]", port, f.cert(&["2001:db8::1"]).into())
            .unwrap();
        f.rejected_unchanged("[2001:db8::1]", port, f.cert(&["[2001:db8::1]"]).into());
    }
}

#[test]
fn empty_principals_accept_any_host_but_still_require_scoped_ca() {
    let f = Fixture::new();
    let cert = f.cert(&[]);
    assert!(f.check(&cert, "anything.example", 100).is_ok());
    f.write(&f.line("@cert-authority ", "host.example", &f.ca));
    f.verify("host.example", 22, cert.clone().into()).unwrap();
    f.rejected_unchanged("anything.example", 22, cert.into());
}

#[test]
fn user_certificates_and_all_critical_options_are_rejected() {
    let f = Fixture::new();
    f.write(&f.line("@cert-authority ", "host.example", &f.ca));
    let mut user = Builder::new_with_random_nonce(
        &mut UnwrapErr(OsRng),
        f.leaf.public_key().key_data().clone(),
        0,
        u64::MAX,
    )
    .unwrap();
    user.cert_type(CertType::User)
        .unwrap()
        .valid_principal("host.example")
        .unwrap();
    f.rejected_unchanged("host.example", 22, user.sign(&f.ca).unwrap().into());
    for option in ["force-command", "source-address", "unknown-option"] {
        let mut builder = f.builder(&["host.example"], 0, u64::MAX);
        builder.critical_option(option, "value").unwrap();
        f.rejected_unchanged("host.example", 22, builder.sign(&f.ca).unwrap().into());
    }
    let mut builder = f.builder(&["host.example"], 0, u64::MAX);
    builder.extension("unknown-extension", "value").unwrap();
    f.verify("host.example", 22, builder.sign(&f.ca).unwrap().into())
        .unwrap();
}

#[test]
fn certificate_time_boundaries_and_go_infinity_policy() {
    let f = Fixture::new();
    let cert = f.builder(&["host.example"], 100, 200).sign(&f.ca).unwrap();
    assert!(f.check(&cert, "host.example", 99).is_err());
    assert!(f.check(&cert, "host.example", 100).is_ok());
    assert!(f.check(&cert, "host.example", 199).is_ok());
    assert!(f.check(&cert, "host.example", 200).is_err());
    assert!(f.check(&cert, "host.example", 201).is_err());
    assert!(
        f.check(&f.cert(&[]), "host.example", i64::MAX as u64)
            .is_ok()
    );
    let cert = f
        .builder(&[], i64::MAX as u64 + 1, u64::MAX)
        .sign(&f.ca)
        .unwrap();
    assert!(f.check(&cert, "host.example", i64::MAX as u64 + 1).is_err());
    let cert = f.builder(&[], 0, i64::MAX as u64 + 1).sign(&f.ca).unwrap();
    assert!(f.check(&cert, "host.example", 100).is_err());
}

#[test]
fn production_clock_rejects_expired_and_future_certificates_without_writing() {
    let f = Fixture::new();
    f.write(&f.line("@cert-authority ", "host.example", &f.ca));
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let expired = f
        .builder(&["host.example"], 0, now - 60)
        .sign(&f.ca)
        .unwrap();
    let future = f
        .builder(&["host.example"], now + 86400, u64::MAX)
        .sign(&f.ca)
        .unwrap();
    f.rejected_unchanged("host.example", 22, expired.into());
    f.rejected_unchanged("host.example", 22, future.into());
}

#[test]
fn altered_certificate_signature_is_cryptographically_rejected() {
    let f = Fixture::new();
    f.write(&f.line("@cert-authority ", "host.example", &f.ca));
    let mut bytes = f.cert(&["host.example"]).to_bytes().unwrap();
    let algorithm_len = u32::from_be_bytes(bytes[..4].try_into().unwrap()) as usize;
    bytes[4 + algorithm_len + 4] ^= 1; // Alter the nonce without changing framing.
    let altered = Certificate::from_bytes(&bytes).unwrap();
    assert!(f.check(&altered, "host.example", 100).is_err());
    f.rejected_unchanged("host.example", 22, altered.into());
}

#[test]
fn certificate_pins_fail_closed_instead_of_bypassing_ca_verification() {
    let f = Fixture::new();
    let cert = f.cert(&["host.example"]);
    f.write(&format!("host.example {}\n", cert.to_openssh().unwrap()));
    f.rejected_unchanged("host.example", 22, cert.into());
}

#[test]
fn matching_ca_prefers_every_supported_leaf_certificate_not_just_ca_algorithm() {
    let f = Fixture::new();
    f.write(&f.line("@cert-authority ", "host.example", &f.ca));
    let prefs = preferred(&f.path(), &[endpoint("host.example", 22)]).unwrap();
    assert_eq!(
        prefs.host_key_certificates.as_ref(),
        prefs
            .key
            .iter()
            .filter(|algorithm| !matches!(algorithm, Algorithm::Rsa { hash: None }))
            .cloned()
            .collect::<Vec<_>>()
    );
    let names: Vec<_> = prefs
        .host_key_certificates
        .iter()
        .map(Algorithm::to_certificate_type)
        .collect();
    assert!(names.contains(&"ssh-ed25519-cert-v01@openssh.com".into()));
    assert!(names.contains(&"rsa-sha2-512-cert-v01@openssh.com".into()));
    assert!(names.contains(&"rsa-sha2-256-cert-v01@openssh.com".into()));
    assert!(!names.contains(&"ssh-rsa-cert-v01@openssh.com".into()));
    assert!(prefs.key.contains(&Algorithm::Rsa { hash: None }));
    for host in ["elsewhere.example", "[host.example]:2200"] {
        assert!(
            !preferred(&f.path(), &[host.into()])
                .unwrap()
                .host_key_certificates
                .is_empty()
        );
        assert!(
            verify(
                &f.path(),
                &[host.into()],
                "host.example",
                &f.cert(&["host.example"]).into()
            )
            .is_err()
        );
    }
    f.write(&format!(
        "{}{}",
        f.line("@cert-authority ", "host.example", &f.ca),
        f.line("@revoked ", "elsewhere.example", &f.ca)
    ));
    f.rejected_unchanged("host.example", 22, f.cert(&["host.example"]).into());
}

#[test]
fn rsa_leaf_signed_by_ed25519_ca_and_raw_rsa_preferences() {
    let mut f = Fixture::new();
    f.leaf = PrivateKey::random(&mut UnwrapErr(OsRng), Algorithm::Rsa { hash: None }).unwrap();
    f.write(&f.line("@cert-authority ", "host.example", &f.ca));
    f.verify("host.example", 22, f.cert(&["host.example"]).into())
        .unwrap();
    f.write(&f.line("", "host.example", &f.leaf));
    let prefs = preferred(&f.path(), &["host.example".into()]).unwrap();
    assert_eq!(
        prefs.key[0],
        Algorithm::Rsa {
            hash: Some(HashAlg::Sha512)
        }
    );
    assert_eq!(
        prefs.key[1],
        Algorithm::Rsa {
            hash: Some(HashAlg::Sha256)
        }
    );
    assert!(prefs.key.contains(&Algorithm::Rsa { hash: None }));
    assert!(prefs.host_key_certificates.is_empty());
    assert_eq!(
        prefs
            .key
            .iter()
            .filter(|algorithm| matches!(algorithm, Algorithm::Rsa { .. }))
            .count(),
        3
    );
    f.verify("host.example", 22, f.leaf.public_key().clone().into())
        .unwrap();
    std::mem::swap(&mut f.ca, &mut f.leaf);
    f.write(&f.line("@cert-authority ", "host.example", &f.ca));
    f.verify("host.example", 22, f.cert(&["host.example"]).into())
        .unwrap();
}

#[test]
fn raw_pin_preference_is_retained_and_duplicate_pins_are_deduplicated() {
    let f = Fixture::new();
    let mut rng = UnwrapErr(OsRng);
    let key = PrivateKey::random(
        &mut rng,
        Algorithm::Ecdsa {
            curve: russh::keys::ssh_key::EcdsaCurve::NistP256,
        },
    )
    .unwrap();
    let pin = f.line("", "host.example", &key);
    f.write(&format!("{pin}{pin}"));
    let prefs = preferred(&f.path(), &["host.example".into()]).unwrap();
    assert_eq!(prefs.key[0], key.algorithm());
    assert_eq!(
        prefs
            .key
            .iter()
            .filter(|algorithm| **algorithm == key.algorithm())
            .count(),
        1
    );
    assert!(prefs.host_key_certificates.is_empty());
    std::fs::remove_file(f.path()).unwrap();
    let prefs = preferred(&f.path(), &["host.example".into()]).unwrap();
    assert!(!prefs.host_key_certificates.is_empty());
    assert!(prefs.key.contains(&Algorithm::Rsa { hash: None }));
}

#[test]
fn ca_negotiation_obeys_wildcard_negation_and_hashed_port_scope() {
    let f = Fixture::new();
    let name = endpoint("host.example", 2200);
    let salt = vec![17; 20];
    let hash = Hmac::<Sha1>::new_from_slice(&salt)
        .unwrap()
        .chain_update(name.as_bytes())
        .finalize()
        .into_bytes()
        .into();
    let hashed = HostPatterns::HashedName { salt, hash }.to_string();
    for pattern in ["[*.example]:2200,![denied.example]:2200", &hashed] {
        f.write(&f.line("@cert-authority ", pattern, &f.ca));
        assert!(
            !preferred(&f.path(), std::slice::from_ref(&name))
                .unwrap()
                .host_key_certificates
                .is_empty()
        );
        for excluded in [
            "[denied.example]:2200",
            "host.example",
            "[host.example]:2201",
        ] {
            assert!(
                !preferred(&f.path(), &[excluded.into()])
                    .unwrap()
                    .host_key_certificates
                    .is_empty()
            );
            assert!(
                verify(
                    &f.path(),
                    &[excluded.into()],
                    "host.example",
                    &f.cert(&["host.example"]).into()
                )
                .is_err()
            );
        }
    }
}
