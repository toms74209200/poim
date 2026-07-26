use crate::ir::Inline;

const MAX_HEADING_LEVEL: u8 = 6;

pub fn emit_heading(level: u8, content: &[Inline]) -> String {
    let level = level.clamp(1, MAX_HEADING_LEVEL);
    let hashes = "#".repeat(level as usize);
    format!("{hashes} {}", emit_inline(content))
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
}
