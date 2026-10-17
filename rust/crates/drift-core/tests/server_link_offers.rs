use drift_core::{
    config::{Auth, Defaults, GlobalConfig, Host, ProjectConfig},
    error::Error,
    pathmap::Mapping,
    store::{LinkTarget, Store},
};
use fs2::FileExt;
use std::{fs, os::unix::fs::PermissionsExt};

fn host(name: &str) -> Host {
    Host {
        name: name.into(),
        hostname: "deploy.example".into(),
        user: "deploy".into(),
        root_path: format!("/srv/{name}"),
        ..Host::default()
    }
}
fn global(store: &Store, config: &GlobalConfig) {
    fs::write(
        store.dir().join("config.toml"),
        toml::to_string(config).unwrap(),
    )
    .unwrap();
}
fn project(store: &Store, slug: &str, config: &ProjectConfig) {
    fs::create_dir_all(store.dir().join("projects")).unwrap();
    fs::write(
        store.dir().join(format!("projects/{slug}.toml")),
        toml::to_string(config).unwrap(),
    )
    .unwrap();
}
fn source_target(store: &Store) -> LinkTarget {
    store
        .link_targets("dest")
        .unwrap()
        .targets
        .into_iter()
        .find(|t| t.project.as_deref() == Some("source"))
        .unwrap()
}
fn setup_source(store: &Store) {
    project(
        store,
        "source",
        &ProjectConfig {
            hosts: vec![host("prod")],
            ..ProjectConfig::default()
        },
    );
}

#[test]
fn discovery_is_readonly_sorted_and_compares_effective_endpoints_not_credentials() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    let mut server = host("z");
    server.user.clear();
    server.auth = Auth {
        kind: "password".into(),
        password: "server-secret".into(),
        ..Auth::default()
    };
    server.keep_alive_interval = Some(0);
    server.protocol = "sftp".into();
    server.port = 22;
    server.hostname = "DEPLOY.EXAMPLE".into();
    global(
        &store,
        &GlobalConfig {
            defaults: Defaults {
                user: "deploy".into(),
                port: 22,
            },
            hosts: vec![
                server,
                host("a"),
                Host {
                    hostname: "elsewhere".into(),
                    ..host("no")
                },
            ],
            ..GlobalConfig::default()
        },
    );
    for slug in ["z-project", "a-project"] {
        project(
            &store,
            slug,
            &ProjectConfig {
                defaults: Defaults {
                    user: "deploy".into(),
                    port: 22,
                },
                hosts: vec![
                    Host {
                        user: String::new(),
                        ..host("z")
                    },
                    host("a"),
                    Host {
                        name: "linked".into(),
                        server: "a".into(),
                        ..Host::default()
                    },
                ],
                ..ProjectConfig::default()
            },
        );
    }
    project(
        &store,
        "dest",
        &ProjectConfig {
            defaults: Defaults {
                user: "deploy".into(),
                port: 22,
            },
            hosts: vec![host("own")],
            ..ProjectConfig::default()
        },
    );
    let paths = [
        "config.toml",
        "projects/dest.toml",
        "projects/a-project.toml",
        "projects/z-project.toml",
    ];
    let before: Vec<_> = paths
        .iter()
        .map(|p| fs::read(dir.path().join(p)).unwrap())
        .collect();
    let draft = Host {
        user: String::new(),
        ..host("unsaved")
    };
    let catalog = store.matching_link_targets("dest", &draft).unwrap();
    assert_eq!(
        catalog
            .targets
            .iter()
            .map(|t| (t.project.as_deref().unwrap_or(""), t.host.name.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("", "a"),
            ("", "z"),
            ("a-project", "a"),
            ("a-project", "z"),
            ("z-project", "a"),
            ("z-project", "z")
        ]
    );
    for (path, bytes) in paths.iter().zip(before) {
        assert_eq!(fs::read(dir.path().join(path)).unwrap(), bytes);
    }
    assert_eq!(store.project("dest").unwrap().hosts.len(), 1);
    assert!(catalog.targets[1].host.auth != draft.auth);
    assert_eq!(catalog.targets[1].host.keep_alive_interval, Some(0));
    let linked = Host {
        server: "a".into(),
        hostname: String::new(),
        user: String::new(),
        ..draft
    };
    assert!(store.matching_link_targets("dest", &linked).is_err());
}

