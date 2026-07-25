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

        let content = parse_inline(&xml[content_start..content_end]);
        blocks.push(Block::Heading { level, content });

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

        let content = parse_inline(&xml[content_start..content_end]);
        blocks.push(Block::Paragraph { content });

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

        let content = parse_inline(&xml[content_start..content_end]);
        items.push(ListItem { content });

        search_from = content_end + "</li>".len();
    }

    items
}

pub fn parse_tables(xhtml: &[u8]) -> Vec<Block> {
    let xml = match core::str::from_utf8(xhtml) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let mut blocks = Vec::new();
    let mut search_from = 0;
    while let Some(tag_start) = find_open_tag(xml, "table", search_from) {
        let tag_end = match xml[tag_start..].find('>') {
            Some(pos) => tag_start + pos,
            None => break,
        };

        let content_start = tag_end + 1;
        let content_end = match xml[content_start..].find("</table>") {
            Some(pos) => content_start + pos,
            None => {
                search_from = tag_end + 1;
                continue;
            }
        };

        if let Some(block) = parse_table_content(&xml[content_start..content_end]) {
            blocks.push(block);
        }

        search_from = content_end + "</table>".len();
    }

    blocks
}

fn parse_table_content(xml: &str) -> Option<Block> {
    let mut headers: Vec<Vec<Inline>> = Vec::new();
    let mut rows: Vec<Vec<Vec<Inline>>> = Vec::new();

    let mut search_from = 0;
    while let Some(tag_start) = find_open_tag(xml, "tr", search_from) {
        let tag_end = match xml[tag_start..].find('>') {
            Some(pos) => tag_start + pos,
            None => break,
        };

        let content_start = tag_end + 1;
        let content_end = match xml[content_start..].find("</tr>") {
            Some(pos) => content_start + pos,
            None => {
                search_from = tag_end + 1;
                continue;
            }
        };

        let (cells, has_header_cell) = parse_row_cells(&xml[content_start..content_end]);
        if has_header_cell && headers.is_empty() && rows.is_empty() {
            headers = cells;
        } else if !cells.is_empty() {
            rows.push(cells);
        }

        search_from = content_end + "</tr>".len();
    }

    if headers.is_empty() && rows.is_empty() {
        return None;
    }

    Some(Block::Table { headers, rows })
}

