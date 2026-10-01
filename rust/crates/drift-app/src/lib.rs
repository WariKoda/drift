//! Application state independent of GPUI and transport implementations.
pub mod browser;

/// Selection uses the displayed row's stable path, independent of filtering.
pub struct FileList {
    entries: Vec<String>,
    visible: Vec<usize>,
    selected: Option<usize>,
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
}
