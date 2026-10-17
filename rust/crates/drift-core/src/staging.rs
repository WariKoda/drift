//! Interrupted transfer names are excluded on both sides of a comparison.
pub fn staging_name(base: &str) -> crate::error::Result<String> {
    if base.is_empty() || base.contains(['/', '\0']) || matches!(base, "." | "..") {
        return Err(crate::error::Error::Invalid(
            "invalid staging basename".into(),
        ));
    }
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|e| crate::error::Error::Invalid(format!("generate staging name: {e}")))?;
    let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    Ok(format!(".{base}.drift-tmp-{token}"))
}
pub fn is_staging_name(name: &str) -> bool {
    let Some((base, token)) = name.rsplit_once(".drift-tmp-") else {
        return false;
    };
    base.starts_with('.')
        && base.len() >= 2
        && token.len() == 32
        && token.bytes().all(|b| b.is_ascii_hexdigit())
}