fn parse_row_cells(xml: &str) -> (Vec<Vec<Inline>>, bool) {
    let mut cells = Vec::new();
    let mut has_header_cell = false;

    let mut search_from = 0;
    loop {
        let (tag_start, tag_name) = match (
            find_open_tag(xml, "th", search_from),
            find_open_tag(xml, "td", search_from),
        ) {
            (Some(h), Some(d)) if h < d => (h, "th"),
            (Some(_), Some(d)) => (d, "td"),
            (Some(h), None) => (h, "th"),
            (None, Some(d)) => (d, "td"),
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

        if tag_name == "th" {
            has_header_cell = true;
        }
        cells.push(parse_inline(&xml[content_start..content_end]));

        search_from = content_end + close_tag.len();
    }

    (cells, has_header_cell)
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

pub fn parse_inline(xml: &str) -> Vec<Inline> {
    let mut inlines = Vec::new();
    let mut text_buf = String::new();
    let mut pos = 0;

    while pos < xml.len() {
        match xml[pos..].find('<') {
            None => {
                text_buf.push_str(&xml[pos..]);
                break;
            }
            Some(rel) => {
                let lt = pos + rel;
                text_buf.push_str(&xml[pos..lt]);

                match match_recognized_open_tag(xml, lt) {
                    Some((tag, href, content_start)) => {
                        let close_tag = format!("</{tag}>");
                        match xml[content_start..].find(&close_tag) {
                            Some(close_rel) => {
                                if !text_buf.is_empty() {
                                    inlines.push(Inline::Text(core::mem::take(&mut text_buf)));
                                }
                                let content_end = content_start + close_rel;
                                let inner = parse_inline(&xml[content_start..content_end]);
                                inlines.push(match tag {
                                    "em" => Inline::Emphasis(inner),
                                    "strong" => Inline::Strong(inner),
                                    "a" => Inline::Link {
                                        href: href.unwrap_or_default(),
                                        content: inner,
                                    },
                                    _ => unreachable!(),
                                });
                                pos = content_end + close_tag.len();
                            }
                            None => pos = content_start,
                        }
                    }
                    None => match xml[lt..].find('>') {
                        Some(gt_rel) => pos = lt + gt_rel + 1,
                        None => {
                            text_buf.push_str(&xml[lt..]);
                            break;
                        }
                    },
                }
            }
        }
    }

    if !text_buf.is_empty() {
        inlines.push(Inline::Text(text_buf));
    }

    trim_edges(inlines)
}

fn trim_edges(mut inlines: Vec<Inline>) -> Vec<Inline> {
    if let Some(Inline::Text(first)) = inlines.first_mut() {
        *first = first.trim_start().to_string();
    }
    if let Some(Inline::Text(last)) = inlines.last_mut() {
        *last = last.trim_end().to_string();
    }
    inlines.retain(|inline| !matches!(inline, Inline::Text(text) if text.is_empty()));
    inlines
}

fn match_recognized_open_tag(
    xml: &str,
    lt: usize,
) -> Option<(&'static str, Option<String>, usize)> {
    let rest = &xml[lt + 1..];
    for tag in ["strong", "em", "a"] {
        if rest.len() > tag.len()
            && rest.starts_with(tag)
            && matches!(
                rest.as_bytes()[tag.len()],
                b' ' | b'\t' | b'\n' | b'\r' | b'>'
            )
        {
            let tag_end_rel = rest.find('>')?;
            let full_tag = &rest[..tag_end_rel];
            let href = if tag == "a" {
                extract_attribute(full_tag, "href").map(|s| s.to_string())
            } else {
                None
            };
            let content_start = lt + 1 + tag_end_rel + 1;
            return Some((tag, href, content_start));
        }
    }
    None
}

fn extract_attribute<'a>(tag: &'a str, attr_name: &str) -> Option<&'a str> {
    let mut search = tag;
    loop {
        let pos = search.find(attr_name)?;
        let before = if pos > 0 {
            search.as_bytes()[pos - 1]
        } else {
            b' '
        };
        if !matches!(before, b' ' | b'\t' | b'\n' | b'\r') {
            search = &search[pos + attr_name.len()..];
            continue;
        }

        let after_name = &search[pos + attr_name.len()..];
        let after_name = after_name.trim_start();
        if !after_name.starts_with('=') {
            search = after_name;
            continue;
        }
        let after_eq = after_name[1..].trim_start();
        let quote = after_eq.as_bytes().first()?;
        if *quote != b'"' && *quote != b'\'' {
            return None;
        }
        let value_start = &after_eq[1..];
        let end = value_start.find(*quote as char)?;
        return Some(&value_start[..end]);
    }
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
        fn when_heading_has_unrecognized_nested_tags_then_strips_them_from_text() {
            let xhtml = b"<h1>Hello <span>World</span></h1>";

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
        fn when_heading_has_emphasis_then_returns_structured_inline() {
            let xhtml = b"<h1>Hello <em>World</em></h1>";

            let blocks = parse_headings(xhtml);

            assert_eq!(
                blocks,
                vec![Block::Heading {
                    level: 1,
                    content: vec![
                        Inline::Text("Hello ".to_string()),
                        Inline::Emphasis(vec![Inline::Text("World".to_string())]),
                    ],
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
        fn when_paragraph_has_unrecognized_nested_tags_then_strips_them_from_text() {
            let xhtml = b"<p>Hello <span>World</span>.</p>";

            let blocks = parse_paragraphs(xhtml);

            assert_eq!(
                blocks,
                vec![Block::Paragraph {
                    content: vec![Inline::Text("Hello World.".to_string())],
                }]
            );
        }

        #[test]
        fn when_paragraph_has_link_and_strong_then_returns_structured_inline() {
            let xhtml = br#"<p>Visit <a href="https://example.com">here</a> for <strong>details</strong>.</p>"#;

            let blocks = parse_paragraphs(xhtml);

            assert_eq!(
                blocks,
                vec![Block::Paragraph {
                    content: vec![
                        Inline::Text("Visit ".to_string()),
                        Inline::Link {
                            href: "https://example.com".to_string(),
                            content: vec![Inline::Text("here".to_string())],
                        },
                        Inline::Text(" for ".to_string()),
                        Inline::Strong(vec![Inline::Text("details".to_string())]),
                        Inline::Text(".".to_string()),
                    ],
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
        fn when_list_item_has_unrecognized_nested_tags_then_strips_them_from_text() {
            let xhtml = b"<ul><li>Hello <span>World</span></li></ul>";

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
        fn when_list_item_has_link_then_returns_structured_inline() {
            let xhtml = br#"<ul><li>See <a href="chapter2.xhtml">Chapter 2</a></li></ul>"#;

            let blocks = parse_lists(xhtml);

            assert_eq!(
                blocks,
                vec![Block::List {
                    ordered: false,
                    items: vec![ListItem {
                        content: vec![
                            Inline::Text("See ".to_string()),
                            Inline::Link {
                                href: "chapter2.xhtml".to_string(),
                                content: vec![Inline::Text("Chapter 2".to_string())],
                            },
                        ],
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

    mod parse_tables {
        use super::*;

        #[test]
        fn when_table_with_header_and_rows_then_returns_table_block() {
            let xhtml = b"<table>
  <tr><th>Name</th><th>Age</th></tr>
  <tr><td>Alice</td><td>30</td></tr>
  <tr><td>Bob</td><td>25</td></tr>
</table>";

            let blocks = parse_tables(xhtml);

            assert_eq!(
                blocks,
                vec![Block::Table {
                    headers: vec![
                        vec![Inline::Text("Name".to_string())],
                        vec![Inline::Text("Age".to_string())],
                    ],
                    rows: vec![
                        vec![
                            vec![Inline::Text("Alice".to_string())],
                            vec![Inline::Text("30".to_string())],
                        ],
                        vec![
                            vec![Inline::Text("Bob".to_string())],
                            vec![Inline::Text("25".to_string())],
                        ],
                    ],
                }]
            );
        }

        #[test]
        fn when_table_wrapped_in_thead_and_tbody_then_still_parsed() {
            let xhtml = b"<table>
  <thead><tr><th>Key</th></tr></thead>
  <tbody><tr><td>Value</td></tr></tbody>
</table>";

            let blocks = parse_tables(xhtml);

            assert_eq!(
                blocks,
                vec![Block::Table {
                    headers: vec![vec![Inline::Text("Key".to_string())]],
                    rows: vec![vec![vec![Inline::Text("Value".to_string())]]],
                }]
            );
        }

        #[test]
        fn when_table_has_no_header_row_then_returns_empty_headers() {
            let xhtml = b"<table><tr><td>A</td><td>B</td></tr></table>";

            let blocks = parse_tables(xhtml);

            assert_eq!(
                blocks,
                vec![Block::Table {
                    headers: vec![],
                    rows: vec![vec![
                        vec![Inline::Text("A".to_string())],
                        vec![Inline::Text("B".to_string())],
                    ]],
                }]
            );
        }

        #[test]
        fn when_cell_has_inline_markup_then_returns_structured_inline() {
            let xhtml = br#"<table><tr><td>See <a href="x.xhtml">link</a></td></tr></table>"#;

            let blocks = parse_tables(xhtml);

            assert_eq!(
                blocks,
                vec![Block::Table {
                    headers: vec![],
                    rows: vec![vec![vec![
                        Inline::Text("See ".to_string()),
                        Inline::Link {
                            href: "x.xhtml".to_string(),
                            content: vec![Inline::Text("link".to_string())],
                        },
                    ]]],
                }]
            );
        }

        #[test]
        fn when_table_has_attributes_then_still_parsed() {
            let xhtml = br#"<table class="data"><tr><td>Cell</td></tr></table>"#;

            let blocks = parse_tables(xhtml);

            assert_eq!(
                blocks,
                vec![Block::Table {
                    headers: vec![],
                    rows: vec![vec![vec![Inline::Text("Cell".to_string())]]],
                }]
            );
        }

        #[test]
        fn when_multiple_tables_then_returns_in_document_order() {
            let xhtml =
                b"<table><tr><td>A</td></tr></table><p>gap</p><table><tr><td>B</td></tr></table>";

            let blocks = parse_tables(xhtml);

            assert_eq!(
                blocks,
                vec![
                    Block::Table {
                        headers: vec![],
                        rows: vec![vec![vec![Inline::Text("A".to_string())]]],
                    },
                    Block::Table {
                        headers: vec![],
                        rows: vec![vec![vec![Inline::Text("B".to_string())]]],
                    },
                ]
            );
        }

        #[test]
        fn when_header_row_appears_after_data_row_then_treated_as_data_row() {
            let xhtml = b"<table><tr><td>A</td></tr><tr><th>H</th></tr></table>";

            let blocks = parse_tables(xhtml);

            assert_eq!(
                blocks,
                vec![Block::Table {
                    headers: vec![],
                    rows: vec![
                        vec![vec![Inline::Text("A".to_string())]],
                        vec![vec![Inline::Text("H".to_string())]],
                    ],
                }]
            );
        }

        #[test]
        fn when_table_is_empty_then_returns_empty() {
            let xhtml = b"<table></table>";

            assert_eq!(parse_tables(xhtml), vec![]);
        }

        #[test]
        fn when_no_tables_then_returns_empty() {
            let xhtml = b"<p>No tables here</p>";

            assert_eq!(parse_tables(xhtml), vec![]);
        }

        #[test]
        fn when_invalid_utf8_then_returns_empty() {
            let xhtml = b"\xff\xfe<table><tr><td>Bad</td></tr></table>";

            assert_eq!(parse_tables(xhtml), vec![]);
        }
    }

    mod parse_inline {
        use super::*;

        #[test]
        fn when_plain_text_then_returns_single_text_inline() {
            let inlines = parse_inline("Hello world");

            assert_eq!(inlines, vec![Inline::Text("Hello world".to_string())]);
        }

        #[test]
        fn when_link_then_returns_link_inline_with_href() {
            let inlines = parse_inline(r#"<a href="https://example.com">here</a>"#);

            assert_eq!(
                inlines,
                vec![Inline::Link {
                    href: "https://example.com".to_string(),
                    content: vec![Inline::Text("here".to_string())],
                }]
            );
        }

        #[test]
        fn when_link_has_no_href_then_returns_empty_href() {
            let inlines = parse_inline("<a>here</a>");

            assert_eq!(
                inlines,
                vec![Inline::Link {
                    href: String::new(),
                    content: vec![Inline::Text("here".to_string())],
                }]
            );
        }

        #[test]
        fn when_emphasis_then_returns_emphasis_inline() {
            let inlines = parse_inline("<em>World</em>");

            assert_eq!(
                inlines,
                vec![Inline::Emphasis(vec![Inline::Text("World".to_string())])]
            );
        }

        #[test]
        fn when_strong_then_returns_strong_inline() {
            let inlines = parse_inline("<strong>World</strong>");

            assert_eq!(
                inlines,
                vec![Inline::Strong(vec![Inline::Text("World".to_string())])]
            );
        }

        #[test]
        fn when_strong_nested_in_emphasis_then_returns_nested_inline() {
            let inlines = parse_inline("<em>very <strong>important</strong></em>");

            assert_eq!(
                inlines,
                vec![Inline::Emphasis(vec![
                    Inline::Text("very ".to_string()),
                    Inline::Strong(vec![Inline::Text("important".to_string())]),
                ])]
            );
        }

        #[test]
        fn when_emphasis_nested_in_link_then_returns_nested_inline() {
            let inlines = parse_inline(r#"<a href="x.xhtml"><em>Chapter</em></a>"#);

            assert_eq!(
                inlines,
                vec![Inline::Link {
                    href: "x.xhtml".to_string(),
                    content: vec![Inline::Emphasis(vec![Inline::Text("Chapter".to_string())])],
                }]
            );
        }

        #[test]
        fn when_unrecognized_tag_then_strips_tag_but_keeps_text() {
            let inlines = parse_inline("Hello <span>World</span>");

            assert_eq!(inlines, vec![Inline::Text("Hello World".to_string())]);
        }

        #[test]
        fn when_multiple_inlines_in_sequence_then_returns_all_in_order() {
            let inlines = parse_inline("A <em>B</em> C <strong>D</strong> E");

            assert_eq!(
                inlines,
                vec![
                    Inline::Text("A ".to_string()),
                    Inline::Emphasis(vec![Inline::Text("B".to_string())]),
                    Inline::Text(" C ".to_string()),
                    Inline::Strong(vec![Inline::Text("D".to_string())]),
                    Inline::Text(" E".to_string()),
                ]
            );
        }

        #[test]
        fn when_empty_string_then_returns_empty() {
            assert_eq!(parse_inline(""), vec![]);
        }

        #[test]
        fn when_unclosed_recognized_tag_then_ignored() {
            let inlines = parse_inline("Hello <em>World");

            assert_eq!(inlines, vec![Inline::Text("Hello World".to_string())]);
        }

        #[test]
        fn when_surrounding_whitespace_then_trims_outer_edges_only() {
            let inlines = parse_inline("\n  Hello <em>World</em>  \n");

            assert_eq!(
                inlines,
                vec![
                    Inline::Text("Hello ".to_string()),
                    Inline::Emphasis(vec![Inline::Text("World".to_string())]),
                ]
            );
        }

        #[test]
        fn when_trailing_whitespace_becomes_empty_after_trim_then_removed() {
            let inlines = parse_inline("<em>World</em>  \n");

            assert_eq!(
                inlines,
                vec![Inline::Emphasis(vec![Inline::Text("World".to_string())])]
            );
        }
    }
}
