use crate::ir::{
    Anchor, Block, Cell, HeadingLevel, Inline, LinkTarget, ListItem, ListKind, NonEmpty,
    ResourcePath, Table,
};

enum BlockKind {
    Heading(u8),
    Paragraph,
    List(ListKind),
    Table,
    Image,
}

pub fn parse_blocks(xhtml: &[u8], document: &ResourcePath) -> Vec<Block> {
    let base = document.parent();
    let xml = match core::str::from_utf8(xhtml) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let mut blocks = Vec::new();
    let mut pos = 0;
    while let Some((tag_start, kind)) = next_block_start(xml, pos) {
        if let Some(anchor) = element_anchor(xml, tag_start, document) {
            blocks.push(Block::Anchor(anchor));
        }
        pos = match kind {
            BlockKind::Heading(level) => {
                match element_bounds(xml, tag_start, &format!("</h{level}>")) {
                    Some((content_start, content_end, next)) => {
                        if let Some(block) = build_heading(
                            level,
                            parse_inline(&xml[content_start..content_end], document),
                        ) {
                            blocks.push(block);
                        }
                        next
                    }
                    None => skip_open_tag(xml, tag_start),
                }
            }
            BlockKind::Paragraph => match element_bounds(xml, tag_start, "</p>") {
                Some((content_start, content_end, next)) => {
                    let inner = &xml[content_start..content_end];
                    if let Some(content) = NonEmpty::new(parse_inline(inner, document)) {
                        blocks.push(Block::Paragraph { content });
                    }
                    blocks.extend(parse_images(inner.as_bytes(), base));
                    next
                }
                None => skip_open_tag(xml, tag_start),
            },
            BlockKind::List(kind) => {
                let close_tag = match kind {
                    ListKind::Ordered => "</ol>",
                    ListKind::Unordered => "</ul>",
                };
                match element_bounds(xml, tag_start, close_tag) {
                    Some((content_start, content_end, next)) => {
                        let items = parse_list_items(&xml[content_start..content_end], document);
                        if let Some(items) = NonEmpty::new(items) {
                            blocks.push(Block::List { kind, items });
                        }
                        next
                    }
                    None => skip_open_tag(xml, tag_start),
                }
            }
            BlockKind::Table => match element_bounds(xml, tag_start, "</table>") {
                Some((content_start, content_end, next)) => {
                    if let Some(block) =
                        parse_table_content(&xml[content_start..content_end], document)
                    {
                        blocks.push(block);
                    }
                    next
                }
                None => skip_open_tag(xml, tag_start),
            },
            BlockKind::Image => {
                let tag_end = match xml[tag_start..].find('>') {
                    Some(rel) => tag_start + rel,
                    None => break,
                };
                let tag = &xml[tag_start..=tag_end];
                if let Some(src) = extract_attribute(tag, "src") {
                    blocks.push(Block::Image {
                        src: ResourcePath::resolve(base, src),
                        alt: extract_attribute(tag, "alt")
                            .unwrap_or_default()
                            .to_string(),
                    });
                }
                tag_end + 1
            }
        };
    }

    blocks
}

fn next_block_start(xml: &str, from: usize) -> Option<(usize, BlockKind)> {
    let mut candidates = Vec::new();
    if let Some((pos, level)) = find_heading_start(xml, from) {
        candidates.push((pos, BlockKind::Heading(level)));
    }
    for (tag_name, kind) in [
        ("p", BlockKind::Paragraph),
        ("ul", BlockKind::List(ListKind::Unordered)),
        ("ol", BlockKind::List(ListKind::Ordered)),
        ("table", BlockKind::Table),
        ("img", BlockKind::Image),
    ] {
        if let Some(pos) = find_open_tag(xml, tag_name, from) {
            candidates.push((pos, kind));
        }
    }
    candidates.into_iter().min_by_key(|(pos, _)| *pos)
}

fn element_bounds(xml: &str, tag_start: usize, close_tag: &str) -> Option<(usize, usize, usize)> {
    let tag_end = tag_start + xml[tag_start..].find('>')?;
    let content_start = tag_end + 1;
    let content_end = content_start + xml[content_start..].find(close_tag)?;
    Some((content_start, content_end, content_end + close_tag.len()))
}

fn element_anchor(xml: &str, tag_start: usize, document: &ResourcePath) -> Option<Anchor> {
    let tag_end = tag_start + xml[tag_start..].find('>')?;
    let id = extract_attribute(&xml[tag_start..=tag_end], "id")?;
    Some(Anchor::new(document, Some(id)))
}

fn skip_open_tag(xml: &str, tag_start: usize) -> usize {
    match xml[tag_start..].find('>') {
        Some(rel) => tag_start + rel + 1,
        None => xml.len(),
    }
}

pub fn parse_headings(xhtml: &[u8], document: &ResourcePath) -> Vec<Block> {
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

        if let Some(block) = build_heading(
            level,
            parse_inline(&xml[content_start..content_end], document),
        ) {
            blocks.push(block);
        }

        search_from = content_end + close_tag.len();
    }

    blocks
}

pub fn parse_paragraphs(xhtml: &[u8], document: &ResourcePath) -> Vec<Block> {
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

        if let Some(content) =
            NonEmpty::new(parse_inline(&xml[content_start..content_end], document))
        {
            blocks.push(Block::Paragraph { content });
        }

        search_from = content_end + "</p>".len();
    }

    blocks
}

