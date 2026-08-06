#[derive(Debug, Clone, PartialEq)]
pub struct NonEmpty<T>(Vec<T>);

impl<T> NonEmpty<T> {
    pub fn new(items: Vec<T>) -> Option<Self> {
        if items.is_empty() {
            None
        } else {
            Some(Self(items))
        }
    }
}

impl<T> core::ops::Deref for NonEmpty<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeadingLevel(u8);

impl HeadingLevel {
    pub const MIN: u8 = 1;
    pub const MAX: u8 = 6;

    pub fn new(level: u8) -> Option<Self> {
        (Self::MIN..=Self::MAX)
            .contains(&level)
            .then_some(Self(level))
    }

    pub fn get(self) -> u8 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListKind {
    Ordered,
    Unordered,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourcePath(String);

impl ResourcePath {
    pub fn resolve(base: &str, href: &str) -> Self {
        let mut segments: Vec<&str> = Vec::new();
        for segment in base.split('/').chain(href.split('/')) {
            match segment {
                "" | "." => {}
                ".." => {
                    segments.pop();
                }
                name => segments.push(name),
            }
        }
        Self(segments.join("/"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn parent(&self) -> &str {
        match self.0.rfind('/') {
            Some(index) => &self.0[..index],
            None => "",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cell(Vec<Inline>);

impl Cell {
    pub fn new(content: Vec<Inline>) -> Self {
        Self(content)
    }

    pub fn content(&self) -> &[Inline] {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    columns: core::num::NonZeroUsize,
    has_headers: bool,
    cells: Vec<Cell>,
}

impl Table {
    pub fn new(headers: Option<Vec<Cell>>, rows: Vec<Vec<Cell>>) -> Option<Self> {
        let columns = core::num::NonZeroUsize::new(
            headers
                .as_ref()
                .map_or(0, Vec::len)
                .max(rows.iter().map(Vec::len).max().unwrap_or(0)),
        )?;

        let has_headers = headers.is_some();
        let cells = headers
            .into_iter()
            .chain(rows)
            .flat_map(|row| pad_row(row, columns))
            .collect();

        Some(Self {
            columns,
            has_headers,
            cells,
        })
    }

    pub fn columns(&self) -> core::num::NonZeroUsize {
        self.columns
    }

    pub fn headers(&self) -> Option<&[Cell]> {
        self.has_headers.then(|| self.records().next()).flatten()
    }

    pub fn rows(&self) -> impl Iterator<Item = &[Cell]> {
        self.records().skip(usize::from(self.has_headers))
    }

    fn records(&self) -> core::slice::ChunksExact<'_, Cell> {
        self.cells.chunks_exact(self.columns.get())
    }
}

fn pad_row(mut cells: Vec<Cell>, columns: core::num::NonZeroUsize) -> Vec<Cell> {
    cells.resize(columns.get(), Cell::new(Vec::new()));
    cells
}

#[derive(Debug, Clone, PartialEq)]
pub struct ListItem(NonEmpty<Inline>);

impl ListItem {
    pub fn new(content: Vec<Inline>) -> Option<Self> {
        NonEmpty::new(content).map(Self)
    }

    pub fn content(&self) -> &[Inline] {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Heading {
        level: HeadingLevel,
        content: NonEmpty<Inline>,
    },
    Paragraph {
        content: NonEmpty<Inline>,
    },
    List {
        kind: ListKind,
        items: NonEmpty<ListItem>,
    },
    Table(Table),
    Image {
        src: ResourcePath,
        alt: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Inline {
    Text(String),
    Emphasis(NonEmpty<Inline>),
    Strong(NonEmpty<Inline>),
    Link { href: String, content: Vec<Inline> },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(value: &str) -> Inline {
        Inline::Text(value.to_string())
    }

    mod non_empty {
        use super::*;

        #[test]
        fn when_items_are_present_then_constructs() {
            let items = NonEmpty::new(vec![text("a")]).unwrap();

            assert_eq!(&*items, &[text("a")]);
        }

        #[test]
        fn when_items_are_empty_then_returns_none() {
            assert_eq!(NonEmpty::<Inline>::new(vec![]), None);
        }
    }

    mod heading_level {
        use super::*;

        #[test]
        fn when_level_is_within_range_then_constructs() {
            assert_eq!(HeadingLevel::new(1).unwrap().get(), 1);
            assert_eq!(HeadingLevel::new(6).unwrap().get(), 6);
        }

        #[test]
        fn when_level_is_zero_then_returns_none() {
            assert_eq!(HeadingLevel::new(0), None);
        }

        #[test]
        fn when_level_is_above_six_then_returns_none() {
            assert_eq!(HeadingLevel::new(7), None);
        }
    }

    mod list_item {
        use super::*;

        #[test]
        fn when_content_is_present_then_constructs() {
            let item = ListItem::new(vec![text("a")]).unwrap();

            assert_eq!(item.content(), &[text("a")]);
        }

        #[test]
        fn when_content_is_empty_then_returns_none() {
            assert_eq!(ListItem::new(vec![]), None);
        }
    }

    mod table {
        use super::*;

        fn cell(value: &str) -> Cell {
            Cell::new(vec![text(value)])
        }

        #[test]
        fn when_headers_and_rows_given_then_columns_is_their_width() {
            let table = Table::new(
                Some(vec![cell("a"), cell("b")]),
                vec![vec![cell("1"), cell("2")]],
            )
            .unwrap();

            assert_eq!(table.columns().get(), 2);
            assert_eq!(table.headers().unwrap().len(), 2);
            assert_eq!(table.rows().next().unwrap().len(), 2);
        }

        #[test]
        fn when_row_is_short_then_padded_to_column_count() {
            let table = Table::new(
                Some(vec![cell("a"), cell("b"), cell("c")]),
                vec![vec![cell("1")]],
            )
            .unwrap();

            assert_eq!(table.rows().next().unwrap().len(), 3);
            assert_eq!(table.rows().next().unwrap()[2], Cell::new(vec![]));
        }

        #[test]
        fn when_row_is_wider_than_headers_then_headers_are_padded() {
            let table =
                Table::new(Some(vec![cell("a")]), vec![vec![cell("1"), cell("2")]]).unwrap();

            assert_eq!(table.columns().get(), 2);
            assert_eq!(table.headers().unwrap().len(), 2);
        }

        #[test]
        fn when_headers_absent_then_columns_come_from_rows() {
            let table = Table::new(None, vec![vec![cell("1"), cell("2")]]).unwrap();

            assert_eq!(table.columns().get(), 2);
            assert_eq!(table.headers(), None);
        }

        #[test]
        fn when_there_is_no_column_then_returns_none() {
            assert_eq!(Table::new(None, vec![]), None);
            assert_eq!(Table::new(Some(vec![]), vec![vec![]]), None);
        }
    }
}
