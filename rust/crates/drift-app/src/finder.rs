//! Finder ranking only; eligibility and marking policy belong to FileList.
use std::cmp::Reverse;

pub(super) fn matching_indices(entries: &[String], query: &str) -> Vec<usize> {
    let query: Vec<_> = query.to_lowercase().chars().collect();
    if query.is_empty() {
        return (0..entries.len()).collect();
    }
    let mut matches: Vec<_> = entries
        .iter()
        .enumerate()
        .filter_map(|(index, path)| {
            let chars = folded_path(path);
            score(&chars, &query).map(|score| (Reverse(score), chars.len(), index))
        })
        .collect();
    matches.sort_unstable();
    matches.into_iter().map(|(_, _, index)| index).collect()
}

fn folded_path(path: &str) -> Vec<(char, bool)> {
    let mut previous: Option<char> = None;
    let mut chars = Vec::new();
    for ch in path.chars() {
        let boundary = previous.is_none_or(|prev| {
            !prev.is_alphanumeric() || (prev.is_lowercase() && ch.is_uppercase())
        });
        for (index, folded) in ch.to_lowercase().enumerate() {
            chars.push((folded, boundary && index == 0));
        }
        previous = Some(ch);
    }
    chars
}

/// Best alignment in O(query * path) time and O(path) space. Adjacent matches
/// earn 24 points, word/path/camel boundaries 16; each skipped scalar costs 1.
/// Ranking uses scalar counts rather than UTF-8 byte lengths. Lowercasing may
/// expand a scalar; this is not Unicode normalization or full case folding.
fn score(path: &[(char, bool)], query: &[char]) -> Option<i64> {
    if query.len() > path.len() {
        return None;
    }
    let mut previous = vec![None; path.len()];
    let mut current = vec![None; path.len()];
    for (query_index, needle) in query.iter().enumerate() {
        current.fill(None);
        let mut best_gapped: Option<i64> = None;
        for (index, (ch, boundary)) in path.iter().enumerate() {
            // For nonadjacent predecessors j, score[j] - (index - j - 1)
            // reduces to a prefix maximum of score[j] + j.
            if index >= 2
                && let Some(score) = previous[index - 2]
            {
                let candidate = score + (index - 2) as i64;
                best_gapped = Some(best_gapped.map_or(candidate, |best| best.max(candidate)));
            }
            if ch != needle {
                continue;
            }
            let bonus = if *boundary { 16 } else { 0 };
            current[index] = if query_index == 0 {
                Some(bonus - index as i64)
            } else {
                let adjacent = index
                    .checked_sub(1)
                    .and_then(|i| previous[i])
                    .map(|score| score + 24);
                let gapped = best_gapped.map(|score| score - index as i64 + 1);
                adjacent
                    .into_iter()
                    .chain(gapped)
                    .max()
                    .map(|score| score + bonus)
            };
        }
        std::mem::swap(&mut current, &mut previous);
    }
    previous.into_iter().flatten().max()
}
