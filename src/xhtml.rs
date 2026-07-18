use crate::ir::{Block, Inline, ListItem};

pub fn parse_headings(xhtml: &[u8]) -> Vec<Block> {
    let xml = match core::str::from_utf8(xhtml) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let mut blocks = Vec::new();
    let mut search_from = 0;
    while let Some((tag_start, level)) = find_heading_start(xml, search_from) {
        let tag_end = match xml[tag_start..].find('>') {
            Some(pos) => tag_start + pos,
            None => break,
        };

        let close_tag = format!("</h{level}>");
        let content_start = tag_end + 1;
        let content_end = match xml[content_start..].find(&close_tag) {
            Some(pos) => content_start + pos,
            None => {
                search_from = tag_end + 1;
                continue;
            }
        };

        let text = strip_tags(&xml[content_start..content_end]);
        blocks.push(Block::Heading {
            level,
            content: vec![Inline::Text(text)],
        });

        search_from = content_end + close_tag.len();
    }

    blocks
}

pub fn parse_paragraphs(xhtml: &[u8]) -> Vec<Block> {
    let xml = match core::str::from_utf8(xhtml) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let mut blocks = Vec::new();
    let mut search_from = 0;
    while let Some(tag_start) = find_open_tag(xml, "p", search_from) {
        let tag_end = match xml[tag_start..].find('>') {
            Some(pos) => tag_start + pos,
            None => break,
        };

        let content_start = tag_end + 1;
        let content_end = match xml[content_start..].find("</p>") {
            Some(pos) => content_start + pos,
            None => {
                search_from = tag_end + 1;
                continue;
            }
        };

        let text = strip_tags(&xml[content_start..content_end]);
        blocks.push(Block::Paragraph {
            content: vec![Inline::Text(text)],
        });

        search_from = content_end + "</p>".len();
    }

    blocks
}

pub fn parse_lists(xhtml: &[u8]) -> Vec<Block> {
    let xml = match core::str::from_utf8(xhtml) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let mut blocks = Vec::new();
    let mut search_from = 0;
    loop {
        let (tag_start, ordered, tag_name) = match (
            find_open_tag(xml, "ul", search_from),
            find_open_tag(xml, "ol", search_from),
        ) {
            (Some(u), Some(o)) if u < o => (u, false, "ul"),
            (Some(_), Some(o)) => (o, true, "ol"),
            (Some(u), None) => (u, false, "ul"),
            (None, Some(o)) => (o, true, "ol"),
            (None, None) => break,
        };

        let tag_end = match xml[tag_start..].find('>') {
            Some(pos) => tag_start + pos,
            None => break,
        };

        let close_tag = format!("</{tag_name}>");
        let content_start = tag_end + 1;
        let content_end = match xml[content_start..].find(&close_tag) {
            Some(pos) => content_start + pos,
            None => {
                search_from = tag_end + 1;
                continue;
            }
        };

        let items = parse_list_items(&xml[content_start..content_end]);
        if !items.is_empty() {
            blocks.push(Block::List { ordered, items });
        }

        search_from = content_end + close_tag.len();
    }

    blocks
}

fn parse_list_items(xml: &str) -> Vec<ListItem> {
    let mut items = Vec::new();
    let mut search_from = 0;
    while let Some(tag_start) = find_open_tag(xml, "li", search_from) {
        let tag_end = match xml[tag_start..].find('>') {
            Some(pos) => tag_start + pos,
            None => break,
        };

        let content_start = tag_end + 1;
        let content_end = match xml[content_start..].find("</li>") {
            Some(pos) => content_start + pos,
            None => {
                search_from = tag_end + 1;
                continue;
            }
        };

        let text = strip_tags(&xml[content_start..content_end]);
        items.push(ListItem {
            content: vec![Inline::Text(text)],
        });

        search_from = content_end + "</li>".len();
    }

    items
}

fn find_open_tag(xml: &str, tag_name: &str, from: usize) -> Option<usize> {
    let haystack = &xml[from..];
    let mut pos = 0;
    while pos < haystack.len() {
        let lt = haystack[pos..].find('<')?;
        let abs = pos + lt;
        let after_lt = abs + 1;
        if after_lt >= haystack.len() {
            return None;
        }
        let rest = &haystack[after_lt..];
        if rest.starts_with(tag_name)
            && rest.len() > tag_name.len()
            && matches!(
                rest.as_bytes()[tag_name.len()],
                b' ' | b'\t' | b'\n' | b'\r' | b'/' | b'>'
            )
        {
            return Some(from + abs);
        }
        pos = abs + 1;
    }
    None
}

