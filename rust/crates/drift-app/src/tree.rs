//! Loaded browser topology only. Listing I/O stays in the browser services;
//! FileList owns cursor/filter/marks independently of collapsed descendants.
use std::{collections::BTreeSet, path::Path};

#[derive(Clone, Debug)]
pub struct Node {
    pub path: String,
    pub directory: bool,
    pub depth: usize,
    pub expanded: bool,
}
#[derive(Default)]
pub struct FileTree {
    nodes: Vec<Node>,
    expanded_paths: BTreeSet<String>,
}
impl FileTree {
    pub fn new(entries: impl IntoIterator<Item = (String, bool)>) -> Self {
        Self {
            expanded_paths: BTreeSet::new(),
            nodes: entries
                .into_iter()
                .map(|(path, directory)| Node {
                    path,
                    directory,
                    depth: 0,
                    expanded: false,
                })
                .collect(),
        }
    }
    /// Keep expansion intent for directories temporarily hidden by visibility
    /// settings; explicit navigation constructs a fresh tree instead.
    pub fn reload(&mut self, entries: impl IntoIterator<Item = (String, bool)>) {
        let expanded_paths = std::mem::take(&mut self.expanded_paths);
        *self = Self::new(entries);
        self.expanded_paths = expanded_paths;
    }
    pub fn expanded_paths(&self) -> impl Iterator<Item = &String> {
        self.expanded_paths.iter()
    }
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }
    pub fn node(&self, path: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.path == path)
    }
    /// Only a currently loaded, collapsed directory can accept children.
    pub fn expand(
        &mut self,
        path: &str,
        entries: impl IntoIterator<Item = (String, bool)>,
    ) -> bool {
        let Some(index) = self
            .nodes
            .iter()
            .position(|n| n.path == path && n.directory && !n.expanded)
        else {
            return false;
        };
        let depth = self.nodes[index].depth + 1;
        self.nodes[index].expanded = true;
        self.expanded_paths.insert(path.to_owned());
        let children = entries.into_iter().map(|(path, directory)| Node {
            path,
            directory,
            depth,
            expanded: false,
        });
        self.nodes.splice(index + 1..index + 1, children);
        true
    }
    pub fn collapse(&mut self, path: &str) {
        self.expanded_paths
            .retain(|p| !Path::new(p).starts_with(path));
        let Some(index) = self.nodes.iter().position(|n| n.path == path && n.expanded) else {
            return;
        };
        let depth = self.nodes[index].depth;
        self.nodes[index].expanded = false;
        let end = self.nodes[index + 1..]
            .iter()
            .position(|n| n.depth <= depth)
            .map_or(self.nodes.len(), |i| index + 1 + i);
        self.nodes.drain(index + 1..end);
    }
    pub fn parent(&self, path: &str) -> Option<&str> {
        let parent = Path::new(path).parent()?.to_str()?;
        self.node(parent).map(|n| n.path.as_str())
    }
}
