use super::page::{Page, Span};
use super::script;
use crate::ir::{Block, HeadingLevel, Inline, NonEmpty, Text};

const SPACE_GAP: f64 = 0.15;
const LINE_TOLERANCE: f64 = 0.5;
const PARAGRAPH_GAP: f64 = 1.8;
const SIZE_TOLERANCE: f64 = 0.05;
const INDENT: f64 = 0.6;
const MARGIN_RATIO: f64 = 0.06;
const SIZE_BUCKET: f64 = 2.0;
const BODY_SHARE: f64 = 0.5;
const WIDE_ADVANCE: f64 = 1.0;
const NARROW_ADVANCE: f64 = 0.5;
const DEFAULT_BODY_SIZE: f64 = 1.0;
const HEADING_SIZES: [(f64, u8); 4] = [(1.8, 1), (1.3, 2), (1.15, 3), (1.08, 4)];

#[derive(Debug, Clone, PartialEq)]
struct Line {
    text: String,
    x: f64,
    y: f64,
    size: f64,
}

pub fn document_blocks(pages: &[Page]) -> Vec<Block> {
    let spans: Vec<Vec<&Span>> = pages.iter().map(body_spans).collect();
    let body = body_size(&spans.concat());

    spans.iter().flat_map(|spans| blocks(spans, body)).collect()
}

fn body_spans(page: &Page) -> Vec<&Span> {
    let [_, bottom, _, top] = page.media_box;
    let margin = (top - bottom) * MARGIN_RATIO;

    page.spans
        .iter()
        .filter(|span| span.y >= bottom + margin && span.y <= top - margin)
        .collect()
}

fn body_size(spans: &[&Span]) -> f64 {
    let mut buckets: Vec<(f64, usize)> = Vec::new();
    for span in spans {
        let size = (span.size * SIZE_BUCKET).round() / SIZE_BUCKET;
        let weight = span.text.chars().count();
        match buckets.iter_mut().find(|(bucket, _)| *bucket == size) {
            Some((_, total)) => *total += weight,
            None => buckets.push((size, weight)),
        }
    }

    let common = buckets
        .iter()
        .map(|(_, total)| *total)
        .max()
        .unwrap_or_default() as f64
        * BODY_SHARE;

    buckets
        .iter()
        .filter(|(size, total)| *size > 0.0 && *total as f64 >= common)
        .map(|(size, _)| *size)
        .fold(f64::NAN, f64::max)
        .max(DEFAULT_BODY_SIZE)
}

fn blocks(spans: &[&Span], body: f64) -> Vec<Block> {
    let lines = lines(spans);
    let mut blocks = Vec::new();
    let mut current: Vec<&Line> = Vec::new();
    let mut left = f64::INFINITY;

    for line in &lines {
        let broken = current.last().is_some_and(|previous| {
            previous.y - line.y > previous.size * PARAGRAPH_GAP
                || (line.size - previous.size).abs() > previous.size * SIZE_TOLERANCE
                || line.x - left > line.size * INDENT
        });
        if broken {
            blocks.extend(block(&current, body));
            current.clear();
            left = f64::INFINITY;
        }
        left = left.min(line.x);
        current.push(line);
    }
    blocks.extend(block(&current, body));

    blocks
}

fn lines(spans: &[&Span]) -> Vec<Line> {
    let mut ordered = spans.to_vec();
    ordered.sort_by(|left, right| right.y.total_cmp(&left.y).then(left.x.total_cmp(&right.x)));

    let mut lines: Vec<Vec<&Span>> = Vec::new();
    for span in ordered {
        let joins = lines
            .last()
            .and_then(|line| line.first())
            .is_some_and(|first| (first.y - span.y).abs() <= span.size * LINE_TOLERANCE);
        match lines.last_mut() {
            Some(line) if joins => line.push(span),
            _ => lines.push(vec![span]),
        }
    }

    lines.iter().filter_map(|line| self::line(line)).collect()
}

