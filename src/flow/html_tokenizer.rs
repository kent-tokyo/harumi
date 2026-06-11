//! Minimal HTML tokenizer for harumi's HTML→PDF renderer.
//!
//! A pure-Rust, zero-dependency HTML parser that handles a document-oriented
//! subset of HTML (headings, paragraphs, lists, tables, inline styles).
//! Designed to replace the `scraper` crate for lightweight PDF generation.

use std::collections::HashMap;

/// A node in the HTML tree: either text or an element.
#[derive(Debug, Clone)]
pub(crate) enum HtmlNode {
    Text(String),
    Element {
        tag: String,
        attrs: HashMap<String, String>,
        children: Vec<HtmlNode>,
    },
}

impl HtmlNode {
    /// Returns the tag name if this is an element, None if text.
    pub(crate) fn tag_name(&self) -> Option<&str> {
        match self {
            HtmlNode::Element { tag, .. } => Some(tag),
            HtmlNode::Text(_) => None,
        }
    }

    /// Returns the text content if this is a text node, None if element.
    pub(crate) fn as_text(&self) -> Option<&str> {
        match self {
            HtmlNode::Text(s) => Some(s),
            HtmlNode::Element { .. } => None,
        }
    }

    /// Returns attribute value by name (case-insensitive keys).
    pub(crate) fn attr(&self, name: &str) -> Option<String> {
        match self {
            HtmlNode::Element { attrs, .. } => {
                let lower = name.to_ascii_lowercase();
                attrs
                    .iter()
                    .find(|(k, _)| k.to_ascii_lowercase() == lower)
                    .map(|(_, v)| v.clone())
            }
            HtmlNode::Text(_) => None,
        }
    }

    /// Returns iterator over child elements (skipping text nodes).
    pub(crate) fn child_elements(&self) -> impl Iterator<Item = &HtmlNode> {
        match self {
            HtmlNode::Element { children, .. } => children.iter(),
            HtmlNode::Text(_) => [].iter(),
        }
    }

    /// Returns iterator over all children (element and text).
    pub(crate) fn children(&self) -> impl Iterator<Item = &HtmlNode> {
        match self {
            HtmlNode::Element { children, .. } => children.iter(),
            HtmlNode::Text(_) => [].iter(),
        }
    }

    /// Recursively collects all text content.
    pub(crate) fn text_content(&self) -> String {
        match self {
            HtmlNode::Text(s) => s.clone(),
            HtmlNode::Element { children, .. } => children
                .iter()
                .map(|child| child.text_content())
                .collect::<Vec<_>>()
                .join(""),
        }
    }
}

/// Parse HTML string into a document tree.
///
/// This is a best-effort HTML parser for harumi's document-oriented subset.
/// It handles:
/// - Basic tags and attributes
/// - Self-closing tags (`<br>`, `<img>`, `<hr>`)
/// - HTML entities (`&amp;`, `&lt;`, `&gt;`, `&nbsp;`, `&#...;`)
/// - Malformed HTML (closes unclosed tags, ignores unmatched closing tags)
/// - Comments (`<!-- ... -->`)
pub(crate) fn parse_html(html: &str) -> HtmlNode {
    let mut parser = HtmlParser::new(html);
    parser.parse_root()
}

struct HtmlParser {
    input: Vec<char>,
    pos: usize,
}

impl HtmlParser {
    fn new(html: &str) -> Self {
        HtmlParser {
            input: html.chars().collect(),
            pos: 0,
        }
    }

    fn current(&self) -> Option<char> {
        if self.pos < self.input.len() {
            Some(self.input[self.pos])
        } else {
            None
        }
    }

    fn peek(&self, offset: usize) -> Option<char> {
        let p = self.pos + offset;
        if p < self.input.len() {
            Some(self.input[p])
        } else {
            None
        }
    }

