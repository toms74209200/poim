use crate::ir::{Block, Inline, ListItem};

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

pub fn emit_table(headers: &[Vec<Inline>], rows: &[Vec<Vec<Inline>>]) -> String {
    let column_count = headers
        .len()
        .max(rows.iter().map(Vec::len).max().unwrap_or(0));
    if column_count == 0 {
        return String::new();
    }

    let mut lines = vec![
        emit_row(headers, column_count),
        format!("|{}", " --- |".repeat(column_count)),
    ];
    lines.extend(rows.iter().map(|row| emit_row(row, column_count)));
    lines.join("\n")
}

fn emit_row(cells: &[Vec<Inline>], column_count: usize) -> String {
    let mut result = String::from("|");
    for index in 0..column_count {
        let cell = cells.get(index).map(|c| emit_cell(c)).unwrap_or_default();
        result.push_str(&format!(" {cell} |"));
    }
    result
}

fn emit_cell(content: &[Inline]) -> String {
    emit_inline(content).replace('|', "\\|")
}

pub fn emit_image(src: &str, alt: &str) -> String {
    format!("![{alt}]({src})")
}

pub fn collect_image_sources(blocks: &[Block]) -> Vec<String> {
    let mut sources: Vec<String> = Vec::new();
    for block in blocks {
        if let Block::Image { src, .. } = block
            && !sources.iter().any(|seen| seen == src)
        {
            sources.push(src.clone());
        }
    }
    sources
}