pub fn parse_lists(xhtml: &[u8], document: &ResourcePath) -> Vec<Block> {
    let xml = match core::str::from_utf8(xhtml) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let mut blocks = Vec::new();
    let mut search_from = 0;
    loop {
        let (tag_start, kind, tag_name) = match (
            find_open_tag(xml, "ul", search_from),
            find_open_tag(xml, "ol", search_from),
        ) {
            (Some(u), Some(o)) if u < o => (u, ListKind::Unordered, "ul"),
            (Some(_), Some(o)) => (o, ListKind::Ordered, "ol"),
            (Some(u), None) => (u, ListKind::Unordered, "ul"),
            (None, Some(o)) => (o, ListKind::Ordered, "ol"),
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

        let items = parse_list_items(&xml[content_start..content_end], document);
        if let Some(items) = NonEmpty::new(items) {
            blocks.push(Block::List { kind, items });
        }

        search_from = content_end + close_tag.len();
    }

    blocks
}

fn parse_list_items(xml: &str, document: &ResourcePath) -> Vec<ListItem> {
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

        if let Some(item) = ListItem::new(parse_inline(&xml[content_start..content_end], document))
        {
            items.push(item);
        }

        search_from = content_end + "</li>".len();
    }

    items
}

pub fn parse_tables(xhtml: &[u8], document: &ResourcePath) -> Vec<Block> {
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

        if let Some(block) = parse_table_content(&xml[content_start..content_end], document) {
            blocks.push(block);
        }

        search_from = content_end + "</table>".len();
    }

    blocks
}

fn parse_table_content(xml: &str, document: &ResourcePath) -> Option<Block> {
    let mut headers: Option<Vec<Cell>> = None;
    let mut rows: Vec<Vec<Cell>> = Vec::new();

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

        let (cells, has_header_cell) = parse_row_cells(&xml[content_start..content_end], document);
        if has_header_cell && headers.is_none() && rows.is_empty() {
            headers = Some(cells);
        } else if !cells.is_empty() {
            rows.push(cells);
        }

        search_from = content_end + "</tr>".len();
    }

    Table::new(headers, rows).map(Block::Table)
}

fn parse_row_cells(xml: &str, document: &ResourcePath) -> (Vec<Cell>, bool) {
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
        cells.push(Cell::new(parse_inline(
            &xml[content_start..content_end],
            document,
        )));

        search_from = content_end + close_tag.len();
    }

    (cells, has_header_cell)
}

