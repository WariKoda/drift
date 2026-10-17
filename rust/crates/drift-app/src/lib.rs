//! Application state independent of GPUI and transport implementations.
pub mod browser;
pub mod cli;
pub mod comparison;
mod finder;
pub mod gui_preferences;
pub mod hosts;
pub mod logging;
pub mod navigation;
pub mod projects;
pub mod remote;
pub mod sync;
pub mod tree;

use std::collections::BTreeSet;

/// Cursor uses the displayed row's stable path, independent of filtering.
#[derive(Clone)]
pub struct FileList {
    entries: Vec<String>,
    visible: Vec<usize>,
    selected: Option<usize>,
    marked: BTreeSet<String>,
    range_start: Option<String>,
    allowed_marks: Option<BTreeSet<String>>,
    remote_root: Option<std::path::PathBuf>,
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
            remote_root: None,
        }
    }
    /// Remote paths stay absolute for selection; exclusions apply only inside
    /// the host root, never to the external prefix naming that root.
    pub fn new_remote(root: &str, entries: Vec<String>) -> Self {
        let mut list = Self::new(vec![]);
        list.remote_root = Some(root.into());
        list.entries = entries
            .into_iter()
            .filter(|path| list.eligible_path(path))
            .collect();
        list.visible = (0..list.entries.len()).collect();
        list
    }
    fn eligible_path(&self, path: &str) -> bool {
        if path.contains('\0') {
            return false;
        }
        let path = std::path::Path::new(path);
        let relative = match &self.remote_root {
            Some(root) => match path.strip_prefix(root) {
                Ok(relative) => relative,
                Err(_) => return false,
            },
            None => path,
        };
        !relative
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
            && !browser::hard_excluded(relative, false)
    }
    /// Replace a listing without losing project-wide marks from other folders
    /// or visibility settings. Only a new project/connection clears marks.
    pub fn replace_entries(&mut self, entries: Vec<String>) {
        let cursor = self.selected().map(str::to_owned);
        let marked = std::mem::take(&mut self.marked);
        *self = match self.remote_root.clone() {
            Some(root) => Self::new_remote(&root.to_string_lossy(), entries),
            None => Self::new(entries),
        };
        self.marked = marked;
        self.selected = cursor.and_then(|path| self.entries.iter().position(|p| *p == path));
    }
    pub fn replace_remote_entries(&mut self, root: &str, entries: Vec<String>) {
        self.remote_root = Some(root.into());
        self.replace_entries(entries);
    }
    /// Restore a saved view, keeping live mark additions and removals even for
    /// paths outside that view. The saved view's marking restrictions apply.
    pub fn restore_view(&mut self, mut previous: FileList) {
        previous.marked = std::mem::take(&mut self.marked)
            .into_iter()
            .filter(|path| previous.can_mark(path))
            .collect();
        *self = previous;
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
        self.eligible_path(path)
            && self
                .allowed_marks
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
    /// Rank Unicode-lowercased subsequence matches across each full path.
    /// Empty queries restore source order; marks, ranges and restrictions stay
    /// intact, and the cursor stays on its stable path while it remains visible.
    pub fn filter_finder(&mut self, query: &str) {
        self.visible = finder::matching_indices(&self.entries, query);
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
    pub fn select_path(&mut self, path: &str) {
        self.selected = self
            .visible
            .iter()
            .copied()
            .find(|i| self.entries[*i] == path);
    }
    pub fn marked_descendants(&self, path: &str) -> usize {
        let parent = std::path::Path::new(path);
        self.marked
            .iter()
            .filter(|p| p.as_str() != path && std::path::Path::new(p).starts_with(parent))
            .count()
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

    #[test]
    fn remote_lists_scope_exclusions_to_the_host_root_across_replacements() {
        let root = "/srv/node_modules/site";
        let good = format!("{root}/normal.txt");
        let staging = format!("{root}/.normal.drift-tmp-0123456789abcdef0123456789abcdef");
        let mut list = FileList::new_remote(
            root,
            vec![
                good.clone(),
                format!("{root}/node_modules/x"),
                staging,
                "/outside/file".into(),
                format!("{root}/../escape"),
                format!("{root}/bad\0name"),
            ],
        );
        assert_eq!(list.len(), 1);
        list.select(0);
        list.toggle_mark();
        list.replace_remote_entries(
            root,
            vec![
                format!("{root}/node_modules"),
                good.clone(),
                format!("{root}/.git/config"),
            ],
        );
        assert_eq!(list.len(), 2);
        assert_eq!(list.selected(), Some(good.as_str()));
        assert_eq!(list.marked(), [good]);
        assert!(list.can_mark(&format!("{root}/node_modules")));
        assert!(!list.can_mark("/outside/file"));
    }

    #[test]
    fn saved_remote_view_retains_relative_exclusion_policy_and_live_marks() {
        let root = "/srv/.git/tree";
        let first = format!("{root}/first");
        let second = format!("{root}/second");
        let mut list = FileList::new_remote(root, vec![first.clone(), second.clone()]);
        list.select(0);
        let saved = list.clone();
        list.select(1);
        list.toggle_mark();
        list.restore_view(saved);
        assert_eq!(list.selected(), Some(first.as_str()));
        assert_eq!(list.marked(), [second]);
        assert!(!list.can_mark(&format!("{root}/.git/config")));
        assert!(list.can_mark(&first));
    }
}

#[cfg(test)]
mod finder_tests {
    use super::*;

    fn list(paths: &[&str]) -> FileList {
        FileList::new(paths.iter().map(|path| (*path).to_owned()).collect())
    }

    fn rows(list: &FileList) -> Vec<&str> {
        (0..list.len()).map(|row| list.row(row).unwrap()).collect()
    }

    #[test]
    fn finder_matches_subsequences_across_paths_not_ordinary_substrings() {
        let source = [
            "internal/config/loader.go",
            "internal/diff/engine.go",
            "README.md",
        ];
        let mut list = list(&source);
        list.filter("cfgload");
        assert!(list.is_empty());
        list.filter_finder("CFGLOAD");
        assert_eq!(rows(&list), [source[0]]);
        list.filter_finder("zzzzzz");
        assert!(list.is_empty());
        list.filter_finder("");
        assert_eq!(rows(&list), source);
    }

    #[test]
    fn ranking_prefers_contiguous_boundary_early_and_short_matches() {
        let mut list = list(&["a/b", "ab"]);
        list.filter_finder("ab");
        assert_eq!(rows(&list), ["ab", "a/b"]);

        let mut list = FileList::new(vec!["xab".into(), "x/ab".into()]);
        list.filter_finder("ab");
        assert_eq!(rows(&list), ["x/ab", "xab"]);

        let mut list = FileList::new(vec!["xab".into(), "xAb".into()]);
        list.filter_finder("ab");
        assert_eq!(rows(&list), ["xAb", "xab"]);

        let mut list = FileList::new(vec!["__ab_".into(), "ab___".into()]);
        list.filter_finder("ab");
        assert_eq!(rows(&list), ["ab___", "__ab_"]);

        let mut list = FileList::new(vec!["ab-long".into(), "ab".into()]);
        list.filter_finder("ab");
        assert_eq!(rows(&list), ["ab", "ab-long"]);
    }

    #[test]
    fn equal_quality_keeps_input_order_and_uses_best_not_first_alignment() {
        let mut list = list(&["ab/z", "ab/a", "ab/m"]);
        list.filter_finder("ab");
        assert_eq!(rows(&list), ["ab/z", "ab/a", "ab/m"]);
        list.filter_finder("ab");
        assert_eq!(rows(&list), ["ab/z", "ab/a", "ab/m"]);

        let mut list = FileList::new(vec!["a__b__".into(), "a___ab".into()]);
        list.filter_finder("ab");
        assert_eq!(rows(&list), ["a___ab", "a__b__"]);
    }

    #[test]
    fn unicode_lowercase_subsequences_and_expansions_are_scalar_based() {
        let mut list = list(&["älpha/журнал.rs", "ÄЖ.rs", "plain.rs"]);
        list.filter_finder("äж");
        assert_eq!(rows(&list), ["ÄЖ.rs", "älpha/журнал.rs"]);
        list.filter_finder("ÄЖ");
        assert_eq!(rows(&list), ["ÄЖ.rs", "älpha/журнал.rs"]);

        let mut list = FileList::new(vec!["İstanbul.rs".into(), "i\u{307}stanbul.rs".into()]);
        list.filter_finder("İST");
        assert_eq!(list.len(), 2);
        list.filter_finder("📁");
        assert!(list.is_empty());

        let mut list = FileList::new(vec!["abé".into(), "abc".into(), "📁/ж.rs".into()]);
        list.filter_finder("ab");
        assert_eq!(rows(&list), ["abé", "abc"]); // Equal scalar, not byte lengths.
        list.filter_finder("📁Ж");
        assert_eq!(rows(&list), ["📁/ж.rs"]);
    }

    #[test]
    fn ordinary_filter_stays_substring_only_and_source_ordered() {
        let mut list = list(&["long/AB.rs", "ab", "a/b", "ÄЖ.rs"]);
        list.filter_finder("ab");
        assert_eq!(list.row(0), Some("ab"));
        list.filter("AB");
        assert_eq!(rows(&list), ["long/AB.rs", "ab"]);
        list.filter("äж");
        assert_eq!(rows(&list), ["ÄЖ.rs"]);
        list.filter("");
        assert_eq!(rows(&list), ["long/AB.rs", "ab", "a/b", "ÄЖ.rs"]);
    }

    #[test]
    fn finder_preserves_stable_cursor_marks_restrictions_and_range_anchor() {
        let mut list = list(&["alpha/beta", "ab", "a/b", "zz"]);
        list.restrict_marks(["ab".into(), "a/b".into()].into());
        list.select_path("a/b");
        list.toggle_mark();
        list.visual_range();
        list.filter_finder("ab");
        assert_eq!(list.selected(), Some("a/b"));
        assert_eq!(list.selected_row(), Some(1));
        assert!(list.range_active());
        assert_eq!(list.marked(), ["a/b"]);
        assert!(!list.can_mark("alpha/beta"));
        list.select_range(0);
        list.visual_range();
        assert!(!list.range_active());
        assert_eq!(list.marked(), ["a/b", "ab"]);
        list.invert_visible();
        assert!(list.marked().is_empty());
        list.visual_range();
        list.filter_finder("zz");
        assert_eq!(list.selected(), None);
        assert!(list.range_active());
        list.select(0);
        list.visual_range(); // A hidden anchor cannot mark intervening paths.
        assert!(!list.range_active());
        list.toggle_mark();
        assert!(list.marked().is_empty());
        list.filter_finder("");
        assert_eq!(list.selected(), Some("zz"));
        assert_eq!(list.selected_row(), Some(3));
    }

    #[test]
    fn hard_exclusions_never_appear_or_become_marks_in_finder_or_restore() {
        let excluded = [
            ".git/config",
            "node_modules/ab.rs",
            "sub/.idea/ab.rs",
            "sub/.ab.drift-tmp-0123456789abcdef0123456789abcdef",
        ];
        let mut list = list(&excluded);
        let previous = list.clone();
        for path in excluded {
            assert!(!list.can_mark(path));
            list.mark(path.into());
        }
        assert!(list.marked().is_empty());
        // Even stale live marks must pass the shared policy during restoration.
        list.marked.extend(excluded.map(str::to_owned));
        list.restore_view(previous);
        assert!(list.marked().is_empty());
        for query in ["ab", "config", "drift-tmp", ""] {
            list.filter_finder(query);
            assert!(list.is_empty());
        }
    }

    #[test]
    fn restore_recovers_saved_view_but_keeps_live_added_and_removed_marks() {
        let tree = ["tree/a", "tree/b", "tree/c", "tree/d", "forbidden"];
        let mut list = list(&["outside/old"]);
        list.select(0);
        list.toggle_mark();
        list.replace_entries(tree.map(str::to_owned).into());
        list.select_path("tree/b");
        list.toggle_mark();
        let allowed = [
            "tree/a",
            "tree/b",
            "tree/c",
            "tree/d",
            "outside/old",
            "outside/new",
        ]
        .map(str::to_owned)
        .into();
        list.restrict_marks(allowed);
        list.filter("tree");
        list.visual_range();
        list.select_path("tree/c");
        let previous = list.clone();
        assert_eq!(previous.marked(), ["outside/old", "tree/b"]);

        list.replace_entries(
            [
                "tree/b",
                "tree/a",
                "outside/old",
                "outside/new",
                "forbidden",
            ]
            .map(str::to_owned)
            .into(),
        );
        list.filter_finder("");
        for path in [
            "tree/b",
            "outside/old",
            "outside/new",
            "tree/a",
            "forbidden",
        ] {
            list.select_path(path);
            list.toggle_mark();
        }
        assert_eq!(list.marked(), ["forbidden", "outside/new", "tree/a"]);
        assert_eq!(previous.marked(), ["outside/old", "tree/b"]);
        list.restore_view(previous);
        assert_eq!(rows(&list), ["tree/a", "tree/b", "tree/c", "tree/d"]);
        assert_eq!(list.selected(), Some("tree/c"));
        assert_eq!(list.selected_row(), Some(2));
        assert_eq!(list.marked(), ["outside/new", "tree/a"]);
        assert!(!list.can_mark("forbidden"));
        assert!(list.can_mark("outside/new"));
        assert!(list.range_active());
        list.visual_range();
        assert_eq!(list.marked(), ["outside/new", "tree/a", "tree/b", "tree/c"]);
        list.filter("");
        assert_eq!(rows(&list), tree);
        list.select_path("forbidden");
        list.toggle_mark();
        assert!(!list.is_marked("forbidden"));
    }
}
