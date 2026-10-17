//! Build-time version policy shared with parser tests, not a runtime setting.
pub fn checked_version(value: &str) -> Option<&str> {
    if value.len() > 128 {
        return None;
    }
    let parsed = semver::Version::parse(value).ok()?;
    if [parsed.major, parsed.minor, parsed.patch]
        .into_iter()
        .any(|part| part > 65535)
    {
        return None;
    }
    Some(value)
}