#[test]
fn endpoint_matching_uses_default_ports_exact_user_protocol_and_no_unicode_expansion() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    let cases = [
        ("Example", "EXAMPLE", "", "sftp", 0, 22, "u", "u", true),
        ("example", "example", "ftp", "ftp", 0, 21, "u", "u", true),
        ("example", "example", "ftps", "ftps", 0, 21, "u", "u", true),
        ("example", "example", "ftp", "ftps", 21, 21, "u", "u", false),
        ("example", "example", "", "sftp", 22, 23, "u", "u", false),
        ("example", "example", "", "sftp", 0, 22, "u", "U", false),
        (
            "example", "example", "sftp", "SFTP", 22, 22, "u", "u", false,
        ),
        ("", "", "", "", 0, 0, "u", "u", false),
        ("Straße", "STRASSE", "", "", 0, 0, "u", "u", false),
        ("K.example", "k.example", "", "", 0, 0, "u", "u", false),
        ("ſ.example", "s.example", "", "", 0, 0, "u", "u", false),
        (
            "İ.example",
            "i\u{307}.example",
            "",
            "",
            0,
            0,
            "u",
            "u",
            false,
        ),
        ("É.example", "é.example", "", "", 0, 0, "u", "u", false),
        ("é.example", "é.example", "", "", 0, 0, "u", "u", true),
        ("example.", "example", "", "", 0, 0, "u", "u", false),
    ];
    for (a, b, pa, pb, porta, portb, ua, ub, matches) in cases {
        let desired = Host {
            hostname: a.into(),
            protocol: pa.into(),
            port: porta,
            user: ua.into(),
            ..host("dest")
        };
        let server = Host {
            hostname: b.into(),
            protocol: pb.into(),
            port: portb,
            user: ub.into(),
            ..host("server")
        };
        global(
            &store,
            &GlobalConfig {
                hosts: vec![server],
                ..GlobalConfig::default()
            },
        );
        assert_eq!(
            !store
                .matching_link_targets("dest", &desired)
                .unwrap()
                .targets
                .is_empty(),
            matches,
            "{a} / {b}, {pa} / {pb}"
        );
        assert!(!dir.path().join("projects/dest.toml").exists());
    }
}

#[test]
fn accepting_global_server_only_writes_destination_and_preserves_draft_scope() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    let server = Host {
        user: String::new(),
        auth: Auth {
            kind: "password".into(),
            password: "chosen-secret".into(),
            ..Auth::default()
        },
        keep_alive_interval: Some(0),
        ..host("shared")
    };
    global(
        &store,
        &GlobalConfig {
            defaults: Defaults {
                port: 2222,
                user: "deploy".into(),
            },
            hosts: vec![server],
            ..GlobalConfig::default()
        },
    );
    let original = Host {
        port: 2222,
        ..host("before")
    };
    store
        .save_host(Some("dest"), None, original.clone())
        .unwrap();
    let desired = Host {
        name: "after".into(),
        root_path: "/different/root".into(),
        mappings: vec![Mapping {
            local: "src".into(),
            remote: "app".into(),
        }],
        auth: Auth {
            kind: "password".into(),
            password: "draft-secret".into(),
            ..Auth::default()
        },
        keep_alive_interval: Some(120),
        ..original.clone()
    };
    let bytes = fs::read(dir.path().join("config.toml")).unwrap();
    let target = store
        .matching_link_targets("dest", &desired)
        .unwrap()
        .targets
        .remove(0);
    assert!(store.project("dest").unwrap().hosts[0] == original);
    let saved = store
        .save_linked_host("dest", Some(&original), desired.clone(), &target)
        .unwrap();
    assert!(saved.warning.is_none() && saved.destination_error.is_none());
    assert_eq!(fs::read(dir.path().join("config.toml")).unwrap(), bytes);
    assert_eq!(saved.server.name, "shared");
    assert_eq!(saved.server.port, 2222);
    let raw = store.project("dest").unwrap().hosts.remove(0);
    assert!(
        raw == Host {
            name: desired.name.clone(),
            server: "shared".into(),
            root_path: desired.root_path.clone(),
            mappings: desired.mappings.clone(),
            ..Host::default()
        }
    );
    let resolved = store.runtime(Some("dest")).unwrap().hosts.remove(0);
    assert_eq!(resolved.auth.password, "chosen-secret");
    assert_eq!(resolved.keep_alive_interval, Some(0));
    assert_eq!(resolved.root_path, desired.root_path);
    assert_eq!(resolved.mappings, desired.mappings);
}

