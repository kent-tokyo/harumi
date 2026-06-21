// repair.rs — empty-translation detection and mojibake detection

/// Returns `true` if `text` looks like mojibake or garbled binary content.
// Used in the empty-translation retry path (called from pdf_translator in a future pass).
#[allow(dead_code)]
///
/// Heuristic: if more than 30 % of the characters are non-printable control
/// characters (other than common whitespace) and the string has at least
/// 4 characters, we flag it as suspect.
///
/// False-positive risk: mathematical symbols and exotic Unicode are *not*
/// flagged — only C0/C1 control codes and replacement characters (U+FFFD).
pub(crate) fn is_likely_mojibake(text: &str) -> bool {
    if text.chars().count() < 4 {
        return false;
    }
    let total = text.chars().count();
    let garbage = text.chars().filter(|&c| {
        // C0 control codes (except tab/LF/CR) or C1 controls or replacement char
        (c < '\x20' && c != '\t' && c != '\n' && c != '\r')
            || ('\x7f'..='\u{9F}').contains(&c)
            || c == '\u{FFFD}'
    }).count();
    garbage as f32 / total as f32 > 0.30
}

/// Identify translation blocks that came back empty or as mojibake.
#[allow(dead_code)]
///
/// Returns `(page_num, block_id)` pairs from `translated` that need to be
/// re-translated.  A block is flagged when its translated text is blank
/// or passes [`is_likely_mojibake`].
pub(crate) fn find_bad_blocks(
    original: &[crate::extractor::PageContent],
    translated: &[(u32, Vec<crate::extractor::TranslatedBlock>)],
) -> Vec<(u32, usize)> {
    let mut bad = Vec::new();
    for (page_num, blocks) in translated {
        for tb in blocks {
            if (tb.text.trim().is_empty() || is_likely_mojibake(&tb.text))
                && let Some(orig_page) = original.iter().find(|p| p.page_num == *page_num)
                && orig_page.blocks.iter().any(|b| b.id == tb.id && !b.text.trim().is_empty())
            {
                bad.push((*page_num, tb.id));
            }
        }
    }
    bad
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mojibake_detected_with_control_chars() {
        // String with > 30% garbage characters
        assert!(is_likely_mojibake("\x01\x02\x03 abc"));
    }

    #[test]
    fn normal_text_not_flagged() {
        assert!(!is_likely_mojibake("This is a perfectly normal English sentence."));
    }

    #[test]
    fn cjk_text_not_flagged() {
        assert!(!is_likely_mojibake("日本語のテキストはモジバケではありません。"));
    }

    #[test]
    fn short_strings_never_flagged() {
        // Below 4 chars, always Ok
        assert!(!is_likely_mojibake("\x01\x02\x03"));
    }

    #[test]
    fn replacement_char_flagged() {
        // U+FFFD * 5 + one real char = 83% garbage
        let s = "\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}X";
        assert!(is_likely_mojibake(s));
    }
}
