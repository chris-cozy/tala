//! Canonical rich-content validation shared by editing, import, and recovery.
//! Persist supported TipTap nodes and local media IDs, never arbitrary HTML or URLs for images.

use crate::error::{AppError, Result};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use unicode_normalization::UnicodeNormalization;

pub fn normalized(text: &str) -> String {
    text.nfkc()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}
pub fn text_document(text: &str) -> Value {
    let content: Vec<_> = text
        .lines()
        .map(|line| {
            if line.is_empty() {
                json!({"type": "paragraph"})
            } else {
                json!({"type": "paragraph", "content": [{"type": "text", "text": line}]})
            }
        })
        .collect();
    json!({"type": "doc", "content": content})
}
pub fn tag_name(value: &str) -> Result<String> {
    let tag = normalized(value);
    if tag.is_empty() || tag.len() > 100 || tag.contains(';') || tag.chars().any(char::is_control) {
        return Err(AppError::invalid(
            "Tags must contain 1–100 characters, without semicolons or control characters.",
        ));
    }
    Ok(tag)
}
pub fn valid_media_id(id: &str) -> bool {
    let Some((hash, ext)) = id.rsplit_once('.') else {
        return false;
    };
    hash.len() == 64
        && hash
            .bytes()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        && ["png", "jpg", "webp", "gif", "mp3", "wav"].contains(&ext)
}
pub fn plain_text(node: &Value) -> String {
    let mut text = String::new();
    fn walk(v: &Value, text: &mut String) {
        if let Some(t) = v.get("text").and_then(Value::as_str) {
            text.push_str(t);
        }
        if matches!(v["type"].as_str(), Some("inlineMath" | "blockMath")) {
            text.push_str(v["attrs"]["latex"].as_str().unwrap_or(""));
        }
        if v["type"] == "image" {
            text.push_str(v["attrs"]["alt"].as_str().unwrap_or("[image]"));
        }
        if let Some(children) = v["content"].as_array() {
            for child in children {
                walk(child, text);
            }
        }
        if matches!(
            v["type"].as_str(),
            Some("paragraph" | "hardBreak" | "heading" | "blockMath" | "codeBlock" | "listItem")
        ) {
            text.push('\n');
        }
    }
    walk(node, &mut text);
    text.trim().to_string()
}