#[test]
fn project_acceptance_promotes_once_avoids_collisions_and_preserves_both_scopes() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    global(
        &store,
        &GlobalConfig {
            defaults: Defaults {
                port: 2222,
                user: "global".into(),
            },
            hosts: ["prod", "source-prod", "source-prod-2"]
                .into_iter()
                .map(|name| Host {
                    hostname: "elsewhere".into(),
                    ..host(name)
                })
                .collect(),
            ..GlobalConfig::default()
        },
    );
    let original = Host {
        user: String::new(),
        protocol: "ftps".into(),
        auth: Auth {
            kind: "password".into(),
            password: "source-secret".into(),
            ..Auth::default()
        },
        keep_alive_interval: Some(0),
        mappings: vec![Mapping {
            local: "src".into(),
            remote: "source-app".into(),
        }],
        ..host("prod")
    };
    let fallback = vec![Mapping {
        local: "fallback".into(),
        remote: "fallback".into(),
    }];
    project(
        &store,
        "source",
        &ProjectConfig {
            defaults: Defaults {
                port: 0,
                user: "deploy".into(),
            },
            hosts: vec![original.clone(), host("unrelated")],
            mappings: fallback.clone(),
        },
    );
    project(
        &store,
        "dest",
        &ProjectConfig {
            mappings: fallback.clone(),
            ..ProjectConfig::default()
        },
    );
    let desired = Host {
        protocol: "ftps".into(),
        root_path: "/dest".into(),
        mappings: vec![Mapping {
            local: "web".into(),
            remote: "destination-app".into(),
        }],
        ..host("destination")
    };
    let target = store
        .matching_link_targets("dest", &desired)
        .unwrap()
        .targets
        .remove(0);
    let saved = store
        .save_linked_host("dest", None, desired.clone(), &target)
        .unwrap();
    assert!(saved.warning.is_none() && saved.destination_error.is_none());
    assert_eq!(saved.server.name, "source-prod-3");
    assert_eq!(saved.server.port, 21);
    assert_eq!(saved.server.user, "deploy");
    assert!(saved.server.auth == original.auth);
    assert_eq!(saved.server.keep_alive_interval, Some(0));
    assert!(saved.server.mappings.is_empty());
    let source = store.project("source").unwrap();
    assert_eq!(source.hosts.len(), 2);
    assert!(
        source.hosts[0]
            == Host {
                name: original.name,
                server: saved.server.name.clone(),
                root_path: original.root_path,
                mappings: original.mappings,
                ..Host::default()
            }
    );
    assert_eq!(source.mappings, fallback);
    let destination = store.project("dest").unwrap();
    assert!(
        destination.hosts[0]
            == Host {
                name: desired.name,
                server: saved.server.name.clone(),
                root_path: desired.root_path,
                mappings: desired.mappings,
                ..Host::default()
            }
    );
    assert_eq!(destination.mappings, fallback);
    assert!(matches!(
        store.save_linked_host("dest", None, host("another"), &target),
        Err(Error::Conflict(_))
    ));
    assert_eq!(store.global().unwrap().hosts.len(), 4);
}