fn line(spans: &[&Span]) -> Option<Line> {
    let mut ordered = spans.to_vec();
    ordered.sort_by(|left, right| left.x.total_cmp(&right.x));

    let mut text = String::new();
    let mut end = f64::NEG_INFINITY;
    for span in &ordered {
        if !text.is_empty()
            && span.x - end > span.size * SPACE_GAP
            && script::separable(&text, &span.text)
        {
            text.push(' ');
        }
        text.push_str(&span.text);
        end = span.x + advance(&span.text, span.size);
    }

    Some(Line {
        text,
        x: ordered.first()?.x,
        y: ordered.first()?.y,
        size: ordered
            .iter()
            .map(|span| span.size)
            .fold(0.0, |largest, size| largest.max(size)),
    })
}

fn block(lines: &[&Line], body: f64) -> Option<Block> {
    let mut text = String::new();
    for line in lines {
        if !text.is_empty() && script::separable(&text, &line.text) {
            text.push(' ');
        }
        text.push_str(&line.text);
    }

    let content = NonEmpty::new(vec![Inline::Text(Text::new(&text)?)])?;
    let ratio = lines.first()?.size / body;

    Some(
        match HEADING_SIZES
            .iter()
            .find(|(threshold, _)| ratio >= *threshold)
            .and_then(|(_, level)| HeadingLevel::new(*level))
        {
            Some(level) => Block::Heading { level, content },
            None => Block::Paragraph { content },
        },
    )
}