/// Returns canonical content and its unique media references after structural and size checks.
/// Unknown attributes are discarded so editor/runtime URLs cannot leak into persisted content.
pub fn validate_document(value: &Value) -> Result<(Value, Vec<String>)> {
    if value["type"] != "doc" || value.to_string().len() > 1_000_000 {
        return Err(AppError::invalid(
            "Card content must be a supported document under 1 MB.",
        ));
    }
    let mut count = 0;
    let mut media = BTreeSet::new();
    fn visit(
        v: &Value,
        depth: u32,
        count: &mut usize,
        media: &mut BTreeSet<String>,
    ) -> Result<Value> {
        *count += 1;
        if depth > 30 || *count > 10_000 {
            return Err(AppError::invalid(
                "This document is too complex to save safely.",
            ));
        }
        let kind = v["type"]
            .as_str()
            .ok_or_else(|| AppError::invalid("Unsupported document node."))?;
        if ![
            "doc",
            "paragraph",
            "text",
            "hardBreak",
            "bulletList",
            "orderedList",
            "listItem",
            "blockquote",
            "codeBlock",
            "horizontalRule",
            "heading",
            "image",
            "audio",
            "inlineMath",
            "blockMath",
        ]
        .contains(&kind)
        {
            return Err(AppError::invalid(
                "This document contains unsupported content.",
            ));
        }
        let mut result = json!({"type":kind});
        let children = match v.get("content") {
            Some(Value::Array(children)) => children.as_slice(),
            None => &[],
            _ => return Err(AppError::invalid("Invalid document structure.")),
        };
        let inline = |child: &Value| {
            matches!(
                child["type"].as_str(),
                Some("text" | "hardBreak" | "inlineMath")
            )
        };
        let block = |child: &Value| {
            matches!(
                child["type"].as_str(),
                Some(
                    "paragraph"
                        | "bulletList"
                        | "orderedList"
                        | "blockquote"
                        | "codeBlock"
                        | "horizontalRule"
                        | "heading"
                        | "image"
                        | "audio"
                        | "blockMath"
                )
            )
        };
        let structure_ok = match kind {
            "doc" => depth == 0 && !children.is_empty() && children.iter().all(block),
            "blockquote" => !children.is_empty() && children.iter().all(block),
            "paragraph" | "heading" => children.iter().all(inline),
            "codeBlock" => children.iter().all(|child| child["type"] == "text"),
            "bulletList" | "orderedList" => {
                !children.is_empty() && children.iter().all(|child| child["type"] == "listItem")
            }
            "listItem" => {
                children
                    .first()
                    .is_some_and(|child| child["type"] == "paragraph")
                    && children.iter().all(block)
            }
            _ => children.is_empty(),
        };
        if !structure_ok {
            return Err(AppError::invalid(
                "This content has an unsupported document structure.",
            ));
        }
        if kind == "text" {
            result["text"] = json!(
                v["text"]
                    .as_str()
                    .ok_or_else(|| AppError::invalid("Invalid text content."))?
            );
            if result["text"].as_str().unwrap().is_empty() {
                return Err(AppError::invalid("Empty text nodes are not supported."));
            }
        }
        if kind == "audio" {
            let id = v["attrs"]["mediaId"]
                .as_str()
                .filter(|id| valid_media_id(id))
                .ok_or_else(|| AppError::invalid("Attach audio locally before saving the card."))?;
            media.insert(id.to_string());
            result["attrs"] = json!({"mediaId":id,"label":v["attrs"]["label"].as_str().unwrap_or("").chars().take(200).collect::<String>()});
        }
        if kind == "image" {
            let id = v["attrs"]["mediaId"]
                .as_str()
                .filter(|id| valid_media_id(id))
                .ok_or_else(|| {
                    AppError::invalid("Attach images locally before saving the card.")
                })?;
            media.insert(id.to_string());
            result["attrs"] = json!({"mediaId":id,"alt":v["attrs"]["alt"].as_str().unwrap_or(""),"title":v["attrs"]["title"].as_str().unwrap_or("")});
        } else if matches!(kind, "inlineMath" | "blockMath") {
            let latex = v["attrs"]["latex"].as_str().unwrap_or("");
            if latex.len() > 20_000 {
                return Err(AppError::invalid("This formula is too long."));
            }
            result["attrs"] = json!({"latex":latex});
        } else if kind == "heading" {
            result["attrs"] = json!({"level":v["attrs"]["level"].as_u64().unwrap_or(2).clamp(1,6)});
        } else if kind == "orderedList" {
            result["attrs"] =
                json!({"start":v["attrs"]["start"].as_u64().unwrap_or(1).clamp(1,100_000)});
        } else if kind == "codeBlock" {
            result["attrs"] = json!({"language":v["attrs"]["language"].as_str().unwrap_or("")});
        }
        if let Some(children) = v["content"].as_array() {
            result["content"] = Value::Array(
                children
                    .iter()
                    .map(|child| visit(child, depth + 1, count, media))
                    .collect::<Result<_>>()?,
            );
        }
        if let Some(marks) = v["marks"].as_array() {
            let mut safe = Vec::new();
            for mark in marks {
                let name = mark["type"].as_str().unwrap_or("");
                if ![
                    "bold",
                    "italic",
                    "underline",
                    "strike",
                    "code",
                    "subscript",
                    "superscript",
                    "link",
                    "highlight",
                    "textStyle",
                ]
                .contains(&name)
                {
                    continue;
                }
                let mut m = json!({"type":name});
                if name == "link" {
                    let href = mark["attrs"]["href"].as_str().unwrap_or("").trim();
                    if !["https://", "http://", "mailto:"]
                        .iter()
                        .any(|scheme| href.to_lowercase().starts_with(scheme))
                        || href.chars().any(char::is_control)
                    {
                        return Err(AppError::invalid("Links must use http, https, or mailto."));
                    }
                    m["attrs"] = json!({"href":href,"target":"_blank","rel":"noopener noreferrer"});
                } else if matches!(name, "highlight" | "textStyle")
                    && let Some(color) = mark["attrs"]["color"].as_str()
                    && color.len() <= 32
                    && color
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "#(),. %".contains(c))
                {
                    m["attrs"] = json!({"color":color});
                }
                safe.push(m);
            }
            result["marks"] = Value::Array(safe);
        }
        Ok(result)
    }
    let document = visit(value, 0, &mut count, &mut media)?;
    let text = plain_text(&document);
    if text.is_empty() && media.is_empty() {
        return Err(AppError::invalid(
            "Front and Back must each contain text, an image, audio, or a formula.",
        ));
    }
    Ok((document, media.into_iter().collect()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn content_is_canonical_and_remote_images_cannot_be_saved() {
        assert!(
            validate_document(&json!({"type":"doc","content":[{"type":"image","attrs":{"src":"https://example.com/tracker.png"}}]}))
            .is_err()
        );
        assert!(
            validate_document(&json!({"type":"doc","content":[{"type":"text","text":"x","marks":[{"type":"link","attrs":{"href":"javascript:alert(1)"}}]}]}))
            .is_err()
        );
        let value = text_document("Hello\nWorld");
        assert_eq!(
            plain_text(&validate_document(&value).unwrap().0),
            "Hello\nWorld"
        );
    }
    #[test]
    fn normalization_handles_unicode_and_spacing() {
        assert_eq!(normalized("  Ｃｅｌｌ   BIOLOGY\n"), "cell biology");
    }
}