#[test]
fn stale_source_and_defaults_snapshots_are_rejected_before_global_commit() {
    for defaults_changed in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().into());
        setup_source(&store);
        let target = source_target(&store);
        let mut source = store.project("source").unwrap();
        if defaults_changed {
            // Even an overridden default is part of the exact candidate snapshot.
            source.defaults.user = "not-used".into();
        } else {
            source.hosts[0].auth.password = "changed-secret".into();
        }
        project(&store, "source", &source);
        let bytes = fs::read(dir.path().join("projects/source.toml")).unwrap();
        assert!(matches!(
            store.save_linked_host("dest", None, host("dest"), &target),
            Err(Error::Conflict(_))
        ));
        assert!(!dir.path().join("config.toml").exists());
        assert!(!dir.path().join("projects/dest.toml").exists());
        assert_eq!(
            fs::read(dir.path().join("projects/source.toml")).unwrap(),
            bytes
        );
    }
}

#[test]
fn stale_global_and_default_snapshots_are_rejected_without_writes() {
    for defaults_changed in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().into());
        global(
            &store,
            &GlobalConfig {
                hosts: vec![host("shared")],
                ..GlobalConfig::default()
            },
        );
        let target = store.link_targets("dest").unwrap().targets.remove(0);
        let mut config = store.global().unwrap();
        if defaults_changed {
            config.defaults.user = "not-used".into();
        } else {
            config.hosts[0].auth.password = "changed-secret".into();
        }
        global(&store, &config);
        let bytes = fs::read(dir.path().join("config.toml")).unwrap();
        assert!(matches!(
            store.save_linked_host("dest", None, host("dest"), &target),
            Err(Error::Conflict(_))
        ));
        assert_eq!(fs::read(dir.path().join("config.toml")).unwrap(), bytes);
        assert!(!dir.path().join("projects/dest.toml").exists());
    }
}

#[test]
fn destination_version_collision_invalid_draft_and_endpoint_changes_prevent_promotion() {
    for failure in [
        "version",
        "collision",
        "keepalive",
        "mapping",
        "link",
        "endpoint",
        "defaults",
        "source-config",
        "dest-config",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().into());
        setup_source(&store);
        let target = source_target(&store);
        let original = host("before");
        store
            .save_host(Some("dest"), None, original.clone())
            .unwrap();
        let mut desired = Host {
            name: "after".into(),
            ..original.clone()
        };
        match failure {
            "version" => {
                let mut changed = original.clone();
                changed.root_path = "/changed".into();
                store
                    .save_host(Some("dest"), Some(&original), changed)
                    .unwrap();
            }
            "collision" => {
                store.save_host(Some("dest"), None, host("after")).unwrap();
            }
            "keepalive" => desired.keep_alive_interval = Some(-1),
            "mapping" => desired.mappings.push(Mapping {
                local: "../outside".into(),
                remote: "app".into(),
            }),
            "link" => {
                desired = Host {
                    server: "already-linked".into(),
                    name: "after".into(),
                    ..Host::default()
                };
            }
            "endpoint" => desired.hostname = "changed.example".into(),
            "defaults" => {
                desired.user.clear();
                let mut config = store.project("dest").unwrap();
                config.defaults.user = "other-account".into();
                project(&store, "dest", &config);
            }
            "source-config" => {
                let mut config = store.project("source").unwrap();
                config.hosts.push(Host {
                    keep_alive_interval: Some(-1),
                    ..host("invalid")
                });
                project(&store, "source", &config);
            }
            "dest-config" => {
                let mut config = store.project("dest").unwrap();
                config.mappings.push(Mapping {
                    local: "../outside".into(),
                    remote: "app".into(),
                });
                project(&store, "dest", &config);
            }
            _ => unreachable!(),
        }
        let before_source = fs::read(dir.path().join("projects/source.toml")).unwrap();
        let before_dest = fs::read(dir.path().join("projects/dest.toml")).unwrap();
        let result = store.save_linked_host("dest", Some(&original), desired, &target);
        if matches!(failure, "version" | "endpoint" | "defaults") {
            assert!(matches!(result, Err(Error::Conflict(_))), "{failure}");
        } else {
            assert!(result.is_err(), "{failure}");
        }
        assert!(!dir.path().join("config.toml").exists(), "{failure}");
        assert_eq!(
            fs::read(dir.path().join("projects/source.toml")).unwrap(),
            before_source
        );
        assert_eq!(
            fs::read(dir.path().join("projects/dest.toml")).unwrap(),
            before_dest
        );
    }
}

