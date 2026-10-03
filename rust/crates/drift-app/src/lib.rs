//! Application state independent of GPUI and transport implementations.
pub mod browser;
pub mod comparison;
pub mod hosts;
pub mod navigation;
pub mod projects;
pub mod remote;
pub mod sync;

use std::collections::BTreeSet;

/// Cursor uses the displayed row's stable path, independent of filtering.
pub struct FileList {
    entries: Vec<String>,
    visible: Vec<usize>,
    selected: Option<usize>,
    marked: BTreeSet<String>,
    range_start: Option<String>,
    allowed_marks: Option<BTreeSet<String>>,
}
impl FileList {
    pub fn new(entries: Vec<String>) -> Self {
        let entries: Vec<_> = entries
            .into_iter()
            .filter(|p| !browser::hard_excluded(std::path::Path::new(p), false))
            .collect();
        let visible = (0..entries.len()).collect();
        Self {
            entries,
            visible,
            selected: None,
            marked: BTreeSet::new(),
            range_start: None,
            allowed_marks: None,
        }
    }
    /// Replace a listing without losing project-wide marks from other folders
    /// or visibility settings. Only a new project/connection clears marks.
    pub fn replace_entries(&mut self, entries: Vec<String>) {
        let cursor = self.selected().map(str::to_owned);
        let marked = std::mem::take(&mut self.marked);
        *self = Self::new(entries);
        self.marked = marked;
        self.selected = cursor.and_then(|path| self.entries.iter().position(|p| *p == path));
    }
    pub fn range_active(&self) -> bool {
        self.range_start.is_some()
    }
    pub fn marked(&self) -> Vec<String> {
        self.marked.iter().cloned().collect()
    }
    pub fn is_marked(&self, path: &str) -> bool {
        self.marked.contains(path)
    }
    pub fn restrict_marks(&mut self, allowed: BTreeSet<String>) {
        self.marked.retain(|path| allowed.contains(path));
        self.allowed_marks = Some(allowed);
    }
    pub fn can_mark(&self, path: &str) -> bool {
        self.allowed_marks
            .as_ref()
            .is_none_or(|allowed| allowed.contains(path))
    }
    fn mark(&mut self, path: String) {
        if self.can_mark(&path) {
            self.marked.insert(path);
        }
    }
    pub fn toggle_mark(&mut self) {
        if let Some(path) = self.selected().map(str::to_owned)
            && !self.marked.remove(&path)
        {
            self.mark(path);
        }
    }
    pub fn mark_siblings(&mut self) {
        let Some(parent) = self
            .selected()
            .and_then(|path| std::path::Path::new(path).parent())
            .map(std::path::Path::to_path_buf)
        else {
            return;
        };
        let paths: Vec<_> = self
            .visible
            .iter()
            .map(|i| &self.entries[*i])
            .filter(|path| std::path::Path::new(path).parent() == Some(parent.as_path()))
            .cloned()
            .collect();
        for path in paths {
            self.mark(path);
        }
    }
    pub fn invert_visible(&mut self) {
        let paths: Vec<_> = self
            .visible
            .iter()
            .map(|i| self.entries[*i].clone())
            .collect();
        for path in paths {
            if !self.marked.remove(&path) {
                self.mark(path);
            }
        }
    }
    /// Add a visible interval; hidden paths are never added implicitly.
    pub fn select_range(&mut self, index: usize) {
        let start = self.selected_row().unwrap_or(index);
        self.select(index);
        if let Some(end) = self.selected_row() {
            for row in start.min(end)..=start.max(end) {
                self.mark(self.row(row).unwrap().to_owned());
            }
        }
    }
    /// Go's v starts/finishes an additive interval anchored by stable path.
    pub fn visual_range(&mut self) {
        if let Some(path) = self.range_start.take() {
            let start = self.visible.iter().position(|i| self.entries[*i] == path);
            if let (Some(start), Some(end)) = (start, self.selected_row()) {
                for row in start.min(end)..=start.max(end) {
                    self.mark(self.row(row).unwrap().to_owned());
                }
            }
        } else {
            self.range_start = self.selected().map(str::to_owned);
        }
    }
    /// Escape first cancels a visual interval, then clears marks.
    pub fn clear_marks(&mut self) {
        if self.range_start.take().is_none() {
            self.marked.clear();
        }
    }
    pub fn filter(&mut self, query: &str) {
        let query = query.to_lowercase();
        self.visible = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(i, name)| name.to_lowercase().contains(&query).then_some(i))
            .collect();
        if self.selected.is_some_and(|i| !self.visible.contains(&i)) {
            self.selected = None;
        }
    }
    pub fn len(&self) -> usize {
        self.visible.len()
    }
    pub fn is_empty(&self) -> bool {
        self.visible.is_empty()
    }
    pub fn row(&self, index: usize) -> Option<&str> {
        self.visible.get(index).map(|i| self.entries[*i].as_str())
    }
    pub fn select(&mut self, index: usize) {
        self.selected = self.visible.get(index).copied();
    }
    pub fn selected(&self) -> Option<&str> {
        self.selected.map(|i| self.entries[i].as_str())
    }
    pub fn selected_row(&self) -> Option<usize> {
        self.selected
            .and_then(|selected| self.visible.iter().position(|i| *i == selected))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_survives_row_reordering_and_clears_when_hidden() {
        let mut list = FileList::new(vec![
            "alpha.rs".into(),
            "beta.rs".into(),
            "beta.toml".into(),
        ]);
        list.select(1);
        list.filter("BETA");
        assert_eq!(list.row(0), Some("beta.rs"));
        assert_eq!(list.selected(), Some("beta.rs"));
        list.filter("toml");
        assert_eq!(list.selected(), None);
        list.select(0);
        list.filter("");
        assert_eq!(list.selected(), Some("beta.toml"));
    }
    #[test]
    fn staging_files_are_never_exposed_by_a_filter() {
        let mut list = FileList::new(vec![
            ".a.drift-tmp-0123456789abcdef0123456789abcdef".into(),
            "a".into(),
        ]);
        list.filter("drift-tmp");
        assert!(list.is_empty());
        list.select(usize::MAX);
        assert_eq!(list.selected(), None);
    }
    #[test]
    fn marks_are_independent_of_cursor_filter_listing_and_other_panes() {
        let mut left = FileList::new(vec!["a".into(), "b".into(), "child/c".into()]);
        let right = FileList::new(vec!["a".into()]);
        left.select(0);
        left.toggle_mark();
        left.select(1);
        left.visual_range();
        left.select(2);
        left.visual_range();
        assert_eq!(left.marked(), ["a", "b", "child/c"]);
        left.filter("child");
        assert_eq!(left.selected(), Some("child/c"));
        left.invert_visible();
        assert_eq!(left.marked(), ["a", "b"]);
        left.replace_entries(vec!["child/d".into()]);
        left.select(0);
        left.toggle_mark();
        assert_eq!(left.marked(), ["a", "b", "child/d"]);
        assert!(right.marked().is_empty());
        left.visual_range();
        left.clear_marks(); // Cancel only the visual interval.
        assert_eq!(left.marked().len(), 3);
        left.clear_marks();
        assert!(left.marked().is_empty());
    }
    #[test]
    fn visible_ranges_and_siblings_never_include_hidden_or_disallowed_paths() {
        let mut list = FileList::new(vec![
            "a".into(),
            "b".into(),
            "child/c".into(),
            "child/d".into(),
            "node_modules/x".into(),
        ]);
        list.select(0);
        list.visual_range();
        list.filter("child");
        list.select(1);
        list.visual_range(); // Hidden anchor cancels the interval.
        assert!(list.marked().is_empty());
        list.filter("");
        list.restrict_marks(["a".into(), "child/c".into()].into());
        list.select(0);
        list.select_range(3);
        assert_eq!(list.marked(), ["a", "child/c"]);
        list.clear_marks();
        list.select(2);
        list.mark_siblings();
        assert_eq!(list.marked(), ["child/c"]);
        list.select(0);
        list.mark_siblings();
        assert_eq!(list.marked(), ["a", "child/c"]);
    }
}
