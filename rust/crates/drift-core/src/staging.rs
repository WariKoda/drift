//! Interrupted transfer names are excluded on both sides of a comparison.
pub fn is_staging_name(name: &str) -> bool {
    let Some((base, token)) = name.rsplit_once(".drift-tmp-") else {
        return false;
    };
    base.starts_with('.')
        && base.len() >= 2
        && token.len() == 32
        && token.bytes().all(|b| b.is_ascii_hexdigit())
}