    fn advance(&mut self) {
        self.pos += 1;
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.current() {
            if c.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn read_until(&mut self, terminator: char) -> String {
        let mut result = String::new();
        while let Some(c) = self.current() {
            if c == terminator {
                break;
            }
            result.push(c);
            self.advance();
        }
        result
    }

    fn read_tag_name(&mut self) -> String {
        let mut result = String::new();
        while let Some(c) = self.current() {
            if c.is_ascii_alphanumeric() || c == '-' {
                result.push(c);
                self.advance();
            } else {
                break;
            }
        }
        result.to_lowercase()
    }

    fn read_attribute_name(&mut self) -> String {
        let mut result = String::new();
        while let Some(c) = self.current() {
            if c.is_ascii_alphanumeric() || c == '-' || c == ':' {
                result.push(c);
                self.advance();
            } else {
                break;
            }
        }
        result.to_lowercase()
    }

    fn read_attribute_value(&mut self) -> String {
        self.skip_whitespace();
        if self.current() != Some('=') {
            return String::new();
        }
        self.advance(); // skip '='
        self.skip_whitespace();

        let quote = self.current();
        if quote == Some('"') || quote == Some('\'') {
            self.advance();
            let value = self.read_until(quote.unwrap());
            if self.current() == quote {
                self.advance();
            }
            Self::decode_html_entities(&value)
        } else {
            // Unquoted attribute
            let mut result = String::new();
            while let Some(c) = self.current() {
                if c.is_whitespace() || c == '>' {
                    break;
                }
                result.push(c);
                self.advance();
            }
            Self::decode_html_entities(&result)
        }
    }

    fn read_attributes(&mut self) -> HashMap<String, String> {
        let mut attrs = HashMap::new();
        loop {
            self.skip_whitespace();
            if self.current() == Some('>') || self.current() == Some('/') {
                break;
            }
            let name = self.read_attribute_name();
            if name.is_empty() {
                break;
            }
            let value = self.read_attribute_value();
            attrs.insert(name, value);
        }
        attrs
    }

    fn parse_tag(&mut self) -> Option<(String, HashMap<String, String>, bool)> {
        // Current char should be '<'
        if self.current() != Some('<') {
            return None;
        }
        self.advance();

        // Check for comment
        if self.current() == Some('!') && self.peek(1) == Some('-') && self.peek(2) == Some('-') {
            self.advance(); // skip '!'
            self.advance(); // skip first '-'
            self.advance(); // skip second '-'
            // Skip until '-->'
            while self.current().is_some() {
                if self.current() == Some('-')
                    && self.peek(1) == Some('-')
                    && self.peek(2) == Some('>')
                {
                    self.advance();
                    self.advance();
                    self.advance();
                    break;
                }
                self.advance();
            }
            return None; // Comment nodes are skipped
        }

        // Check for closing tag
        if self.current() == Some('/') {
            return None;
        }

        let tag_name = self.read_tag_name();
        if tag_name.is_empty() {
            return None;
        }

        let attrs = self.read_attributes();

        let self_closing = self.current() == Some('/');
        if self_closing {
            self.advance();
        }

        if self.current() == Some('>') {
            self.advance();
        }

        Some((tag_name, attrs, self_closing))
    }

    fn is_self_closing_tag(tag: &str) -> bool {
        matches!(
            tag,
            "br" | "hr"
                | "img"
                | "input"
                | "meta"
                | "link"
                | "area"
                | "base"
                | "col"
                | "embed"
                | "source"
                | "track"
                | "wbr"
        )
    }

    fn decode_html_entities(text: &str) -> String {
        let mut result = String::new();
        let mut chars = text.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '&' {
                let mut entity = String::new();
                while let Some(&next) = chars.peek() {
                    if next == ';' {
                        chars.next();
                        break;
                    }
                    entity.push(next);
                    chars.next();
                }

                let decoded: String = match entity.as_str() {
                    "amp" => "&".to_string(),
                    "lt" => "<".to_string(),
                    "gt" => ">".to_string(),
                    "quot" => "\"".to_string(),
                    "apos" => "'".to_string(),
                    "nbsp" => "\u{00A0}".to_string(),
                    _ if entity.starts_with('#') => {
                        if let Ok(code) = entity[1..].parse::<u32>() {
                            if let Some(ch) = char::from_u32(code) {
                                ch.to_string()
                            } else {
                                format!("&{};", entity)
                            }
                        } else {
                            format!("&{};", entity)
                        }
                    }
                    _ => format!("&{};", entity),
                };
                result.push_str(&decoded);
            } else {
                result.push(c);
            }
        }

        result
    }

    fn parse_root(&mut self) -> HtmlNode {
        let mut stack: Vec<(String, HashMap<String, String>, Vec<HtmlNode>)> = Vec::new(); // (tag, attrs, children)
        stack.push(("root".to_string(), HashMap::new(), Vec::new()));

        while self.current().is_some() && !stack.is_empty() {
            self.skip_whitespace();
            if self.current() == Some('<') {
                if self.peek(1) == Some('/') {
                    // Closing tag
                    self.advance(); // skip '<'
                    self.advance(); // skip '/'
                    let closing_tag = self.read_tag_name();
                    self.skip_whitespace();
                    if self.current() == Some('>') {
                        self.advance();
                    }
                    // Pop from stack if tag matches
                    if let Some((tag, _, _)) = stack.last() {
                        if closing_tag == *tag {
                            let (tag, attrs, children) = stack.pop().unwrap();
                            let node = HtmlNode::Element {
                                tag,
                                attrs,
                                children,
                            };
                            if let Some((_, _, parent_children)) = stack.last_mut() {
                                parent_children.push(node);
                            }
                        }
                    }
                } else if let Some((tag, attrs, is_self_closing)) = self.parse_tag() {
                    if is_self_closing || Self::is_self_closing_tag(&tag) {
                        let node = HtmlNode::Element {
                            tag,
                            attrs,
                            children: Vec::new(),
                        };
                        if let Some((_, _, children)) = stack.last_mut() {
                            children.push(node);
                        }
                    } else {
                        stack.push((tag, attrs, Vec::new()));
                    }
                }
            } else {
                // Text node
                let mut text = String::new();
                while let Some(c) = self.current() {
                    if c == '<' {
                        break;
                    }
                    text.push(c);
                    self.advance();
                }
                let decoded = Self::decode_html_entities(&text);
                if !decoded.trim().is_empty() {
                    if let Some((_, _, children)) = stack.last_mut() {
                        children.push(HtmlNode::Text(decoded));
                    }
                }
            }
        }

        if let Some((_, _, children)) = stack.pop() {
            HtmlNode::Element {
                tag: "root".to_string(),
                attrs: HashMap::new(),
                children,
            }
        } else {
            HtmlNode::Element {
                tag: "root".to_string(),
                attrs: HashMap::new(),
                children: Vec::new(),
            }
        }
    }
}
