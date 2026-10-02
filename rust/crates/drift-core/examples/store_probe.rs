//! Process entry for Go/Rust interoperability tests; no GUI or real user stores.
use drift_core::{
    config::Host,
    error::{Error, Result},
    project::now,
    store::Store,
    tlstrust::{Manager, Problem, TrustedCertificate},
};
use std::path::PathBuf;
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        return Err(Error::Invalid(
            "usage: store_probe <config-dir> <operation> [name]".into(),
        ));
    }
    let store = Store::new(PathBuf::from(&args[0]));
    match args[1].as_str() {
        "hold" => {
            use fs2::FileExt;
            use std::io::{Read, Write};
            std::fs::create_dir_all(store.dir())?;
            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(store.dir().join("write.lock"))?;
            file.lock_exclusive()?;
            std::io::stdout().write_all(b"locked\n")?;
            std::io::stdout().flush()?;
            let mut release = [0u8; 1];
            std::io::stdin().read_exact(&mut release)?;
            Ok(())
        }
        "host" => store.save_host(
            None,
            None,
            Host {
                name: args
                    .get(2)
                    .ok_or_else(|| Error::Invalid("missing name".into()))?
                    .clone(),
                hostname: "rust.example".into(),
                keep_alive_interval: Some(0),
                ..Host::default()
            },
        ),
        "trust" => store.save_trusted_certificate(
            None,
            TrustedCertificate {
                protocol: "ftps".into(),
                hostname: args
                    .get(2)
                    .ok_or_else(|| Error::Invalid("missing hostname".into()))?
                    .clone(),
                port: 21,
                fingerprint: vec!["AB"; 32].join(":"),
                problems: vec![Problem::UnknownAuthority, Problem::Expired],
                trusted_at: now(),
            },
        ),
        "reset-trust" => {
            let hostname = args
                .get(2)
                .ok_or_else(|| Error::Invalid("missing hostname".into()))?;
            let certificate = store
                .trusted_certificates()?
                .into_iter()
                .find(|entry| &entry.hostname == hostname)
                .ok_or_else(|| Error::Invalid("missing trust record".into()))?;
            let manager = Manager::new(store);
            manager.reset(&manager.inspect(certificate.endpoint())?)
        }
        "roundtrip" => {
            for entry in store.trusted_certificates()? {
                store.save_trusted_certificate(Some(&entry), entry.clone())?;
            }

            let registry = store.registry()?;
            for p in registry.projects {
                store.save_project(Some(&p), p.clone())?;
            }
            let global = store.global()?;
            for host in global.hosts {
                store.save_host(None, Some(&host), host.clone())?;
            }
            let slug = args.get(2).map_or("shop", String::as_str);
            let project = store.project(slug)?;
            for host in project.hosts {
                store.save_host(Some(slug), Some(&host), host.clone())?;
            }
            Ok(())
        }
        "targets" => {
            let current = args
                .get(2)
                .ok_or_else(|| Error::Invalid("missing current project".into()))?;
            for target in store.link_targets(current)?.targets {
                println!(
                    "{}|{}|{}|{}|{}|{}|{}",
                    target.project.as_deref().unwrap_or(""),
                    target.host.name,
                    target.host.hostname,
                    target.host.port,
                    target.host.user,
                    if target.host.protocol.is_empty() {
                        "sftp"
                    } else {
                        &target.host.protocol
                    },
                    target.used_by.join(",")
                );
            }
            Ok(())
        }
        "promote" | "promote-paused" => {
            let source = args
                .get(2)
                .ok_or_else(|| Error::Invalid("missing source project".into()))?;
            let name = args
                .get(3)
                .ok_or_else(|| Error::Invalid("missing host name".into()))?;
            let current = args
                .get(4)
                .ok_or_else(|| Error::Invalid("missing current project".into()))?;
            let target = store
                .link_targets(current)?
                .targets
                .into_iter()
                .find(|target| target.project.as_ref() == Some(source) && &target.host.name == name)
                .ok_or_else(|| Error::Invalid("missing link target".into()))?;
            if args[1] == "promote-paused" {
                use std::io::{Read, Write};
                std::io::stdout().write_all(b"selected\n")?;
                std::io::stdout().flush()?;
                std::io::stdin().read_exact(&mut [0u8; 1])?;
            }
            let promotion = store.select_link_target(current, &target)?;
            if let Some(warning) = promotion.warning {
                return Err(Error::Invalid(warning));
            }
            println!("{}", promotion.server.name);
            Ok(())
        }
        "rename-project" => {
            let registry = store.registry()?;
            let p = registry
                .find("shop")
                .ok_or_else(|| Error::Invalid("missing shop".into()))?;
            let mut edited = p.clone();
            edited.name = "Rust edit".into();
            edited.updated_at = now();
            store.save_project(Some(p), edited)
        }
        _ => Err(Error::Invalid("unknown operation".into())),
    }
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