fn advance(text: &str, size: f64) -> f64 {
    text.chars()
        .map(|character| match script::is_wide(character) {
            true => WIDE_ADVANCE,
            false => NARROW_ADVANCE,
        })
        .sum::<f64>()
        * size
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: f64 = 10.0;
    const LINE_HEIGHT: f64 = 12.0;
    const LETTER_MEDIA_BOX: [f64; 4] = [0.0, 0.0, 612.0, 792.0];

    fn span(text: &str, x: f64, y: f64, size: f64) -> Span {
        Span {
            text: text.to_string(),
            x,
            y,
            size,
        }
    }

    fn page(spans: Vec<Span>) -> Page {
        Page {
            spans,
            media_box: LETTER_MEDIA_BOX,
        }
    }

    fn paragraph(text: &str) -> Block {
        Block::Paragraph {
            content: NonEmpty::new(vec![Inline::Text(Text::new(text).unwrap())]).unwrap(),
        }
    }

    fn heading(level: u8, text: &str) -> Block {
        Block::Heading {
            level: HeadingLevel::new(level).unwrap(),
            content: NonEmpty::new(vec![Inline::Text(Text::new(text).unwrap())]).unwrap(),
        }
    }

    mod document_blocks {
        use super::*;

        #[test]
        fn when_document_blocks_with_one_line_then_returns_one_paragraph() {
            let pages = [page(vec![span("Hello world.", 50.0, 700.0, BODY)])];

            assert_eq!(document_blocks(&pages), vec![paragraph("Hello world.")]);
        }

        #[test]
        fn when_document_blocks_with_no_pages_then_returns_nothing() {
            assert_eq!(document_blocks(&[]), Vec::new());
        }

        #[test]
        fn when_document_blocks_with_wrapped_latin_lines_then_joins_them_with_a_space() {
            let pages = [page(vec![
                span("the first line", 50.0, 700.0, BODY),
                span("and the second", 50.0, 700.0 - LINE_HEIGHT, BODY),
            ])];

            assert_eq!(
                document_blocks(&pages),
                vec![paragraph("the first line and the second")]
            );
        }

        #[test]
        fn when_document_blocks_with_wrapped_japanese_lines_then_joins_them_without_a_space() {
            let pages = [page(vec![
                span("これは一行目で", 50.0, 700.0, BODY),
                span("これは二行目です。", 50.0, 700.0 - LINE_HEIGHT, BODY),
            ])];

            assert_eq!(
                document_blocks(&pages),
                vec![paragraph("これは一行目でこれは二行目です。")]
            );
        }

        #[test]
        fn when_document_blocks_with_a_wide_vertical_gap_then_starts_a_new_paragraph() {
            let pages = [page(vec![
                span("first paragraph", 50.0, 700.0, BODY),
                span("second paragraph", 50.0, 700.0 - BODY * 3.0, BODY),
            ])];

            assert_eq!(
                document_blocks(&pages),
                vec![paragraph("first paragraph"), paragraph("second paragraph")]
            );
        }

        #[test]
        fn when_document_blocks_with_an_indented_line_then_starts_a_new_paragraph() {
            let pages = [page(vec![
                span("だんらくの一行目", 50.0, 700.0, BODY),
                span("つぎのだんらく", 50.0 + BODY, 700.0 - LINE_HEIGHT, BODY),
            ])];

            assert_eq!(
                document_blocks(&pages),
                vec![paragraph("だんらくの一行目"), paragraph("つぎのだんらく")]
            );
        }

        #[test]
        fn when_document_blocks_with_spans_out_of_order_then_reads_them_left_to_right() {
            let pages = [page(vec![
                span("world", 100.0, 700.0, BODY),
                span("Hello", 50.0, 700.0, BODY),
            ])];

            assert_eq!(document_blocks(&pages), vec![paragraph("Hello world")]);
        }

        #[test]
        fn when_document_blocks_with_adjacent_japanese_spans_then_keeps_them_unspaced() {
            let pages = [page(vec![
                span("定理", 50.0, 700.0, BODY),
                span("証明", 50.0 + BODY * 2.0, 700.0, BODY),
            ])];

            assert_eq!(document_blocks(&pages), vec![paragraph("定理証明")]);
        }

        #[test]
        fn when_document_blocks_with_a_larger_line_then_returns_a_heading() {
            let body = "body text".repeat(4);
            let pages = [page(vec![
                span("Chapter", 50.0, 700.0, BODY * 2.0),
                span(&body, 50.0, 650.0, BODY),
            ])];

            assert_eq!(
                document_blocks(&pages),
                vec![heading(1, "Chapter"), paragraph(&body)]
            );
        }

        #[test]
        fn when_document_blocks_with_heading_sizes_then_returns_their_levels() {
            for (size, level) in [(1.8, 1), (1.3, 2), (1.15, 3), (1.08, 4)] {
                let pages = [page(vec![
                    span("Title", 50.0, 700.0, BODY * size),
                    span(&"body text".repeat(4), 50.0, 600.0, BODY),
                ])];

                assert_eq!(document_blocks(&pages)[0], heading(level, "Title"));
            }
        }

        #[test]
        fn when_document_blocks_with_a_slightly_larger_line_then_keeps_it_a_paragraph() {
            let pages = [page(vec![
                span("body text", 50.0, 700.0, BODY * 1.05),
                span(&"more body text".repeat(4), 50.0, 600.0, BODY),
            ])];

            assert_eq!(document_blocks(&pages)[0], paragraph("body text"));
        }

        #[test]
        fn when_document_blocks_with_text_in_the_margins_then_drops_it() {
            let pages = [page(vec![
                span("running header", 50.0, 780.0, BODY),
                span("body text", 50.0, 700.0, BODY),
            ])];

            assert_eq!(document_blocks(&pages), vec![paragraph("body text")]);
        }

        #[test]
        fn when_document_blocks_with_several_pages_then_reads_them_in_order() {
            let pages = [
                page(vec![span("first page", 50.0, 700.0, BODY)]),
                page(vec![span("second page", 50.0, 700.0, BODY)]),
            ];

            assert_eq!(
                document_blocks(&pages),
                vec![paragraph("first page"), paragraph("second page")]
            );
        }

        #[test]
        fn when_document_blocks_with_code_sized_text_everywhere_then_body_is_the_larger_common_size()
         {
            let pages = [page(vec![
                span(&"code sample".repeat(10), 50.0, 700.0, BODY / 2.0),
                span(&"prose text".repeat(8), 50.0, 600.0, BODY),
            ])];

            assert_eq!(
                document_blocks(&pages),
                vec![
                    paragraph(&"code sample".repeat(10)),
                    paragraph(&"prose text".repeat(8))
                ]
            );
        }
    }
}