#[test]
fn acceptance_rechecks_destination_defaults_after_discovery() {
    for promotion in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().into());
        if promotion {
            setup_source(&store);
        } else {
            global(
                &store,
                &GlobalConfig {
                    hosts: vec![host("shared")],
                    ..GlobalConfig::default()
                },
            );
        }
        project(
            &store,
            "dest",
            &ProjectConfig {
                defaults: Defaults {
                    port: 22,
                    user: "deploy".into(),
                },
                ..ProjectConfig::default()
            },
        );
        let desired = Host {
            user: String::new(),
            ..host("dest")
        };
        let target = store
            .matching_link_targets("dest", &desired)
            .unwrap()
            .targets
            .remove(0);
        let before_global = fs::read(dir.path().join("config.toml")).ok();
        let mut config = store.project("dest").unwrap();
        config.defaults.port = 2222;
        project(&store, "dest", &config);
        assert!(matches!(
            store.save_linked_host("dest", None, desired, &target),
            Err(Error::Conflict(_))
        ));
        assert_eq!(fs::read(dir.path().join("config.toml")).ok(), before_global);
        assert!(store.project("dest").unwrap().hosts.is_empty());
    }
}

#[test]
fn promotion_cannot_silently_inherit_a_different_global_default_account() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    global(
        &store,
        &GlobalConfig {
            defaults: Defaults {
                user: "other-account".into(),
                port: 0,
            },
            ..GlobalConfig::default()
        },
    );
    project(
        &store,
        "source",
        &ProjectConfig {
            hosts: vec![Host {
                user: String::new(),
                ..host("prod")
            }],
            ..ProjectConfig::default()
        },
    );
    let desired = Host {
        user: String::new(),
        ..host("dest")
    };
    let target = store
        .matching_link_targets("dest", &desired)
        .unwrap()
        .targets
        .remove(0);
    assert!(matches!(
        store.save_linked_host("dest", None, desired, &target),
        Err(Error::Conflict(_))
    ));
    assert!(store.global().unwrap().hosts.is_empty());
}

#[test]
fn acceptance_obeys_shared_lock_and_global_write_failure_has_no_commit_outcome() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    setup_source(&store);
    let target = source_target(&store);
    let lock = fs::OpenOptions::new()
        .write(true)
        .open(dir.path().join("write.lock"))
        .unwrap();
    lock.lock_exclusive().unwrap();
    assert!(matches!(
        store.save_linked_host("dest", None, host("dest"), &target),
        Err(Error::Busy)
    ));
    lock.unlock().unwrap();
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o500)).unwrap();
    let result = store.save_linked_host("dest", None, host("dest"), &target);
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
    assert!(matches!(result, Err(Error::Io(_))));
    assert!(!dir.path().join("config.toml").exists());
    assert!(store.project("source").unwrap().hosts[0].server.is_empty());
    assert!(!dir.path().join("projects/dest.toml").exists());
}

