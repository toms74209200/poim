use crate::ir::{Inline, ListItem};

const MAX_HEADING_LEVEL: u8 = 6;

pub fn emit_heading(level: u8, content: &[Inline]) -> String {
    let level = level.clamp(1, MAX_HEADING_LEVEL);
    let hashes = "#".repeat(level as usize);
    format!("{hashes} {}", emit_inline(content))
}

pub fn emit_paragraph(content: &[Inline]) -> String {
    emit_inline(content)
}

pub fn emit_list(ordered: bool, items: &[ListItem]) -> String {
    items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let marker = if ordered {
                format!("{}. ", index + 1)
            } else {
                "- ".to_string()
            };
            format!("{marker}{}", emit_inline(&item.content))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn emit_inline(content: &[Inline]) -> String {
    let mut result = String::new();
    for inline in content {
        match inline {
            Inline::Text(text) => result.push_str(text),
            Inline::Emphasis(inner) => result.push_str(&emit_inline(inner)),
            Inline::Strong(inner) => result.push_str(&emit_inline(inner)),
            Inline::Link { content, .. } => result.push_str(&emit_inline(content)),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    mod emit_heading {
        use super::*;

        #[test]
        fn when_level_one_then_emits_single_hash() {
            let content = vec![Inline::Text("Chapter One".to_string())];

            assert_eq!(emit_heading(1, &content), "# Chapter One");
        }

        #[test]
        fn when_level_two_then_emits_two_hashes() {
            let content = vec![Inline::Text("Section".to_string())];

            assert_eq!(emit_heading(2, &content), "## Section");
        }

        #[test]
        fn when_level_six_then_emits_six_hashes() {
            let content = vec![Inline::Text("Deep".to_string())];

            assert_eq!(emit_heading(6, &content), "###### Deep");
        }

        #[test]
        fn when_level_above_six_then_clamps_to_six() {
            let content = vec![Inline::Text("Too deep".to_string())];

            assert_eq!(emit_heading(7, &content), "###### Too deep");
        }

        #[test]
        fn when_level_zero_then_clamps_to_one() {
            let content = vec![Inline::Text("Too shallow".to_string())];

            assert_eq!(emit_heading(0, &content), "# Too shallow");
        }

        #[test]
        fn when_content_has_multiple_text_inlines_then_concatenates_them() {
            let content = vec![
                Inline::Text("Hello ".to_string()),
                Inline::Text("World".to_string()),
            ];

            assert_eq!(emit_heading(1, &content), "# Hello World");
        }

        #[test]
        fn when_content_has_emphasis_then_emits_inner_text() {
            let content = vec![
                Inline::Text("Hello ".to_string()),
                Inline::Emphasis(vec![Inline::Text("World".to_string())]),
            ];

            assert_eq!(emit_heading(1, &content), "# Hello World");
        }

        #[test]
        fn when_content_has_link_then_emits_inner_text() {
            let content = vec![Inline::Link {
                href: "https://example.com".to_string(),
                content: vec![Inline::Text("here".to_string())],
            }];

            assert_eq!(emit_heading(2, &content), "## here");
        }

        #[test]
        fn when_content_is_empty_then_emits_hashes_only() {
            assert_eq!(emit_heading(1, &[]), "# ");
        }
    }

    mod emit_paragraph {
        use super::*;

        #[test]
        fn when_plain_text_then_emits_text_as_is() {
            let content = vec![Inline::Text("Hello world.".to_string())];

            assert_eq!(emit_paragraph(&content), "Hello world.");
        }

        #[test]
        fn when_content_has_multiple_text_inlines_then_concatenates_them() {
            let content = vec![
                Inline::Text("Hello ".to_string()),
                Inline::Text("world.".to_string()),
            ];

            assert_eq!(emit_paragraph(&content), "Hello world.");
        }

        #[test]
        fn when_content_has_emphasis_then_emits_inner_text() {
            let content = vec![
                Inline::Text("Hello ".to_string()),
                Inline::Emphasis(vec![Inline::Text("world".to_string())]),
                Inline::Text(".".to_string()),
            ];

            assert_eq!(emit_paragraph(&content), "Hello world.");
        }

        #[test]
        fn when_content_has_strong_then_emits_inner_text() {
            let content = vec![Inline::Strong(vec![Inline::Text("important".to_string())])];

            assert_eq!(emit_paragraph(&content), "important");
        }

        #[test]
        fn when_content_has_link_then_emits_inner_text() {
            let content = vec![
                Inline::Text("Visit ".to_string()),
                Inline::Link {
                    href: "https://example.com".to_string(),
                    content: vec![Inline::Text("here".to_string())],
                },
            ];

            assert_eq!(emit_paragraph(&content), "Visit here");
        }

        #[test]
        fn when_content_is_empty_then_emits_empty_string() {
            assert_eq!(emit_paragraph(&[]), "");
        }
    }

    mod emit_list {
        use super::*;

        fn item(text: &str) -> ListItem {
            ListItem {
                content: vec![Inline::Text(text.to_string())],
            }
        }

        #[test]
        fn when_unordered_then_emits_hyphen_markers() {
            let items = vec![item("First"), item("Second")];

            assert_eq!(emit_list(false, &items), "- First\n- Second");
        }

        #[test]
        fn when_ordered_then_emits_incrementing_numbers() {
            let items = vec![item("First"), item("Second"), item("Third")];

            assert_eq!(emit_list(true, &items), "1. First\n2. Second\n3. Third");
        }

        #[test]
        fn when_single_item_then_emits_one_line() {
            let items = vec![item("Only")];

            assert_eq!(emit_list(false, &items), "- Only");
        }

        #[test]
        fn when_item_has_emphasis_then_emits_inner_text() {
            let items = vec![ListItem {
                content: vec![
                    Inline::Text("Hello ".to_string()),
                    Inline::Emphasis(vec![Inline::Text("World".to_string())]),
                ],
            }];

            assert_eq!(emit_list(false, &items), "- Hello World");
        }

        #[test]
        fn when_item_has_link_then_emits_inner_text() {
            let items = vec![ListItem {
                content: vec![
                    Inline::Text("See ".to_string()),
                    Inline::Link {
                        href: "chapter2.xhtml".to_string(),
                        content: vec![Inline::Text("Chapter 2".to_string())],
                    },
                ],
            }];

            assert_eq!(emit_list(true, &items), "1. See Chapter 2");
        }

        #[test]
        fn when_item_content_is_empty_then_emits_marker_only() {
            let items = vec![ListItem { content: vec![] }];

            assert_eq!(emit_list(false, &items), "- ");
        }

        #[test]
        fn when_no_items_then_emits_empty_string() {
            assert_eq!(emit_list(false, &[]), "");
            assert_eq!(emit_list(true, &[]), "");
        }
    }
}