fn find_heading_start(xml: &str, from: usize) -> Option<(usize, u8)> {
    let haystack = &xml[from..];
    let mut pos = 0;
    while pos < haystack.len() {
        let lt = haystack[pos..].find('<')?;
        let abs = pos + lt;
        let after_lt = abs + 1;
        if after_lt >= haystack.len() {
            return None;
        }
        let rest_bytes = &haystack.as_bytes()[after_lt..];
        if rest_bytes.len() >= 3 && rest_bytes[0] == b'h' && rest_bytes[1].is_ascii_digit() {
            let level = rest_bytes[1] - b'0';
            if (1..=6).contains(&level)
                && matches!(rest_bytes[2], b' ' | b'\t' | b'\n' | b'\r' | b'>')
            {
                return Some((from + abs, level));
            }
        }
        pos = abs + 1;
    }
    None
}

fn strip_tags(s: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(c),
            _ => {}
        }
    }
    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    mod parse_headings {
        use super::*;

        #[test]
        fn when_single_h1_then_returns_heading_block() {
            let xhtml = b"<h1>Chapter One</h1>";

            let blocks = parse_headings(xhtml);

            assert_eq!(
                blocks,
                vec![Block::Heading {
                    level: 1,
                    content: vec![Inline::Text("Chapter One".to_string())],
                }]
            );
        }

        #[test]
        fn when_multiple_headings_then_returns_in_document_order() {
            let xhtml = b"<h1>Title</h1><p>ignored</p><h2>Subtitle</h2>";

            let blocks = parse_headings(xhtml);

            assert_eq!(
                blocks,
                vec![
                    Block::Heading {
                        level: 1,
                        content: vec![Inline::Text("Title".to_string())],
                    },
                    Block::Heading {
                        level: 2,
                        content: vec![Inline::Text("Subtitle".to_string())],
                    },
                ]
            );
        }

        #[test]
        fn when_heading_has_attributes_then_still_parsed() {
            let xhtml = br#"<h2 class="chapter" id="ch1">Section</h2>"#;

            let blocks = parse_headings(xhtml);

            assert_eq!(
                blocks,
                vec![Block::Heading {
                    level: 2,
                    content: vec![Inline::Text("Section".to_string())],
                }]
            );
        }

        #[test]
        fn when_heading_has_nested_tags_then_strips_them_from_text() {
            let xhtml = b"<h1>Hello <em>World</em></h1>";

            let blocks = parse_headings(xhtml);

            assert_eq!(
                blocks,
                vec![Block::Heading {
                    level: 1,
                    content: vec![Inline::Text("Hello World".to_string())],
                }]
            );
        }

        #[test]
        fn when_heading_has_surrounding_whitespace_then_trims_it() {
            let xhtml = b"<h3>\n  Padded  \n</h3>";

            let blocks = parse_headings(xhtml);

            assert_eq!(
                blocks,
                vec![Block::Heading {
                    level: 3,
                    content: vec![Inline::Text("Padded".to_string())],
                }]
            );
        }

        #[test]
        fn when_no_headings_then_returns_empty() {
            let xhtml = b"<p>No headings here</p>";

            assert_eq!(parse_headings(xhtml), vec![]);
        }

        #[test]
        fn when_similarly_named_tag_then_not_mistaken_for_heading() {
            let xhtml = b"<header>Not a heading</header>";

            assert_eq!(parse_headings(xhtml), vec![]);
        }

        #[test]
        fn when_invalid_utf8_then_returns_empty() {
            let xhtml = b"\xff\xfe<h1>Bad</h1>";

            assert_eq!(parse_headings(xhtml), vec![]);
        }

        #[test]
        fn when_all_six_heading_levels_present_then_parses_each() {
            let xhtml = b"<h1>A</h1><h2>B</h2><h3>C</h3><h4>D</h4><h5>E</h5><h6>F</h6>";

            let blocks = parse_headings(xhtml);

            assert_eq!(
                blocks,
                vec![
                    Block::Heading {
                        level: 1,
                        content: vec![Inline::Text("A".to_string())]
                    },
                    Block::Heading {
                        level: 2,
                        content: vec![Inline::Text("B".to_string())]
                    },
                    Block::Heading {
                        level: 3,
                        content: vec![Inline::Text("C".to_string())]
                    },
                    Block::Heading {
                        level: 4,
                        content: vec![Inline::Text("D".to_string())]
                    },
                    Block::Heading {
                        level: 5,
                        content: vec![Inline::Text("E".to_string())]
                    },
                    Block::Heading {
                        level: 6,
                        content: vec![Inline::Text("F".to_string())]
                    },
                ]
            );
        }
    }

    mod parse_paragraphs {
        use super::*;

        #[test]
        fn when_single_paragraph_then_returns_paragraph_block() {
            let xhtml = b"<p>Hello world.</p>";

            let blocks = parse_paragraphs(xhtml);

            assert_eq!(
                blocks,
                vec![Block::Paragraph {
                    content: vec![Inline::Text("Hello world.".to_string())],
                }]
            );
        }

        #[test]
        fn when_multiple_paragraphs_then_returns_in_document_order() {
            let xhtml = b"<h1>Title</h1><p>First.</p><p>Second.</p>";

            let blocks = parse_paragraphs(xhtml);

            assert_eq!(
                blocks,
                vec![
                    Block::Paragraph {
                        content: vec![Inline::Text("First.".to_string())],
                    },
                    Block::Paragraph {
                        content: vec![Inline::Text("Second.".to_string())],
                    },
                ]
            );
        }

        #[test]
        fn when_paragraph_has_attributes_then_still_parsed() {
            let xhtml = br#"<p class="intro">Welcome.</p>"#;

            let blocks = parse_paragraphs(xhtml);

            assert_eq!(
                blocks,
                vec![Block::Paragraph {
                    content: vec![Inline::Text("Welcome.".to_string())],
                }]
            );
        }

        #[test]
        fn when_paragraph_has_nested_tags_then_strips_them_from_text() {
            let xhtml = b"<p>Hello <em>World</em>.</p>";

            let blocks = parse_paragraphs(xhtml);

            assert_eq!(
                blocks,
                vec![Block::Paragraph {
                    content: vec![Inline::Text("Hello World.".to_string())],
                }]
            );
        }

        #[test]
        fn when_no_paragraphs_then_returns_empty() {
            let xhtml = b"<h1>No paragraphs here</h1>";

            assert_eq!(parse_paragraphs(xhtml), vec![]);
        }

        #[test]
        fn when_invalid_utf8_then_returns_empty() {
            let xhtml = b"\xff\xfe<p>Bad</p>";

            assert_eq!(parse_paragraphs(xhtml), vec![]);
        }
    }

    mod parse_lists {
        use super::*;

        #[test]
        fn when_unordered_list_then_returns_unordered_list_block() {
            let xhtml = b"<ul><li>First</li><li>Second</li></ul>";

            let blocks = parse_lists(xhtml);

            assert_eq!(
                blocks,
                vec![Block::List {
                    ordered: false,
                    items: vec![
                        ListItem {
                            content: vec![Inline::Text("First".to_string())],
                        },
                        ListItem {
                            content: vec![Inline::Text("Second".to_string())],
                        },
                    ],
                }]
            );
        }

        #[test]
        fn when_ordered_list_then_returns_ordered_list_block() {
            let xhtml = b"<ol><li>First</li><li>Second</li></ol>";

            let blocks = parse_lists(xhtml);

            assert_eq!(
                blocks,
                vec![Block::List {
                    ordered: true,
                    items: vec![
                        ListItem {
                            content: vec![Inline::Text("First".to_string())],
                        },
                        ListItem {
                            content: vec![Inline::Text("Second".to_string())],
                        },
                    ],
                }]
            );
        }

        #[test]
        fn when_multiple_lists_then_returns_in_document_order() {
            let xhtml = b"<ul><li>A</li></ul><p>between</p><ol><li>B</li></ol>";

            let blocks = parse_lists(xhtml);

            assert_eq!(
                blocks,
                vec![
                    Block::List {
                        ordered: false,
                        items: vec![ListItem {
                            content: vec![Inline::Text("A".to_string())],
                        }],
                    },
                    Block::List {
                        ordered: true,
                        items: vec![ListItem {
                            content: vec![Inline::Text("B".to_string())],
                        }],
                    },
                ]
            );
        }

        #[test]
        fn when_list_item_has_nested_tags_then_strips_them_from_text() {
            let xhtml = b"<ul><li>Hello <em>World</em></li></ul>";

            let blocks = parse_lists(xhtml);

            assert_eq!(
                blocks,
                vec![Block::List {
                    ordered: false,
                    items: vec![ListItem {
                        content: vec![Inline::Text("Hello World".to_string())],
                    }],
                }]
            );
        }

        #[test]
        fn when_list_has_attributes_then_still_parsed() {
            let xhtml = br#"<ul class="toc"><li>Entry</li></ul>"#;

            let blocks = parse_lists(xhtml);

            assert_eq!(
                blocks,
                vec![Block::List {
                    ordered: false,
                    items: vec![ListItem {
                        content: vec![Inline::Text("Entry".to_string())],
                    }],
                }]
            );
        }

        #[test]
        fn when_list_has_no_items_then_returns_empty() {
            let xhtml = b"<ul></ul>";

            assert_eq!(parse_lists(xhtml), vec![]);
        }

        #[test]
        fn when_no_lists_then_returns_empty() {
            let xhtml = b"<p>No lists here</p>";

            assert_eq!(parse_lists(xhtml), vec![]);
        }

        #[test]
        fn when_invalid_utf8_then_returns_empty() {
            let xhtml = b"\xff\xfe<ul><li>Bad</li></ul>";

            assert_eq!(parse_lists(xhtml), vec![]);
        }
    }
}
