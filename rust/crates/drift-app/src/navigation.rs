//! Committed browser history. Failed or cancelled loads never enter it.
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct History {
    paths: Vec<PathBuf>,
    cursor: usize,
}
impl History {
    pub fn reset(&mut self, path: PathBuf) {
        self.paths = vec![path];
        self.cursor = 0;
    }
    pub fn push(&mut self, path: PathBuf) {
        if self.paths.get(self.cursor) == Some(&path) {
            return;
        }
        self.paths.truncate(self.cursor + 1);
        self.paths.push(path);
        self.cursor = self.paths.len() - 1;
    }
    pub fn previous(&self) -> Option<(usize, PathBuf)> {
        let index = self.cursor.checked_sub(1)?;
        self.paths.get(index).cloned().map(|path| (index, path))
    }
    pub fn next(&self) -> Option<(usize, PathBuf)> {
        let index = self.cursor + 1;
        self.paths.get(index).cloned().map(|path| (index, path))
    }
    pub fn commit(&mut self, index: usize, path: &Path) {
        if self.paths.get(index).is_some_and(|p| p == path) {
            self.cursor = index;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn traversal_commits_only_after_loading_and_new_routes_discard_forward_history() {
        let mut history = History::default();
        history.reset("/project".into());
        history.push("/project/src".into());
        history.push("/project/src/nested".into());
        let (index, path) = history.previous().unwrap();
        assert!(history.next().is_none()); // requesting does not commit a failed load
        history.commit(index, &path);
        assert_eq!(
            history.next().unwrap().1,
            PathBuf::from("/project/src/nested")
        );
        history.push("/project/tests".into());
        assert!(history.next().is_none());
        history.reset("/other".into());
        assert!(history.previous().is_none());
    }
}
