use drift_core::{
    pathmap::{Mapper, Mapping},
    staging::is_staging_name,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct MappingFixture {
    cases: Vec<MappingCase>,
}
#[derive(Deserialize)]
struct MappingCase {
    name: String,
    project_root: String,
    local: String,
    remote: String,
    #[serde(default)]
    root_path: String,
    #[serde(default)]
    project_mappings: Vec<Mapping>,
    #[serde(default)]
    host_mappings: Vec<Mapping>,
    #[serde(default)]
    error: bool,
}
#[test]
fn shared_go_mapping_fixtures() {
    let fixture: MappingFixture =
        toml::from_str(include_str!("../../../../testdata/parity/pathmap.toml")).unwrap();
    assert!(!fixture.cases.is_empty());
    for case in fixture.cases {
        let mapper = Mapper::new(
            &case.project_root,
            &case.root_path,
            &case.project_mappings,
            &case.host_mappings,
        );
        if case.error {
            if let Ok(mapper) = mapper {
                assert!(
                    mapper.local_to_remote(&case.local).is_err(),
                    "{}",
                    case.name
                );
                assert!(
                    mapper.remote_to_local(&case.remote).is_err(),
                    "{}",
                    case.name
                );
            }
        } else {
            let mapper = mapper.unwrap();
            assert_eq!(
                mapper.local_to_remote(&case.local).unwrap(),
                case.remote,
                "{}",
                case.name
            );
            assert_eq!(
                mapper.remote_to_local(&case.remote).unwrap(),
                case.local,
                "{}",
                case.name
            );
        }
    }
}
#[test]
fn shared_go_staging_fixtures() {
    #[derive(Deserialize)]
    struct Fixture {
        cases: Vec<Case>,
    }
    #[derive(Deserialize)]
    struct Case {
        name: String,
        staging: bool,
    }
    let fixture: Fixture =
        toml::from_str(include_str!("../../../../testdata/parity/staging.toml")).unwrap();
    assert!(!fixture.cases.is_empty());
    for case in fixture.cases {
        assert_eq!(is_staging_name(&case.name), case.staging, "{}", case.name);
    }
}
