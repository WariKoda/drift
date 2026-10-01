use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use toml::value::Datetime;

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Project {
    pub slug: String,
    pub name: String,
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub archived: bool,
    pub created_at: Datetime,
    pub updated_at: Datetime,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opened_at: Option<Datetime>,
}
#[derive(Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct Registry {
    pub projects: Vec<Project>,
}
impl Registry {
    pub fn find(&self, slug: &str) -> Option<&Project> {
        self.projects.iter().find(|p| p.slug == slug)
    }
    pub fn find_by_path(&self, path: &Path) -> Option<&Project> {
        let path = path.components().collect::<PathBuf>();
        self.projects
            .iter()
            .filter(|p| path.starts_with(&p.path))
            .max_by_key(|p| p.path.as_os_str().len())
    }
    pub fn active(&self) -> Vec<&Project> {
        let mut entries: Vec<_> = self.projects.iter().filter(|p| !p.archived).collect();
        entries.sort_by(|a, b| {
            let timestamp = |p: &Project| {
                p.opened_at
                    .as_ref()
                    .and_then(|date| chrono::DateTime::parse_from_rfc3339(&date.to_string()).ok())
            };
            timestamp(b)
                .cmp(&timestamp(a))
                .then(a.name.cmp(&b.name))
                .then(a.slug.cmp(&b.slug))
        });
        entries
    }
    pub fn unique_slug(&self, name: &str) -> String {
        let mut base = String::new();
        for ch in name.trim().to_lowercase().chars() {
            if ch.is_ascii_alphanumeric() {
                base.push(ch);
            } else if !base.is_empty() && !base.ends_with('-') {
                base.push('-');
            }
        }
        let base = base.trim_end_matches('-');
        let base = if base.is_empty() { "project" } else { base };
        if self.find(base).is_none() {
            return base.into();
        }
        for n in 2.. {
            let slug = format!("{base}-{n}");
            if self.find(&slug).is_none() {
                return slug;
            }
        }
        unreachable!()
    }
    pub fn match_query(&self, query: &str) -> Result<&Project> {
        let query = query.trim();
        if query.is_empty() {
            return Err(Error::Invalid("project query must not be empty".into()));
        }
        if let Some(p) = self.find(query) {
            return Ok(p);
        }
        let lower = query.to_lowercase();
        for kind in 0..3 {
            let hits: Vec<_> = self
                .projects
                .iter()
                .filter(|p| {
                    let name = p.name.to_lowercase();
                    let slug = p.slug.to_lowercase();
                    match kind {
                        0 => name == lower,
                        1 => name.starts_with(&lower) || slug.starts_with(&lower),
                        _ => name.contains(&lower) || slug.contains(&lower),
                    }
                })
                .collect();
            match hits.as_slice() {
                [p] => return Ok(p),
                [] => {}
                _ => return Err(Error::Invalid(format!("ambiguous project {query:?}"))),
            }
        }
        Err(Error::Invalid(format!("no project matching {query:?}")))
    }
    pub fn validate(&self) -> Result<()> {
        let mut slugs = std::collections::HashSet::new();
        let mut paths = std::collections::HashSet::new();
        for p in &self.projects {
            crate::config::project_store_path(Path::new("."), &p.slug)?;
            if p.name.trim().is_empty()
                || !p.path.is_absolute()
                || !slugs.insert(&p.slug)
                || !paths.insert(&p.path)
            {
                return Err(Error::Invalid(format!(
                    "invalid or duplicate project {:?}",
                    p.slug
                )));
            }
        }
        Ok(())
    }
}
pub fn now() -> Datetime {
    chrono::Utc::now()
        .to_rfc3339()
        .parse()
        .expect("RFC3339 is a TOML datetime")
}
