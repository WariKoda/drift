mod build_support;

fn main() {
    println!("cargo:rerun-if-env-changed=DRIFT_GUI_VERSION");
    let value = match std::env::var("DRIFT_GUI_VERSION") {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => {
            std::env::var("CARGO_PKG_VERSION").expect("Cargo package version is required")
        }
        Err(std::env::VarError::NotUnicode(_)) => {
            panic!("DRIFT_GUI_VERSION must be Unicode SemVer")
        }
    };
    let version = build_support::checked_version(&value)
        .expect("DRIFT_GUI_VERSION must be SemVer (no v prefix), at most 128 bytes, with core components at most 65535");
    println!("cargo:rustc-env=DRIFT_GUI_VERSION={version}");
}