pub fn parse_images(xhtml: &[u8], base: &str) -> Vec<Block> {
    let xml = match core::str::from_utf8(xhtml) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let mut blocks = Vec::new();
    let mut search_from = 0;
    while let Some(tag_start) = find_open_tag(xml, "img", search_from) {
        let tag_end = match xml[tag_start..].find('>') {
            Some(pos) => tag_start + pos,
            None => break,
        };
        let tag = &xml[tag_start..=tag_end];

        if let Some(src) = extract_attribute(tag, "src") {
            blocks.push(Block::Image {
                src: ResourcePath::resolve(base, src),
                alt: extract_attribute(tag, "alt")
                    .unwrap_or_default()
                    .to_string(),
            });
        }

        search_from = tag_end + 1;
    }

    blocks
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

pub fn parse_inline(xml: &str, document: &ResourcePath) -> Vec<Inline> {
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
                    Some((tag, open_tag, content_start)) => {
                        let close_tag = format!("</{tag}>");
                        match xml[content_start..].find(&close_tag) {
                            Some(close_rel) => {
                                if !text_buf.is_empty() {
                                    inlines.push(Inline::Text(core::mem::take(&mut text_buf)));
                                }
                                let content_end = content_start + close_rel;
                                let inner =
                                    parse_inline(&xml[content_start..content_end], document);
                                let built = match tag {
                                    "em" => NonEmpty::new(inner)
                                        .map(Inline::Emphasis)
                                        .into_iter()
                                        .collect(),
                                    "strong" => NonEmpty::new(inner)
                                        .map(Inline::Strong)
                                        .into_iter()
                                        .collect(),
                                    "a" => build_anchor_element(open_tag, inner, document),
                                    _ => unreachable!(),
                                };
                                inlines.extend(built);
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

fn match_recognized_open_tag(xml: &str, lt: usize) -> Option<(&'static str, &str, usize)> {
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
            let content_start = lt + 1 + tag_end_rel + 1;
            return Some((tag, &rest[..tag_end_rel], content_start));
        }
    }
    None
}

fn build_anchor_element(
    open_tag: &str,
    content: Vec<Inline>,
    document: &ResourcePath,
) -> Vec<Inline> {
    if let Some(href) = extract_attribute(open_tag, "href") {
        return vec![Inline::Link {
            target: resolve_link_target(href, document),
            content,
        }];
    }
    match extract_attribute(open_tag, "id") {
        Some(id) => {
            let mut result = vec![Inline::Anchor(Anchor::new(document, Some(id)))];
            result.extend(content);
            result
        }
        None => content,
    }
}

fn resolve_link_target(href: &str, document: &ResourcePath) -> LinkTarget {
    if let Some(fragment) = href.strip_prefix('#') {
        return LinkTarget::Internal(Anchor::new(document, Some(fragment)));
    }
    if has_uri_scheme(href) {
        return LinkTarget::External(href.to_string());
    }
    let (path, fragment) = match href.split_once('#') {
        Some((path, fragment)) => (path, Some(fragment)),
        None => (href, None),
    };
    LinkTarget::Internal(Anchor::new(
        &ResourcePath::resolve(document.parent(), path),
        fragment,
    ))
}

fn has_uri_scheme(href: &str) -> bool {
    let Some(colon) = href.find(':') else {
        return false;
    };
    let scheme = &href[..colon];
    !scheme.is_empty()
        && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
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

fn build_heading(level: u8, content: Vec<Inline>) -> Option<Block> {
    Some(Block::Heading {
        level: HeadingLevel::new(level)?,
        content: NonEmpty::new(content)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> ResourcePath {
        ResourcePath::resolve("", "doc.xhtml")
    }

    fn link_target(href: &str) -> LinkTarget {
        resolve_link_target(href, &doc())
    }

    fn table(headers: Vec<Vec<Inline>>, rows: Vec<Vec<Vec<Inline>>>) -> Table {
        Table::new(
            if headers.is_empty() {
                None
            } else {
                Some(headers.into_iter().map(Cell::new).collect())
            },
            rows.into_iter()
                .map(|row| row.into_iter().map(Cell::new).collect())
                .collect(),
        )
        .unwrap()
    }

    mod parse_blocks {
        use super::*;

        #[test]
        fn when_mixed_blocks_then_returns_document_order() {
            let xhtml = b"<h1>Title</h1><p>Intro.</p><h2>Sub</h2><p>Body.</p>";

            let blocks = parse_blocks(xhtml, &doc());

            assert_eq!(
                blocks,
                vec![
                    Block::Heading {
                        level: HeadingLevel::new(1).unwrap(),
                        content: NonEmpty::new(vec![Inline::Text("Title".to_string())]).unwrap(),
                    },
                    Block::Paragraph {
                        content: NonEmpty::new(vec![Inline::Text("Intro.".to_string())]).unwrap(),
                    },
                    Block::Heading {
                        level: HeadingLevel::new(2).unwrap(),
                        content: NonEmpty::new(vec![Inline::Text("Sub".to_string())]).unwrap(),
                    },
                    Block::Paragraph {
                        content: NonEmpty::new(vec![Inline::Text("Body.".to_string())]).unwrap(),
                    },
                ]
            );
        }

        #[test]
        fn when_list_follows_paragraph_then_keeps_order() {
            let xhtml = b"<p>Before</p><ul><li>A</li></ul><p>After</p>";

            let blocks = parse_blocks(xhtml, &doc());

            assert_eq!(
                blocks,
                vec![
                    Block::Paragraph {
                        content: NonEmpty::new(vec![Inline::Text("Before".to_string())]).unwrap(),
                    },
                    Block::List {
                        kind: ListKind::Unordered,
                        items: NonEmpty::new(vec![
                            ListItem::new(vec![Inline::Text("A".to_string())]).unwrap()
                        ])
                        .unwrap(),
                    },
                    Block::Paragraph {
                        content: NonEmpty::new(vec![Inline::Text("After".to_string())]).unwrap(),
                    },
                ]
            );
        }

        #[test]
        fn when_paragraph_inside_list_item_then_not_emitted_separately() {
            let xhtml = b"<ul><li><p>Nested</p></li></ul>";

            let blocks = parse_blocks(xhtml, &doc());

            assert_eq!(
                blocks,
                vec![Block::List {
                    kind: ListKind::Unordered,
                    items: NonEmpty::new(vec![
                        ListItem::new(vec![Inline::Text("Nested".to_string())]).unwrap()
                    ])
                    .unwrap(),
                }]
            );
        }

        #[test]
        fn when_image_wrapped_in_paragraph_then_emits_image_block() {
            let xhtml = br#"<p><img src="figure.png" alt="Figure"/></p>"#;

            let blocks = parse_blocks(xhtml, &doc());

            assert_eq!(
                blocks,
                vec![Block::Image {
                    src: ResourcePath::resolve("", "figure.png"),
                    alt: "Figure".to_string(),
                }]
            );
        }

        #[test]
        fn when_paragraph_has_text_and_image_then_emits_both() {
            let xhtml = br#"<p>Caption<img src="a.png" alt="A"/></p>"#;

            let blocks = parse_blocks(xhtml, &doc());

            assert_eq!(
                blocks,
                vec![
                    Block::Paragraph {
                        content: NonEmpty::new(vec![Inline::Text("Caption".to_string())]).unwrap(),
                    },
                    Block::Image {
                        src: ResourcePath::resolve("", "a.png"),
                        alt: "A".to_string(),
                    },
                ]
            );
        }

        #[test]
        fn when_empty_paragraph_then_skipped() {
            let xhtml = b"<p></p><p>Real</p>";

            let blocks = parse_blocks(xhtml, &doc());

            assert_eq!(
                blocks,
                vec![Block::Paragraph {
                    content: NonEmpty::new(vec![Inline::Text("Real".to_string())]).unwrap(),
                }]
            );
        }

        #[test]
        fn when_table_between_paragraphs_then_keeps_order() {
            let xhtml = b"<p>Before</p><table><tr><th>H</th></tr></table><p>After</p>";

            let blocks = parse_blocks(xhtml, &doc());

            assert_eq!(
                blocks,
                vec![
                    Block::Paragraph {
                        content: NonEmpty::new(vec![Inline::Text("Before".to_string())]).unwrap(),
                    },
                    Block::Table(table(vec![vec![Inline::Text("H".to_string())]], vec![])),
                    Block::Paragraph {
                        content: NonEmpty::new(vec![Inline::Text("After".to_string())]).unwrap(),
                    },
                ]
            );
        }

        #[test]
        fn when_ordered_and_unordered_lists_then_distinguishes_them() {
            let xhtml = b"<ol><li>A</li></ol><ul><li>B</li></ul>";

            let blocks = parse_blocks(xhtml, &doc());

            assert_eq!(
                blocks,
                vec![
                    Block::List {
                        kind: ListKind::Ordered,
                        items: NonEmpty::new(vec![
                            ListItem::new(vec![Inline::Text("A".to_string())]).unwrap()
                        ])
                        .unwrap(),
                    },
                    Block::List {
                        kind: ListKind::Unordered,
                        items: NonEmpty::new(vec![
                            ListItem::new(vec![Inline::Text("B".to_string())]).unwrap()
                        ])
                        .unwrap(),
                    },
                ]
            );
        }

        #[test]
        fn when_inline_markup_present_then_preserved_in_blocks() {
            let xhtml = br#"<p>See <a href="x.xhtml">here</a></p>"#;

            let blocks = parse_blocks(xhtml, &doc());

            assert_eq!(
                blocks,
                vec![Block::Paragraph {
                    content: NonEmpty::new(vec![
                        Inline::Text("See ".to_string()),
                        Inline::Link {
                            target: link_target("x.xhtml"),
                            content: vec![Inline::Text("here".to_string())],
                        },
                    ])
                    .unwrap(),
                }]
            );
        }

        #[test]
        fn when_unclosed_paragraph_then_does_not_loop_forever() {
            let xhtml = b"<p>Unclosed<h1>Title</h1>";

            let blocks = parse_blocks(xhtml, &doc());

            assert_eq!(
                blocks,
                vec![Block::Heading {
                    level: HeadingLevel::new(1).unwrap(),
                    content: NonEmpty::new(vec![Inline::Text("Title".to_string())]).unwrap(),
                }]
            );
        }

        #[test]
        fn when_no_blocks_then_returns_empty() {
            assert_eq!(parse_blocks(b"<html><body></body></html>", &doc()), vec![]);
        }

        #[test]
        fn when_invalid_utf8_then_returns_empty() {
            assert_eq!(parse_blocks(b"\xff\xfe<h1>Bad</h1>", &doc()), vec![]);
        }
    }

    mod parse_headings {
        use super::*;

        #[test]
        fn when_single_h1_then_returns_heading_block() {
            let xhtml = b"<h1>Chapter One</h1>";

            let blocks = parse_headings(xhtml, &doc());

            assert_eq!(
                blocks,
                vec![Block::Heading {
                    level: HeadingLevel::new(1).unwrap(),
                    content: NonEmpty::new(vec![Inline::Text("Chapter One".to_string())]).unwrap(),
                }]
            );
        }

        #[test]
        fn when_multiple_headings_then_returns_in_document_order() {
            let xhtml = b"<h1>Title</h1><p>ignored</p><h2>Subtitle</h2>";

            let blocks = parse_headings(xhtml, &doc());

            assert_eq!(
                blocks,
                vec![
                    Block::Heading {
                        level: HeadingLevel::new(1).unwrap(),
                        content: NonEmpty::new(vec![Inline::Text("Title".to_string())]).unwrap(),
                    },
                    Block::Heading {
                        level: HeadingLevel::new(2).unwrap(),
                        content: NonEmpty::new(vec![Inline::Text("Subtitle".to_string())]).unwrap(),
                    },
                ]
            );
        }

        #[test]
        fn when_heading_has_attributes_then_still_parsed() {
            let xhtml = br#"<h2 class="chapter" id="ch1">Section</h2>"#;

            let blocks = parse_headings(xhtml, &doc());

            assert_eq!(
                blocks,
                vec![Block::Heading {
                    level: HeadingLevel::new(2).unwrap(),
                    content: NonEmpty::new(vec![Inline::Text("Section".to_string())]).unwrap(),
                }]
            );
        }

        #[test]
        fn when_heading_has_unrecognized_nested_tags_then_strips_them_from_text() {
            let xhtml = b"<h1>Hello <span>World</span></h1>";

            let blocks = parse_headings(xhtml, &doc());

            assert_eq!(
                blocks,
                vec![Block::Heading {
                    level: HeadingLevel::new(1).unwrap(),
                    content: NonEmpty::new(vec![Inline::Text("Hello World".to_string())]).unwrap(),
                }]
            );
        }

        #[test]
        fn when_heading_has_emphasis_then_returns_structured_inline() {
            let xhtml = b"<h1>Hello <em>World</em></h1>";

            let blocks = parse_headings(xhtml, &doc());

            assert_eq!(
                blocks,
                vec![Block::Heading {
                    level: HeadingLevel::new(1).unwrap(),
                    content: NonEmpty::new(vec![
                        Inline::Text("Hello ".to_string()),
                        Inline::Emphasis(
                            NonEmpty::new(vec![Inline::Text("World".to_string())]).unwrap()
                        ),
                    ])
                    .unwrap(),
                }]
            );
        }

        #[test]
        fn when_heading_has_surrounding_whitespace_then_trims_it() {
            let xhtml = b"<h3>\n  Padded  \n</h3>";

            let blocks = parse_headings(xhtml, &doc());

            assert_eq!(
                blocks,
                vec![Block::Heading {
                    level: HeadingLevel::new(3).unwrap(),
                    content: NonEmpty::new(vec![Inline::Text("Padded".to_string())]).unwrap(),
                }]
            );
        }

        #[test]
        fn when_no_headings_then_returns_empty() {
            let xhtml = b"<p>No headings here</p>";

            assert_eq!(parse_headings(xhtml, &doc()), vec![]);
        }

        #[test]
        fn when_similarly_named_tag_then_not_mistaken_for_heading() {
            let xhtml = b"<header>Not a heading</header>";

            assert_eq!(parse_headings(xhtml, &doc()), vec![]);
        }

        #[test]
        fn when_invalid_utf8_then_returns_empty() {
            let xhtml = b"\xff\xfe<h1>Bad</h1>";

            assert_eq!(parse_headings(xhtml, &doc()), vec![]);
        }

        #[test]
        fn when_all_six_heading_levels_present_then_parses_each() {
            let xhtml = b"<h1>A</h1><h2>B</h2><h3>C</h3><h4>D</h4><h5>E</h5><h6>F</h6>";

            let blocks = parse_headings(xhtml, &doc());

            assert_eq!(
                blocks,
                vec![
                    Block::Heading {
                        level: HeadingLevel::new(1).unwrap(),
                        content: NonEmpty::new(vec![Inline::Text("A".to_string())]).unwrap()
                    },
                    Block::Heading {
                        level: HeadingLevel::new(2).unwrap(),
                        content: NonEmpty::new(vec![Inline::Text("B".to_string())]).unwrap()
                    },
                    Block::Heading {
                        level: HeadingLevel::new(3).unwrap(),
                        content: NonEmpty::new(vec![Inline::Text("C".to_string())]).unwrap()
                    },
                    Block::Heading {
                        level: HeadingLevel::new(4).unwrap(),
                        content: NonEmpty::new(vec![Inline::Text("D".to_string())]).unwrap()
                    },
                    Block::Heading {
                        level: HeadingLevel::new(5).unwrap(),
                        content: NonEmpty::new(vec![Inline::Text("E".to_string())]).unwrap()
                    },
                    Block::Heading {
                        level: HeadingLevel::new(6).unwrap(),
                        content: NonEmpty::new(vec![Inline::Text("F".to_string())]).unwrap()
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

            let blocks = parse_paragraphs(xhtml, &doc());

            assert_eq!(
                blocks,
                vec![Block::Paragraph {
                    content: NonEmpty::new(vec![Inline::Text("Hello world.".to_string())]).unwrap(),
                }]
            );
        }

        #[test]
        fn when_multiple_paragraphs_then_returns_in_document_order() {
            let xhtml = b"<h1>Title</h1><p>First.</p><p>Second.</p>";

            let blocks = parse_paragraphs(xhtml, &doc());

            assert_eq!(
                blocks,
                vec![
                    Block::Paragraph {
                        content: NonEmpty::new(vec![Inline::Text("First.".to_string())]).unwrap(),
                    },
                    Block::Paragraph {
                        content: NonEmpty::new(vec![Inline::Text("Second.".to_string())]).unwrap(),
                    },
                ]
            );
        }

        #[test]
        fn when_paragraph_has_attributes_then_still_parsed() {
            let xhtml = br#"<p class="intro">Welcome.</p>"#;

            let blocks = parse_paragraphs(xhtml, &doc());

            assert_eq!(
                blocks,
                vec![Block::Paragraph {
                    content: NonEmpty::new(vec![Inline::Text("Welcome.".to_string())]).unwrap(),
                }]
            );
        }

        #[test]
        fn when_paragraph_has_unrecognized_nested_tags_then_strips_them_from_text() {
            let xhtml = b"<p>Hello <span>World</span>.</p>";

            let blocks = parse_paragraphs(xhtml, &doc());

            assert_eq!(
                blocks,
                vec![Block::Paragraph {
                    content: NonEmpty::new(vec![Inline::Text("Hello World.".to_string())]).unwrap(),
                }]
            );
        }

        #[test]
        fn when_paragraph_has_link_and_strong_then_returns_structured_inline() {
            let xhtml = br#"<p>Visit <a href="https://example.com">here</a> for <strong>details</strong>.</p>"#;

            let blocks = parse_paragraphs(xhtml, &doc());

            assert_eq!(
                blocks,
                vec![Block::Paragraph {
                    content: NonEmpty::new(vec![
                        Inline::Text("Visit ".to_string()),
                        Inline::Link {
                            target: link_target("https://example.com"),
                            content: vec![Inline::Text("here".to_string())],
                        },
                        Inline::Text(" for ".to_string()),
                        Inline::Strong(
                            NonEmpty::new(vec![Inline::Text("details".to_string())]).unwrap()
                        ),
                        Inline::Text(".".to_string()),
                    ])
                    .unwrap(),
                }]
            );
        }

        #[test]
        fn when_no_paragraphs_then_returns_empty() {
            let xhtml = b"<h1>No paragraphs here</h1>";

            assert_eq!(parse_paragraphs(xhtml, &doc()), vec![]);
        }

        #[test]
        fn when_invalid_utf8_then_returns_empty() {
            let xhtml = b"\xff\xfe<p>Bad</p>";

            assert_eq!(parse_paragraphs(xhtml, &doc()), vec![]);
        }
    }

    mod parse_lists {
        use super::*;

        #[test]
        fn when_unordered_list_then_returns_unordered_list_block() {
            let xhtml = b"<ul><li>First</li><li>Second</li></ul>";

            let blocks = parse_lists(xhtml, &doc());

            assert_eq!(
                blocks,
                vec![Block::List {
                    kind: ListKind::Unordered,
                    items: NonEmpty::new(vec![
                        ListItem::new(vec![Inline::Text("First".to_string())]).unwrap(),
                        ListItem::new(vec![Inline::Text("Second".to_string())]).unwrap(),
                    ])
                    .unwrap(),
                }]
            );
        }

        #[test]
        fn when_ordered_list_then_returns_ordered_list_block() {
            let xhtml = b"<ol><li>First</li><li>Second</li></ol>";

            let blocks = parse_lists(xhtml, &doc());

            assert_eq!(
                blocks,
                vec![Block::List {
                    kind: ListKind::Ordered,
                    items: NonEmpty::new(vec![
                        ListItem::new(vec![Inline::Text("First".to_string())]).unwrap(),
                        ListItem::new(vec![Inline::Text("Second".to_string())]).unwrap(),
                    ])
                    .unwrap(),
                }]
            );
        }

        #[test]
        fn when_multiple_lists_then_returns_in_document_order() {
            let xhtml = b"<ul><li>A</li></ul><p>between</p><ol><li>B</li></ol>";

            let blocks = parse_lists(xhtml, &doc());

            assert_eq!(
                blocks,
                vec![
                    Block::List {
                        kind: ListKind::Unordered,
                        items: NonEmpty::new(vec![
                            ListItem::new(vec![Inline::Text("A".to_string())]).unwrap()
                        ])
                        .unwrap(),
                    },
                    Block::List {
                        kind: ListKind::Ordered,
                        items: NonEmpty::new(vec![
                            ListItem::new(vec![Inline::Text("B".to_string())]).unwrap()
                        ])
                        .unwrap(),
                    },
                ]
            );
        }

        #[test]
        fn when_list_item_has_unrecognized_nested_tags_then_strips_them_from_text() {
            let xhtml = b"<ul><li>Hello <span>World</span></li></ul>";

            let blocks = parse_lists(xhtml, &doc());

            assert_eq!(
                blocks,
                vec![Block::List {
                    kind: ListKind::Unordered,
                    items: NonEmpty::new(vec![
                        ListItem::new(vec![Inline::Text("Hello World".to_string())]).unwrap()
                    ])
                    .unwrap(),
                }]
            );
        }

        #[test]
        fn when_list_item_has_link_then_returns_structured_inline() {
            let xhtml = br#"<ul><li>See <a href="chapter2.xhtml">Chapter 2</a></li></ul>"#;

            let blocks = parse_lists(xhtml, &doc());

            assert_eq!(
                blocks,
                vec![Block::List {
                    kind: ListKind::Unordered,
                    items: NonEmpty::new(vec![
                        ListItem::new(vec![
                            Inline::Text("See ".to_string()),
                            Inline::Link {
                                target: link_target("chapter2.xhtml"),
                                content: vec![Inline::Text("Chapter 2".to_string())],
                            },
                        ])
                        .unwrap()
                    ])
                    .unwrap(),
                }]
            );
        }

        #[test]
        fn when_list_has_attributes_then_still_parsed() {
            let xhtml = br#"<ul class="toc"><li>Entry</li></ul>"#;

            let blocks = parse_lists(xhtml, &doc());

            assert_eq!(
                blocks,
                vec![Block::List {
                    kind: ListKind::Unordered,
                    items: NonEmpty::new(vec![
                        ListItem::new(vec![Inline::Text("Entry".to_string())]).unwrap()
                    ])
                    .unwrap(),
                }]
            );
        }

        #[test]
        fn when_list_has_no_items_then_returns_empty() {
            let xhtml = b"<ul></ul>";

            assert_eq!(parse_lists(xhtml, &doc()), vec![]);
        }

        #[test]
        fn when_no_lists_then_returns_empty() {
            let xhtml = b"<p>No lists here</p>";

            assert_eq!(parse_lists(xhtml, &doc()), vec![]);
        }

        #[test]
        fn when_invalid_utf8_then_returns_empty() {
            let xhtml = b"\xff\xfe<ul><li>Bad</li></ul>";

            assert_eq!(parse_lists(xhtml, &doc()), vec![]);
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

            let blocks = parse_tables(xhtml, &doc());

            assert_eq!(
                blocks,
                vec![Block::Table(table(
                    vec![
                        vec![Inline::Text("Name".to_string())],
                        vec![Inline::Text("Age".to_string())],
                    ],
                    vec![
                        vec![
                            vec![Inline::Text("Alice".to_string())],
                            vec![Inline::Text("30".to_string())],
                        ],
                        vec![
                            vec![Inline::Text("Bob".to_string())],
                            vec![Inline::Text("25".to_string())],
                        ],
                    ]
                ))]
            );
        }

        #[test]
        fn when_table_wrapped_in_thead_and_tbody_then_still_parsed() {
            let xhtml = b"<table>
  <thead><tr><th>Key</th></tr></thead>
  <tbody><tr><td>Value</td></tr></tbody>
</table>";

            let blocks = parse_tables(xhtml, &doc());

            assert_eq!(
                blocks,
                vec![Block::Table(table(
                    vec![vec![Inline::Text("Key".to_string())]],
                    vec![vec![vec![Inline::Text("Value".to_string())]]]
                ))]
            );
        }

        #[test]
        fn when_table_has_no_header_row_then_returns_empty_headers() {
            let xhtml = b"<table><tr><td>A</td><td>B</td></tr></table>";

            let blocks = parse_tables(xhtml, &doc());

            assert_eq!(
                blocks,
                vec![Block::Table(table(
                    vec![],
                    vec![vec![
                        vec![Inline::Text("A".to_string())],
                        vec![Inline::Text("B".to_string())],
                    ]]
                ))]
            );
        }

        #[test]
        fn when_cell_has_inline_markup_then_returns_structured_inline() {
            let xhtml = br#"<table><tr><td>See <a href="x.xhtml">link</a></td></tr></table>"#;

            let blocks = parse_tables(xhtml, &doc());

            assert_eq!(
                blocks,
                vec![Block::Table(table(
                    vec![],
                    vec![vec![vec![
                        Inline::Text("See ".to_string()),
                        Inline::Link {
                            target: link_target("x.xhtml"),
                            content: vec![Inline::Text("link".to_string())],
                        },
                    ]]]
                ))]
            );
        }

        #[test]
        fn when_table_has_attributes_then_still_parsed() {
            let xhtml = br#"<table class="data"><tr><td>Cell</td></tr></table>"#;

            let blocks = parse_tables(xhtml, &doc());

            assert_eq!(
                blocks,
                vec![Block::Table(table(
                    vec![],
                    vec![vec![vec![Inline::Text("Cell".to_string())]]]
                ))]
            );
        }

        #[test]
        fn when_multiple_tables_then_returns_in_document_order() {
            let xhtml =
                b"<table><tr><td>A</td></tr></table><p>gap</p><table><tr><td>B</td></tr></table>";

            let blocks = parse_tables(xhtml, &doc());

            assert_eq!(
                blocks,
                vec![
                    Block::Table(table(
                        vec![],
                        vec![vec![vec![Inline::Text("A".to_string())]]]
                    )),
                    Block::Table(table(
                        vec![],
                        vec![vec![vec![Inline::Text("B".to_string())]]]
                    )),
                ]
            );
        }

        #[test]
        fn when_header_row_appears_after_data_row_then_treated_as_data_row() {
            let xhtml = b"<table><tr><td>A</td></tr><tr><th>H</th></tr></table>";

            let blocks = parse_tables(xhtml, &doc());

            assert_eq!(
                blocks,
                vec![Block::Table(table(
                    vec![],
                    vec![
                        vec![vec![Inline::Text("A".to_string())]],
                        vec![vec![Inline::Text("H".to_string())]],
                    ]
                ))]
            );
        }

        #[test]
        fn when_table_is_empty_then_returns_empty() {
            let xhtml = b"<table></table>";

            assert_eq!(parse_tables(xhtml, &doc()), vec![]);
        }

        #[test]
        fn when_no_tables_then_returns_empty() {
            let xhtml = b"<p>No tables here</p>";

            assert_eq!(parse_tables(xhtml, &doc()), vec![]);
        }

        #[test]
        fn when_invalid_utf8_then_returns_empty() {
            let xhtml = b"\xff\xfe<table><tr><td>Bad</td></tr></table>";

            assert_eq!(parse_tables(xhtml, &doc()), vec![]);
        }
    }

    mod parse_images {
        use super::*;

        #[test]
        fn when_self_closing_img_then_returns_image_block() {
            let xhtml = br#"<img src="cover.jpg" alt="Cover"/>"#;

            let blocks = parse_images(xhtml, "");

            assert_eq!(
                blocks,
                vec![Block::Image {
                    src: ResourcePath::resolve("", "cover.jpg"),
                    alt: "Cover".to_string(),
                }]
            );
        }

        #[test]
        fn when_img_without_alt_then_returns_empty_alt() {
            let xhtml = br#"<img src="figure1.png"/>"#;

            let blocks = parse_images(xhtml, "");

            assert_eq!(
                blocks,
                vec![Block::Image {
                    src: ResourcePath::resolve("", "figure1.png"),
                    alt: String::new(),
                }]
            );
        }

        #[test]
        fn when_img_without_src_then_skipped() {
            let xhtml = br#"<img alt="orphan"/>"#;

            assert_eq!(parse_images(xhtml, ""), vec![]);
        }

        #[test]
        fn when_attributes_in_reversed_order_then_still_parsed() {
            let xhtml = br#"<img alt="Cover" src="cover.jpg"/>"#;

            let blocks = parse_images(xhtml, "");

            assert_eq!(
                blocks,
                vec![Block::Image {
                    src: ResourcePath::resolve("", "cover.jpg"),
                    alt: "Cover".to_string(),
                }]
            );
        }

        #[test]
        fn when_nested_path_then_returns_full_path() {
            let xhtml = br#"<img src="Images/ch1/figure1.png" alt="Figure 1"/>"#;

            let blocks = parse_images(xhtml, "");

            assert_eq!(
                blocks,
                vec![Block::Image {
                    src: ResourcePath::resolve("", "Images/ch1/figure1.png"),
                    alt: "Figure 1".to_string(),
                }]
            );
        }

        #[test]
        fn when_multiple_images_then_returns_in_document_order() {
            let xhtml =
                br#"<p>text</p><img src="a.png" alt="A"/><p>gap</p><img src="b.png" alt="B"/>"#;

            let blocks = parse_images(xhtml, "");

            assert_eq!(
                blocks,
                vec![
                    Block::Image {
                        src: ResourcePath::resolve("", "a.png"),
                        alt: "A".to_string(),
                    },
                    Block::Image {
                        src: ResourcePath::resolve("", "b.png"),
                        alt: "B".to_string(),
                    },
                ]
            );
        }

        #[test]
        fn when_single_quoted_attributes_then_returns_values() {
            let xhtml = b"<img src='cover.jpg' alt='Cover'/>";

            let blocks = parse_images(xhtml, "");

            assert_eq!(
                blocks,
                vec![Block::Image {
                    src: ResourcePath::resolve("", "cover.jpg"),
                    alt: "Cover".to_string(),
                }]
            );
        }

        #[test]
        fn when_similarly_named_attribute_then_not_mistaken_for_src() {
            let xhtml = br#"<img data-src="wrong.png" src="right.png"/>"#;

            let blocks = parse_images(xhtml, "");

            assert_eq!(
                blocks,
                vec![Block::Image {
                    src: ResourcePath::resolve("", "right.png"),
                    alt: String::new(),
                }]
            );
        }

        #[test]
        fn when_no_images_then_returns_empty() {
            let xhtml = b"<p>No images here</p>";

            assert_eq!(parse_images(xhtml, ""), vec![]);
        }

        #[test]
        fn when_invalid_utf8_then_returns_empty() {
            let xhtml = b"\xff\xfe<img src=\"bad.png\"/>";

            assert_eq!(parse_images(xhtml, ""), vec![]);
        }
    }

    mod parse_inline {
        use super::*;

        #[test]
        fn when_plain_text_then_returns_single_text_inline() {
            let inlines = parse_inline("Hello world", &doc());

            assert_eq!(inlines, vec![Inline::Text("Hello world".to_string())]);
        }

        #[test]
        fn when_link_then_returns_link_inline_with_href() {
            let inlines = parse_inline(r#"<a href="https://example.com">here</a>"#, &doc());

            assert_eq!(
                inlines,
                vec![Inline::Link {
                    target: link_target("https://example.com"),
                    content: vec![Inline::Text("here".to_string())],
                }]
            );
        }

        #[test]
        fn when_anchor_element_has_neither_href_nor_id_then_keeps_content() {
            let inlines = parse_inline("<a>here</a>", &doc());

            assert_eq!(inlines, vec![Inline::Text("here".to_string())]);
        }

        #[test]
        fn when_emphasis_then_returns_emphasis_inline() {
            let inlines = parse_inline("<em>World</em>", &doc());

            assert_eq!(
                inlines,
                vec![Inline::Emphasis(
                    NonEmpty::new(vec![Inline::Text("World".to_string())]).unwrap()
                )]
            );
        }

        #[test]
        fn when_strong_then_returns_strong_inline() {
            let inlines = parse_inline("<strong>World</strong>", &doc());

            assert_eq!(
                inlines,
                vec![Inline::Strong(
                    NonEmpty::new(vec![Inline::Text("World".to_string())]).unwrap()
                )]
            );
        }

        #[test]
        fn when_strong_nested_in_emphasis_then_returns_nested_inline() {
            let inlines = parse_inline("<em>very <strong>important</strong></em>", &doc());

            assert_eq!(
                inlines,
                vec![Inline::Emphasis(
                    NonEmpty::new(vec![
                        Inline::Text("very ".to_string()),
                        Inline::Strong(
                            NonEmpty::new(vec![Inline::Text("important".to_string())]).unwrap()
                        ),
                    ])
                    .unwrap()
                )]
            );
        }

        #[test]
        fn when_emphasis_nested_in_link_then_returns_nested_inline() {
            let inlines = parse_inline(r#"<a href="x.xhtml"><em>Chapter</em></a>"#, &doc());

            assert_eq!(
                inlines,
                vec![Inline::Link {
                    target: link_target("x.xhtml"),
                    content: vec![Inline::Emphasis(
                        NonEmpty::new(vec![Inline::Text("Chapter".to_string())]).unwrap()
                    )],
                }]
            );
        }

        #[test]
        fn when_unrecognized_tag_then_strips_tag_but_keeps_text() {
            let inlines = parse_inline("Hello <span>World</span>", &doc());

            assert_eq!(inlines, vec![Inline::Text("Hello World".to_string())]);
        }

        #[test]
        fn when_multiple_inlines_in_sequence_then_returns_all_in_order() {
            let inlines = parse_inline("A <em>B</em> C <strong>D</strong> E", &doc());

            assert_eq!(
                inlines,
                vec![
                    Inline::Text("A ".to_string()),
                    Inline::Emphasis(NonEmpty::new(vec![Inline::Text("B".to_string())]).unwrap()),
                    Inline::Text(" C ".to_string()),
                    Inline::Strong(NonEmpty::new(vec![Inline::Text("D".to_string())]).unwrap()),
                    Inline::Text(" E".to_string()),
                ]
            );
        }

        #[test]
        fn when_empty_string_then_returns_empty() {
            assert_eq!(parse_inline("", &doc()), vec![]);
        }

        #[test]
        fn when_unclosed_recognized_tag_then_ignored() {
            let inlines = parse_inline("Hello <em>World", &doc());

            assert_eq!(inlines, vec![Inline::Text("Hello World".to_string())]);
        }

        #[test]
        fn when_surrounding_whitespace_then_trims_outer_edges_only() {
            let inlines = parse_inline("\n  Hello <em>World</em>  \n", &doc());

            assert_eq!(
                inlines,
                vec![
                    Inline::Text("Hello ".to_string()),
                    Inline::Emphasis(
                        NonEmpty::new(vec![Inline::Text("World".to_string())]).unwrap()
                    ),
                ]
            );
        }

        #[test]
        fn when_trailing_whitespace_becomes_empty_after_trim_then_removed() {
            let inlines = parse_inline("<em>World</em>  \n", &doc());

            assert_eq!(
                inlines,
                vec![Inline::Emphasis(
                    NonEmpty::new(vec![Inline::Text("World".to_string())]).unwrap()
                )]
            );
        }
    }
}
