#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Heading {
        level: u8,
        content: Vec<Inline>,
    },
    Paragraph {
        content: Vec<Inline>,
    },
    List {
        ordered: bool,
        items: Vec<ListItem>,
    },
    Table {
        headers: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
    },
    Image {
        src: String,
        alt: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ListItem {
    pub content: Vec<Inline>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Inline {
    Text(String),
    Emphasis(Vec<Inline>),
    Strong(Vec<Inline>),
    Link { href: String, content: Vec<Inline> },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heading_with_plain_text() {
        let block = Block::Heading {
            level: 1,
            content: vec![Inline::Text("Hello".to_string())],
        };
        assert_eq!(
            block,
            Block::Heading {
                level: 1,
                content: vec![Inline::Text("Hello".to_string())],
            }
        );
    }

    #[test]
    fn paragraph_with_mixed_inlines() {
        let block = Block::Paragraph {
            content: vec![
                Inline::Text("Click ".to_string()),
                Inline::Link {
                    href: "https://example.com".to_string(),
                    content: vec![Inline::Strong(vec![Inline::Text("here".to_string())])],
                },
            ],
        };
        match &block {
            Block::Paragraph { content } => assert_eq!(content.len(), 2),
            _ => panic!("expected Paragraph"),
        }
    }

    #[test]
    fn unordered_list() {
        let block = Block::List {
            ordered: false,
            items: vec![
                ListItem {
                    content: vec![Inline::Text("first".to_string())],
                },
                ListItem {
                    content: vec![Inline::Text("second".to_string())],
                },
            ],
        };
        match &block {
            Block::List { ordered, items } => {
                assert!(!ordered);
                assert_eq!(items.len(), 2);
            }
            _ => panic!("expected List"),
        }
    }

    #[test]
    fn table_with_headers_and_rows() {
        let block = Block::Table {
            headers: vec![
                vec![Inline::Text("Name".to_string())],
                vec![Inline::Text("Age".to_string())],
            ],
            rows: vec![vec![
                vec![Inline::Text("Alice".to_string())],
                vec![Inline::Text("30".to_string())],
            ]],
        };
        match &block {
            Block::Table { headers, rows } => {
                assert_eq!(headers.len(), 2);
                assert_eq!(rows.len(), 1);
            }
            _ => panic!("expected Table"),
        }
    }
}
