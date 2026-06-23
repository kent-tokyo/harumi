// repair.rs — empty-translation detection and mojibake detection

/// Returns `true` if `text` looks like mojibake or garbled binary content.
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
