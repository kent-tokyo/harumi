#[cfg_attr(not(any(feature = "openai", feature = "anthropic")), allow(dead_code))]
pub(crate) fn translation_system_prompt(target_lang: &str, source_lang: Option<&str>) -> String {
    let source_clause = source_lang_clause(source_lang);
    format!(
        "You are a document translation engine.\n\n\
         Input contract:\n\
         - The user message is a JSON object with this shape:\n\
           {{\"pages\":[{{\"page\":<number>,\"blocks\":[{{\"id\":<number>,\"type\":\"h1\"|\"h2\"|\"h3\"|\"h4\"|\"h5\"|\"h6\"|\"paragraph\",\"text\":\"<source>\"}}]}}]}}\n\
         - Translate every `text` field {source_clause}to {target_lang}.\n\
         - Preserve page order, block order, page numbers, ids, and block types exactly.\n\
         - Keep punctuation, numbers, units, and product codes intact unless translation requires changing them.\n\
         - Keep the translation concise enough to fit the original layout when possible.\n\
         - If a block is already in the target language, keep it natural and readable.\n\n\
         Output contract:\n\
         - Return ONLY valid JSON.\n\
         - Do not wrap the JSON in markdown fences.\n\
         - Do not add commentary or extra keys.\n\
         - Return the same number of pages and blocks as the input.\n\
         - Escape any double-quote characters inside `text` values as \\\".\n\
         - Output shape:\n\
           {{\"pages\":[{{\"blocks\":[{{\"id\":<number>,\"text\":\"<translated>\"}}]}}]}}"
    )
}

pub(crate) fn layout_correction_prompt(
    target_lang: &str,
    source_lang: Option<&str>,
    issues_json: &str,
) -> String {
    let source_clause = source_lang_clause(source_lang);
    format!(
        "You are a PDF layout editor.\n\n\
         Task:\n\
         - Repair only the translated lines listed in the layout issue JSON below.\n\
         - Preserve meaning and keep the output in {target_lang}{source_clause}.\n\
         - Prefer concise wording over reflowing or commentary.\n\
         - For table cells and value fields, preserve numbers, units, product names, and codes exactly.\n\
         - For headings, shorten wording but keep the section meaning clear.\n\
         - For notes/paragraphs, remove redundant phrasing before dropping meaning.\n\
         - Do not change Minor issues unless they are paired with overflow, major collision, or image overlap.\n\
         - Prioritize major collision, overflow, and image_overlap issues over accepted shrink.\n\n\
         Rules:\n\
         - Return ONLY valid JSON.\n\
         - Preserve every `id` and `page`.\n\
         - Do not change lines that are not listed in the issue JSON.\n\
         - Do not add markdown fences, explanations, or extra keys.\n\
         - Escape any double-quote characters inside `text` values as \\\".\n\
         - Output shape:\n\
           {{\"corrections\":[{{\"id\":<number>,\"page\":<number>,\"text\":\"<shorter translation>\"}}]}}\n\n\
         Layout issue JSON:\n\
         {issues_json}"
    )
}

fn source_lang_clause(source_lang: Option<&str>) -> String {
    match source_lang {
        Some(s) if !s.is_empty() && s != "auto" => format!("from {s} "),
        _ => String::from("from the auto-detected source language "),
    }
}

#[cfg(test)]
mod tests {
    use super::{layout_correction_prompt, translation_system_prompt};

    #[test]
    fn translation_prompt_mentions_json_and_target() {
        let prompt = translation_system_prompt("zh", Some("ja"));
        assert!(prompt.contains("valid JSON"));
        assert!(prompt.contains("zh"));
        assert!(prompt.contains("ja"));
    }

    #[test]
    fn correction_prompt_mentions_overflows_and_target() {
        let prompt = layout_correction_prompt("zh", None, "{\"pages\":[]}");
        assert!(prompt.contains("Layout issue JSON"));
        assert!(prompt.contains("zh"));
        assert!(prompt.contains("auto-detected source language"));
    }
}
