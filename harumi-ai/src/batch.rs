use std::cmp::Reverse;
use std::collections::HashMap;

/// Collect unique non-empty strings from `texts`, preserving first-occurrence order.
///
/// Returns `(unique_texts, index_map)` where `index_map[i]` is the index into
/// `unique_texts` for the i-th input string. Sending only unique strings to the
/// LLM minimises tokens when the same string appears on a page multiple times.
pub(crate) fn unique_strings(texts: impl Iterator<Item = String>) -> (Vec<String>, Vec<usize>) {
    let mut unique: Vec<String> = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut idx_map: Vec<usize> = Vec::new();

    for text in texts {
        let idx = if let Some(&i) = seen.get(&text) {
            i
        } else {
            let i = unique.len();
            seen.insert(text.clone(), i);
            unique.push(text);
            i
        };
        idx_map.push(idx);
    }
    (unique, idx_map)
}

/// Sort replacement pairs longest-old-string first to prevent substring collisions.
///
/// For example, "Subtotal" must be replaced before "Total" so that "Total" inside
/// "Subtotal" is not corrupted by an earlier replacement.
pub(crate) fn sort_pairs_longest_first(pairs: &mut [(String, String)]) {
    pairs.sort_by_key(|b| Reverse(b.0.len()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_strings_deduplicates() {
        let texts = vec!["a".to_owned(), "b".to_owned(), "a".to_owned()];
        let (unique, idx_map) = unique_strings(texts.into_iter());
        assert_eq!(unique, vec!["a", "b"]);
        assert_eq!(idx_map, vec![0, 1, 0]);
    }

    #[test]
    fn sort_pairs_longest_first_ordering() {
        let mut pairs = vec![
            ("Total".to_owned(), "合計".to_owned()),
            ("Subtotal".to_owned(), "小計".to_owned()),
        ];
        sort_pairs_longest_first(&mut pairs);
        assert_eq!(pairs[0].0, "Subtotal");
    }
}
