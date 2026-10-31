use super::*;
use std::os::unix::fs::PermissionsExt;

#[test]
fn certificate_pins_parse_but_cannot_replace_authorities_or_trust_the_raw_leaf() {
    let f = Fixture::new();
    let cert = f.cert(&["host.example"]);
    let pin = format!("host.example {}\n", cert.to_openssh().unwrap());
    f.write(&pin);
    f.rejected_unchanged("host.example", 22, cert.clone().into());
    f.rejected_unchanged("host.example", 22, f.leaf.public_key().clone().into());
    let text = format!("{pin}{}", f.line("@cert-authority ", "host.example", &f.ca));
    f.write(&text);
    f.verify("host.example", 22, cert.into()).unwrap();
    assert_eq!(std::fs::read_to_string(f.path()).unwrap(), text);
    assert!(preferred(&f.path(), &["host.example".into()]).is_ok());
}
#[test]
fn certificate_encoded_revocations_reject_the_leaf_and_all_its_certificates() {
    let f = Fixture::new();
    let cert = f.cert(&["host.example"]);
    let revoked = format!(
        "{}@revoked unrelated.example {}\n",
        f.line("@cert-authority ", "host.example", &f.ca),
        cert.to_openssh().unwrap()
    );
    f.write(&revoked);
    f.rejected_unchanged("host.example", 22, cert.into());
    f.rejected_unchanged("host.example", 22, f.cert(&["host.example"]).into());
    f.rejected_unchanged("host.example", 22, f.leaf.public_key().clone().into());
}
#[test]
fn certificates_verify_readonly_trust_and_untrusted_certificates_create_nothing() {
    let f = Fixture::new();
    f.write(&f.line("@cert-authority ", "host.example", &f.ca));
    std::fs::set_permissions(f.path(), std::fs::Permissions::from_mode(0o400)).unwrap();
    f.verify("host.example", 22, f.cert(&["host.example"]).into())
        .unwrap();
    assert_eq!(
        std::fs::metadata(f.path()).unwrap().permissions().mode() & 0o777,
        0o400
    );
    std::fs::remove_file(f.path()).unwrap();
    let path = f.dir.path().join("missing-parent/known_hosts");
    assert!(
        verify(
            &path,
            &["host.example".into()],
            "host.example",
            &f.cert(&["host.example"]).into()
        )
        .is_err()
    );
    assert!(!path.parent().unwrap().exists());
}