#[test]
fn source_io_failure_still_attempts_destination_and_reports_committed_server_without_retry() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    setup_source(&store);
    let target = source_target(&store);
    let desired = host("destination");
    let projects = dir.path().join("projects");
    fs::set_permissions(&projects, fs::Permissions::from_mode(0o500)).unwrap();
    let result = store.save_linked_host("dest", None, desired.clone(), &target);
    fs::set_permissions(&projects, fs::Permissions::from_mode(0o700)).unwrap();
    let saved = result.unwrap();
    assert_eq!(saved.server.name, "prod");
    assert!(
        saved
            .warning
            .as_ref()
            .unwrap()
            .contains("keeps its own copy")
    );
    assert!(matches!(saved.destination_error, Some(Error::Io(_))));
    assert_eq!(store.global().unwrap().hosts.len(), 1);
    assert!(store.project("source").unwrap().hosts[0].server.is_empty());
    assert!(!dir.path().join("projects/dest.toml").exists());
    // An explicit future save uses the committed server, not the old offer.
    let link = Host {
        name: desired.name,
        server: saved.server.name,
        root_path: desired.root_path,
        mappings: desired.mappings,
        ..Host::default()
    };
    store.save_host(Some("dest"), None, link).unwrap();
    assert_eq!(store.global().unwrap().hosts.len(), 1);
    assert_eq!(store.project("dest").unwrap().hosts[0].server, "prod");
}

#[test]
fn destination_io_failure_after_promotion_reports_server_and_typed_error() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    setup_source(&store);
    let target = source_target(&store);
    let desired = host("destination");
    store
        .save_host(Some("dest"), None, desired.clone())
        .unwrap();
    let source = store.project("source").unwrap();
    let source_path = dir.path().join("projects/source.toml");
    fs::remove_file(&source_path).unwrap();
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&source_path)
            .status()
            .unwrap()
            .success()
    );
    let worker_store = store.clone();
    let worker = std::thread::spawn(move || {
        worker_store.save_linked_host("dest", Some(&desired), desired.clone(), &target)
    });
    // The source read follows destination validation. A real filesystem change
    // now makes only the final atomic destination replacement fail.
    let mut writer = fs::OpenOptions::new()
        .write(true)
        .open(source_path)
        .unwrap();
    let destination_path = dir.path().join("projects/dest.toml");
    fs::remove_file(&destination_path).unwrap();
    fs::create_dir(&destination_path).unwrap();
    writer
        .write_all(toml::to_string(&source).unwrap().as_bytes())
        .unwrap();
    drop(writer);
    let saved = worker.join().unwrap().unwrap();
    assert_eq!(saved.server.name, "prod");
    assert!(saved.warning.is_none());
    assert!(matches!(saved.destination_error, Some(Error::Io(_))));
    assert_eq!(store.global().unwrap().hosts.len(), 1);
    assert_eq!(store.project("source").unwrap().hosts[0].server, "prod");
    assert!(destination_path.is_dir());
}

#[test]
fn global_acceptance_io_failure_is_an_ordinary_error_without_other_commits() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    global(
        &store,
        &GlobalConfig {
            hosts: vec![host("shared")],
            ..GlobalConfig::default()
        },
    );
    fs::create_dir(dir.path().join("projects")).unwrap();
    let target = store.link_targets("dest").unwrap().targets.remove(0);
    let before = fs::read(dir.path().join("config.toml")).unwrap();
    fs::set_permissions(
        dir.path().join("projects"),
        fs::Permissions::from_mode(0o500),
    )
    .unwrap();
    let result = store.save_linked_host("dest", None, host("dest"), &target);
    fs::set_permissions(
        dir.path().join("projects"),
        fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    assert!(matches!(result, Err(Error::Io(_))));
    assert_eq!(fs::read(dir.path().join("config.toml")).unwrap(), before);
    assert!(!dir.path().join("projects/dest.toml").exists());
}