pub fn emit_inline(content: &[Inline]) -> String {
    let mut result = String::new();
    for inline in content {
        match inline {
            Inline::Text(text) => result.push_str(text),
            Inline::Emphasis(inner) => {
                let inner = emit_inline(inner);
                if !inner.is_empty() {
                    result.push_str(&format!("*{inner}*"));
                }
            }
            Inline::Strong(inner) => {
                let inner = emit_inline(inner);
                if !inner.is_empty() {
                    result.push_str(&format!("**{inner}**"));
                }
            }
            Inline::Link { href, content } => {
                result.push_str(&format!("[{}]({href})", emit_inline(content)));
            }
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
        fn when_content_has_emphasis_then_emits_emphasis_markers() {
            let content = vec![
                Inline::Text("Hello ".to_string()),
                Inline::Emphasis(vec![Inline::Text("World".to_string())]),
            ];

            assert_eq!(emit_heading(1, &content), "# Hello *World*");
        }

        #[test]
        fn when_content_has_link_then_emits_link_syntax() {
            let content = vec![Inline::Link {
                href: "https://example.com".to_string(),
                content: vec![Inline::Text("here".to_string())],
            }];

            assert_eq!(emit_heading(2, &content), "## [here](https://example.com)");
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
        fn when_content_has_emphasis_then_emits_emphasis_markers() {
            let content = vec![
                Inline::Text("Hello ".to_string()),
                Inline::Emphasis(vec![Inline::Text("world".to_string())]),
                Inline::Text(".".to_string()),
            ];

            assert_eq!(emit_paragraph(&content), "Hello *world*.");
        }

        #[test]
        fn when_content_has_strong_then_emits_strong_markers() {
            let content = vec![Inline::Strong(vec![Inline::Text("important".to_string())])];

            assert_eq!(emit_paragraph(&content), "**important**");
        }

        #[test]
        fn when_content_has_link_then_emits_link_syntax() {
            let content = vec![
                Inline::Text("Visit ".to_string()),
                Inline::Link {
                    href: "https://example.com".to_string(),
                    content: vec![Inline::Text("here".to_string())],
                },
            ];

            assert_eq!(
                emit_paragraph(&content),
                "Visit [here](https://example.com)"
            );
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
        fn when_item_has_emphasis_then_emits_emphasis_markers() {
            let items = vec![ListItem {
                content: vec![
                    Inline::Text("Hello ".to_string()),
                    Inline::Emphasis(vec![Inline::Text("World".to_string())]),
                ],
            }];

            assert_eq!(emit_list(false, &items), "- Hello *World*");
        }

        #[test]
        fn when_item_has_link_then_emits_link_syntax() {
            let items = vec![ListItem {
                content: vec![
                    Inline::Text("See ".to_string()),
                    Inline::Link {
                        href: "chapter2.xhtml".to_string(),
                        content: vec![Inline::Text("Chapter 2".to_string())],
                    },
                ],
            }];

            assert_eq!(
                emit_list(true, &items),
                "1. See [Chapter 2](chapter2.xhtml)"
            );
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

    mod emit_table {
        use super::*;

        fn cell(text: &str) -> Vec<Inline> {
            vec![Inline::Text(text.to_string())]
        }

        #[test]
        fn when_headers_and_rows_then_emits_gfm_table() {
            let headers = vec![cell("Name"), cell("Age")];
            let rows = vec![
                vec![cell("Alice"), cell("30")],
                vec![cell("Bob"), cell("25")],
            ];

            assert_eq!(
                emit_table(&headers, &rows),
                "| Name | Age |\n| --- | --- |\n| Alice | 30 |\n| Bob | 25 |"
            );
        }

        #[test]
        fn when_single_column_then_emits_single_column_table() {
            let headers = vec![cell("Key")];
            let rows = vec![vec![cell("Value")]];

            assert_eq!(emit_table(&headers, &rows), "| Key |\n| --- |\n| Value |");
        }

        #[test]
        fn when_no_headers_then_emits_blank_header_row() {
            let rows = vec![vec![cell("A"), cell("B")]];

            assert_eq!(emit_table(&[], &rows), "|  |  |\n| --- | --- |\n| A | B |");
        }

        #[test]
        fn when_headers_only_then_emits_header_and_separator() {
            let headers = vec![cell("Name"), cell("Age")];

            assert_eq!(emit_table(&headers, &[]), "| Name | Age |\n| --- | --- |");
        }

        #[test]
        fn when_row_has_fewer_cells_then_pads_with_empty_cells() {
            let headers = vec![cell("A"), cell("B"), cell("C")];
            let rows = vec![vec![cell("1")]];

            assert_eq!(
                emit_table(&headers, &rows),
                "| A | B | C |\n| --- | --- | --- |\n| 1 |  |  |"
            );
        }

        #[test]
        fn when_row_has_more_cells_than_headers_then_widens_table() {
            let headers = vec![cell("A")];
            let rows = vec![vec![cell("1"), cell("2")]];

            assert_eq!(
                emit_table(&headers, &rows),
                "| A |  |\n| --- | --- |\n| 1 | 2 |"
            );
        }

        #[test]
        fn when_cell_has_inline_markup_then_emits_markup() {
            let headers = vec![cell("Link")];
            let rows = vec![vec![vec![Inline::Link {
                href: "x.xhtml".to_string(),
                content: vec![Inline::Text("here".to_string())],
            }]]];

            assert_eq!(
                emit_table(&headers, &rows),
                "| Link |\n| --- |\n| [here](x.xhtml) |"
            );
        }

        #[test]
        fn when_cell_contains_pipe_then_escapes_it() {
            let headers = vec![cell("Expr")];
            let rows = vec![vec![cell("a | b")]];

            assert_eq!(
                emit_table(&headers, &rows),
                "| Expr |\n| --- |\n| a \\| b |"
            );
        }

        #[test]
        fn when_cell_is_empty_then_emits_blank_cell() {
            let headers = vec![cell("A"), cell("B")];
            let rows = vec![vec![vec![], cell("2")]];

            assert_eq!(
                emit_table(&headers, &rows),
                "| A | B |\n| --- | --- |\n|  | 2 |"
            );
        }

        #[test]
        fn when_no_headers_and_no_rows_then_emits_empty_string() {
            assert_eq!(emit_table(&[], &[]), "");
        }
    }

    mod emit_image {
        use super::*;

        #[test]
        fn when_src_and_alt_then_emits_image_syntax() {
            assert_eq!(emit_image("cover.jpg", "Cover"), "![Cover](cover.jpg)");
        }

        #[test]
        fn when_alt_is_empty_then_emits_empty_brackets() {
            assert_eq!(emit_image("figure1.png", ""), "![](figure1.png)");
        }

        #[test]
        fn when_nested_path_then_emits_full_path() {
            assert_eq!(
                emit_image("Images/ch1/figure1.png", "Figure 1"),
                "![Figure 1](Images/ch1/figure1.png)"
            );
        }
    }

    mod collect_image_sources {
        use super::*;

        fn image(src: &str) -> Block {
            Block::Image {
                src: src.to_string(),
                alt: String::new(),
            }
        }

        #[test]
        fn when_blocks_have_images_then_collects_sources_in_order() {
            let blocks = vec![image("a.png"), image("b.png")];

            assert_eq!(collect_image_sources(&blocks), vec!["a.png", "b.png"]);
        }

        #[test]
        fn when_same_source_repeats_then_collects_it_once() {
            let blocks = vec![image("a.png"), image("b.png"), image("a.png")];

            assert_eq!(collect_image_sources(&blocks), vec!["a.png", "b.png"]);
        }

        #[test]
        fn when_non_image_blocks_present_then_ignores_them() {
            let blocks = vec![
                Block::Paragraph {
                    content: vec![Inline::Text("text".to_string())],
                },
                image("a.png"),
                Block::Heading {
                    level: 1,
                    content: vec![Inline::Text("Title".to_string())],
                },
            ];

            assert_eq!(collect_image_sources(&blocks), vec!["a.png"]);
        }

        #[test]
        fn when_no_images_then_returns_empty() {
            let blocks = vec![Block::Paragraph {
                content: vec![Inline::Text("text".to_string())],
            }];

            assert_eq!(collect_image_sources(&blocks), Vec::<String>::new());
        }

        #[test]
        fn when_no_blocks_then_returns_empty() {
            assert_eq!(collect_image_sources(&[]), Vec::<String>::new());
        }
    }

    mod emit_inline {
        use super::*;

        #[test]
        fn when_plain_text_then_emits_text_as_is() {
            let content = vec![Inline::Text("Hello".to_string())];

            assert_eq!(emit_inline(&content), "Hello");
        }

        #[test]
        fn when_emphasis_then_wraps_in_single_asterisks() {
            let content = vec![Inline::Emphasis(vec![Inline::Text("World".to_string())])];

            assert_eq!(emit_inline(&content), "*World*");
        }

        #[test]
        fn when_strong_then_wraps_in_double_asterisks() {
            let content = vec![Inline::Strong(vec![Inline::Text("World".to_string())])];

            assert_eq!(emit_inline(&content), "**World**");
        }

        #[test]
        fn when_link_then_emits_bracket_paren_syntax() {
            let content = vec![Inline::Link {
                href: "https://example.com".to_string(),
                content: vec![Inline::Text("here".to_string())],
            }];

            assert_eq!(emit_inline(&content), "[here](https://example.com)");
        }

        #[test]
        fn when_strong_nested_in_emphasis_then_nests_markers() {
            let content = vec![Inline::Emphasis(vec![
                Inline::Text("very ".to_string()),
                Inline::Strong(vec![Inline::Text("important".to_string())]),
            ])];

            assert_eq!(emit_inline(&content), "*very **important***");
        }

        #[test]
        fn when_emphasis_nested_in_link_then_nests_markers() {
            let content = vec![Inline::Link {
                href: "x.xhtml".to_string(),
                content: vec![Inline::Emphasis(vec![Inline::Text("Chapter".to_string())])],
            }];

            assert_eq!(emit_inline(&content), "[*Chapter*](x.xhtml)");
        }

        #[test]
        fn when_multiple_inlines_in_sequence_then_emits_all_in_order() {
            let content = vec![
                Inline::Text("A ".to_string()),
                Inline::Emphasis(vec![Inline::Text("B".to_string())]),
                Inline::Text(" C ".to_string()),
                Inline::Strong(vec![Inline::Text("D".to_string())]),
            ];

            assert_eq!(emit_inline(&content), "A *B* C **D**");
        }

        #[test]
        fn when_emphasis_content_is_empty_then_omits_markers() {
            let content = vec![Inline::Emphasis(vec![])];

            assert_eq!(emit_inline(&content), "");
        }

        #[test]
        fn when_strong_content_is_empty_then_omits_markers() {
            let content = vec![Inline::Strong(vec![])];

            assert_eq!(emit_inline(&content), "");
        }

        #[test]
        fn when_link_content_is_empty_then_still_emits_href() {
            let content = vec![Inline::Link {
                href: "cover.jpg".to_string(),
                content: vec![],
            }];

            assert_eq!(emit_inline(&content), "[](cover.jpg)");
        }

        #[test]
        fn when_content_is_empty_then_emits_empty_string() {
            assert_eq!(emit_inline(&[]), "");
        }
    }
}
