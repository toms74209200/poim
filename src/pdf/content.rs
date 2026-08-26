use super::{Object, PdfError};

const TRUE_OPERATOR: &str = "true";
const FALSE_OPERATOR: &str = "false";
const NULL_OPERATOR: &str = "null";
const INLINE_IMAGE_BEGIN: &str = "BI";
const INLINE_IMAGE_END: &[u8] = b"EI";
const SAVE_STATE: &str = "q";
const RESTORE_STATE: &str = "Q";
const CONCAT_MATRIX: &str = "cm";
const BEGIN_TEXT: &str = "BT";
const SET_FONT: &str = "Tf";
const SET_LEADING: &str = "TL";
const MOVE_TEXT: &str = "Td";
const MOVE_TEXT_SET_LEADING: &str = "TD";
const SET_TEXT_MATRIX: &str = "Tm";
const NEXT_LINE: &str = "T*";
const SHOW_TEXT: &str = "Tj";
const SHOW_TEXT_ARRAY: &str = "TJ";
const NEXT_LINE_SHOW_TEXT: &str = "'";
const NEXT_LINE_SPACED_SHOW_TEXT: &str = "\"";
const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
const DEFAULT_FONT_SIZE: f64 = 0.0;

type Matrix = [f64; 6];

#[derive(Debug, Clone, PartialEq)]
pub struct Operation {
    pub operator: String,
    pub operands: Vec<Object>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TextPart {
    Text(Vec<u8>),
    Adjustment(f64),
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextItem {
    pub parts: Vec<TextPart>,
    pub x: f64,
    pub y: f64,
    pub font: String,
    pub size: f64,
}

pub fn parse_content(data: &[u8]) -> Result<Vec<Operation>, PdfError> {
    let mut operations = Vec::new();
    let mut operands: Vec<Object> = Vec::new();
    let mut position = 0;
    loop {
        position = super::skip_blanks(data, position);
        let Some(byte) = data.get(position).copied() else {
            return Ok(operations);
        };

        if is_operand_start(byte) {
            let (object, after_object) = super::parse_object(data, position)?;
            operands.push(object);
            position = after_object;
            continue;
        }

        let (token, after_token) = read_token(data, position)?;
        position = after_token;
        match token.as_str() {
            TRUE_OPERATOR => operands.push(Object::Boolean(true)),
            FALSE_OPERATOR => operands.push(Object::Boolean(false)),
            NULL_OPERATOR => operands.push(Object::Null),
            INLINE_IMAGE_BEGIN => {
                operands.clear();
                position = skip_inline_image(data, after_token);
            }
            _ => operations.push(Operation {
                operator: token,
                operands: core::mem::take(&mut operands),
            }),
        }
    }
}

pub fn extract_text_items(operations: &[Operation]) -> Vec<TextItem> {
    let mut state = State::new();
    let mut items = Vec::new();
    for operation in operations {
        let operands = operation.operands.as_slice();
        match operation.operator.as_str() {
            SAVE_STATE => state.save(),
            RESTORE_STATE => state.restore(),
            CONCAT_MATRIX => {
                if let Some(matrix) = matrix_operand(operands) {
                    state.concat(matrix);
                }
            }
            BEGIN_TEXT => state.begin_text(),
            SET_FONT => {
                if let [Object::Name(name), size] = operands
                    && let Some(size) = size.as_f64()
                {
                    state.set_font(name, size);
                }
            }
            SET_LEADING => {
                if let Some(leading) = number_operand(operands) {
                    state.set_leading(leading);
                }
            }
            MOVE_TEXT => {
                if let Some((x, y)) = offset_operand(operands) {
                    state.translate(x, y);
                }
            }
            MOVE_TEXT_SET_LEADING => {
                if let Some((x, y)) = offset_operand(operands) {
                    state.set_leading(-y);
                    state.translate(x, y);
                }
            }
            SET_TEXT_MATRIX => {
                if let Some(matrix) = matrix_operand(operands) {
                    state.set_matrix(matrix);
                }
            }
            NEXT_LINE => state.next_line(),
            SHOW_TEXT | SHOW_TEXT_ARRAY => {
                if let Some(parts) = text_operand(operands) {
                    items.push(state.item(parts));
                }
            }
            NEXT_LINE_SHOW_TEXT | NEXT_LINE_SPACED_SHOW_TEXT => {
                if let Some(parts) = text_operand(operands) {
                    state.next_line();
                    items.push(state.item(parts));
                }
            }
            _ => {}
        }
    }

    items
}

#[derive(Clone)]
struct Graphics {
    ctm: Matrix,
    leading: f64,
    font: String,
    size: f64,
}

struct State {
    graphics: Graphics,
    stack: Vec<Graphics>,
    matrix: Matrix,
    line: Matrix,
}

impl State {
    fn new() -> Self {
        State {
            graphics: Graphics {
                ctm: IDENTITY,
                leading: 0.0,
                font: String::new(),
                size: DEFAULT_FONT_SIZE,
            },
            stack: Vec::new(),
            matrix: IDENTITY,
            line: IDENTITY,
        }
    }

    fn save(&mut self) {
        self.stack.push(self.graphics.clone());
    }

    fn restore(&mut self) {
        if let Some(graphics) = self.stack.pop() {
            self.graphics = graphics;
        }
    }

    fn concat(&mut self, matrix: Matrix) {
        self.graphics.ctm = multiply(matrix, self.graphics.ctm);
    }

    fn set_font(&mut self, font: &str, size: f64) {
        self.graphics.font = font.to_string();
        self.graphics.size = size;
    }

    fn set_leading(&mut self, leading: f64) {
        self.graphics.leading = leading;
    }

    fn begin_text(&mut self) {
        self.matrix = IDENTITY;
        self.line = IDENTITY;
    }

    fn set_matrix(&mut self, matrix: Matrix) {
        self.matrix = matrix;
        self.line = matrix;
    }

    fn translate(&mut self, x: f64, y: f64) {
        self.line = multiply([1.0, 0.0, 0.0, 1.0, x, y], self.line);
        self.matrix = self.line;
    }

    fn next_line(&mut self) {
        self.translate(0.0, -self.graphics.leading);
    }

    fn item(&self, parts: Vec<TextPart>) -> TextItem {
        let rendering = multiply(self.matrix, self.graphics.ctm);

        TextItem {
            parts,
            x: rendering[4],
            y: rendering[5],
            font: self.graphics.font.clone(),
            size: self.graphics.size * (rendering[2].powi(2) + rendering[3].powi(2)).sqrt(),
        }
    }
}

fn multiply(left: Matrix, right: Matrix) -> Matrix {
    [
        left[0] * right[0] + left[1] * right[2],
        left[0] * right[1] + left[1] * right[3],
        left[2] * right[0] + left[3] * right[2],
        left[2] * right[1] + left[3] * right[3],
        left[4] * right[0] + left[5] * right[2] + right[4],
        left[4] * right[1] + left[5] * right[3] + right[5],
    ]
}

fn is_operand_start(byte: u8) -> bool {
    byte.is_ascii_digit() || matches!(byte, b'/' | b'(' | b'<' | b'[' | b'+' | b'-' | b'.')
}

fn read_token(data: &[u8], from: usize) -> Result<(String, usize), PdfError> {
    let mut position = from;
    while data
        .get(position)
        .is_some_and(|byte| super::is_regular(*byte))
    {
        position += 1;
    }
    if position == from {
        return Err(PdfError::MalformedObject);
    }

    let token = core::str::from_utf8(&data[from..position])
        .map_err(|_| PdfError::MalformedObject)?
        .to_string();

    Ok((token, position))
}

fn skip_inline_image(data: &[u8], from: usize) -> usize {
    let mut position = from;
    while let Some(found) = find_from(data, INLINE_IMAGE_END, position) {
        let after_keyword = found + INLINE_IMAGE_END.len();
        let separated_before = found == 0 || !super::is_regular(data[found - 1]);
        let separated_after = data
            .get(after_keyword)
            .is_none_or(|byte| !super::is_regular(*byte));
        if separated_before && separated_after {
            return after_keyword;
        }
        position = after_keyword;
    }

    data.len()
}

fn find_from(data: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    data.get(from..)?
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|index| index + from)
}

fn matrix_operand(operands: &[Object]) -> Option<Matrix> {
    let [a, b, c, d, e, f] = operands else {
        return None;
    };

    Some([
        a.as_f64()?,
        b.as_f64()?,
        c.as_f64()?,
        d.as_f64()?,
        e.as_f64()?,
        f.as_f64()?,
    ])
}

fn offset_operand(operands: &[Object]) -> Option<(f64, f64)> {
    let [x, y] = operands else {
        return None;
    };

    Some((x.as_f64()?, y.as_f64()?))
}

fn number_operand(operands: &[Object]) -> Option<f64> {
    let [value] = operands else {
        return None;
    };

    value.as_f64()
}

fn text_operand(operands: &[Object]) -> Option<Vec<TextPart>> {
    match operands.last()? {
        Object::String(bytes) => Some(vec![TextPart::Text(bytes.clone())]),
        Object::Array(items) => Some(
            items
                .iter()
                .filter_map(|item| match item {
                    Object::String(bytes) => Some(TextPart::Text(bytes.clone())),
                    other => other.as_f64().map(TextPart::Adjustment),
                })
                .collect(),
        ),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn operations(source: &str) -> Vec<Operation> {
        parse_content(source.as_bytes()).unwrap()
    }

    fn positions(items: &[TextItem]) -> Vec<(f64, f64)> {
        items.iter().map(|item| (item.x, item.y)).collect()
    }

    mod parse_content {
        use super::*;

        #[test]
        fn when_parse_with_text_object_then_returns_operations() {
            let result = parse_content(b"BT /F1 12 Tf (Hi) Tj ET");
            assert_eq!(
                result,
                Ok(vec![
                    Operation {
                        operator: "BT".to_string(),
                        operands: Vec::new(),
                    },
                    Operation {
                        operator: "Tf".to_string(),
                        operands: vec![Object::Name("F1".to_string()), Object::Integer(12),],
                    },
                    Operation {
                        operator: "Tj".to_string(),
                        operands: vec![Object::String(b"Hi".to_vec())],
                    },
                    Operation {
                        operator: "ET".to_string(),
                        operands: Vec::new(),
                    },
                ])
            );
        }

        #[test]
        fn when_parse_with_array_operand_then_returns_array() {
            let result = parse_content(b"[(A) -250 (B)] TJ");
            assert_eq!(
                result,
                Ok(vec![Operation {
                    operator: "TJ".to_string(),
                    operands: vec![Object::Array(vec![
                        Object::String(b"A".to_vec()),
                        Object::Integer(-250),
                        Object::String(b"B".to_vec()),
                    ])],
                }])
            );
        }

        #[test]
        fn when_parse_with_keyword_operands_then_returns_objects() {
            let result = parse_content(b"true false null gs");
            assert_eq!(
                result,
                Ok(vec![Operation {
                    operator: "gs".to_string(),
                    operands: vec![Object::Boolean(true), Object::Boolean(false), Object::Null],
                }])
            );
        }

        #[test]
        fn when_parse_with_dictionary_operand_then_returns_dictionary() {
            let result = parse_content(b"/Span << /Lang (en) >> BDC");
            assert_eq!(
                result,
                Ok(vec![Operation {
                    operator: "BDC".to_string(),
                    operands: vec![
                        Object::Name("Span".to_string()),
                        Object::Dictionary(vec![(
                            "Lang".to_string(),
                            Object::String(b"en".to_vec())
                        )]),
                    ],
                }])
            );
        }

        #[test]
        fn when_parse_with_real_operand_then_returns_real() {
            let result = parse_content(b"1.5 w");
            assert_eq!(
                result,
                Ok(vec![Operation {
                    operator: "w".to_string(),
                    operands: vec![Object::Real(1.5)],
                }])
            );
        }

        #[test]
        fn when_parse_with_comment_then_skips_comment() {
            let result = parse_content(b"% a comment\n(A) Tj");
            assert_eq!(
                result,
                Ok(vec![Operation {
                    operator: "Tj".to_string(),
                    operands: vec![Object::String(b"A".to_vec())],
                }])
            );
        }

        #[test]
        fn when_parse_with_quote_operators_then_returns_operations() {
            let result = parse_content(b"(A) ' 1 2 (B) \"");
            let operators: Vec<String> = result
                .unwrap()
                .into_iter()
                .map(|operation| operation.operator)
                .collect();
            assert_eq!(operators, vec!["'".to_string(), "\"".to_string()]);
        }

        #[test]
        fn when_parse_with_star_operator_then_returns_operation() {
            let result = parse_content(b"T*");
            assert_eq!(
                result,
                Ok(vec![Operation {
                    operator: "T*".to_string(),
                    operands: Vec::new(),
                }])
            );
        }

        #[test]
        fn when_parse_with_trailing_operands_then_ignores_them() {
            let result = parse_content(b"(A) Tj 1 2");
            assert_eq!(result.unwrap().len(), 1);
        }

        #[test]
        fn when_parse_with_inline_image_then_skips_image() {
            let result = parse_content(b"BI /W 1 /H 1 ID \x00\xff EI (A) Tj");
            let operators: Vec<String> = result
                .unwrap()
                .into_iter()
                .map(|operation| operation.operator)
                .collect();
            assert_eq!(operators, vec!["Tj".to_string()]);
        }

        #[test]
        fn when_parse_with_unterminated_inline_image_then_returns_no_operations() {
            let result = parse_content(b"BI /W 1 ID \x00\xff");
            assert_eq!(result, Ok(Vec::new()));
        }

        #[test]
        fn when_parse_with_embedded_end_keyword_then_skips_to_separated_keyword() {
            let result = parse_content(b"BI /W 1 ID \x00xEIy\x01 EI (A) Tj");
            let operators: Vec<String> = result
                .unwrap()
                .into_iter()
                .map(|operation| operation.operator)
                .collect();
            assert_eq!(operators, vec!["Tj".to_string()]);
        }

        #[test]
        fn when_parse_with_empty_input_then_returns_no_operations() {
            assert_eq!(parse_content(b""), Ok(Vec::new()));
        }

        #[test]
        fn when_parse_with_malformed_operand_then_returns_error() {
            assert_eq!(parse_content(b"[ (A"), Err(PdfError::UnexpectedEof));
        }

        #[test]
        fn when_parse_with_unexpected_delimiter_then_returns_error() {
            assert_eq!(parse_content(b"]"), Err(PdfError::MalformedObject));
        }

        #[test]
        fn when_parse_with_adjacent_tokens_then_returns_operations() {
            let result = parse_content(b"/F1 12 Tf(A)Tj");
            let operators: Vec<String> = result
                .unwrap()
                .into_iter()
                .map(|operation| operation.operator)
                .collect();
            assert_eq!(operators, vec!["Tf".to_string(), "Tj".to_string()]);
        }

        #[test]
        fn when_parse_with_hex_string_operand_then_returns_string() {
            let result = parse_content(b"<48656C6C6F> Tj");
            assert_eq!(
                result,
                Ok(vec![Operation {
                    operator: "Tj".to_string(),
                    operands: vec![Object::String(b"Hello".to_vec())],
                }])
            );
        }

        #[test]
        fn when_parse_with_signed_operands_then_returns_numbers() {
            let result = parse_content(b"+1 -2.5 .5 Td");
            assert_eq!(
                result,
                Ok(vec![Operation {
                    operator: "Td".to_string(),
                    operands: vec![Object::Integer(1), Object::Real(-2.5), Object::Real(0.5),],
                }])
            );
        }

        #[test]
        fn when_parse_with_line_breaks_then_returns_operations() {
            let result = parse_content(b"BT\r\n(A) Tj\r\nET");
            assert_eq!(result.unwrap().len(), 3);
        }

        #[test]
        fn when_parse_with_non_utf8_operator_then_returns_error() {
            assert_eq!(parse_content(b"\xff\xfe"), Err(PdfError::MalformedObject));
        }
    }

    mod extract_text_items {
        use super::*;

        #[test]
        fn when_extract_with_text_matrix_then_returns_item() {
            let items =
                extract_text_items(&operations("BT /F1 12 Tf 1 0 0 1 72 720 Tm (Hi) Tj ET"));
            assert_eq!(
                items,
                vec![TextItem {
                    parts: vec![TextPart::Text(b"Hi".to_vec())],
                    x: 72.0,
                    y: 720.0,
                    font: "F1".to_string(),
                    size: 12.0,
                }]
            );
        }

        #[test]
        fn when_extract_with_relative_move_then_accumulates_offsets() {
            let items = extract_text_items(&operations("BT 10 20 Td (A) Tj 5 -5 Td (B) Tj ET"));
            assert_eq!(positions(&items), vec![(10.0, 20.0), (15.0, 15.0)]);
        }

        #[test]
        fn when_extract_with_leading_then_moves_to_next_line() {
            let items = extract_text_items(&operations("BT 14 TL 0 100 Td (A) Tj T* (B) Tj ET"));
            assert_eq!(positions(&items), vec![(0.0, 100.0), (0.0, 86.0)]);
        }

        #[test]
        fn when_extract_with_move_text_set_leading_then_sets_leading() {
            let items = extract_text_items(&operations("BT 0 -14 TD (A) Tj T* (B) Tj ET"));
            assert_eq!(positions(&items), vec![(0.0, -14.0), (0.0, -28.0)]);
        }

        #[test]
        fn when_extract_with_text_array_then_returns_parts() {
            let items = extract_text_items(&operations("BT [(A) -250 (B)] TJ ET"));
            assert_eq!(
                items[0].parts,
                vec![
                    TextPart::Text(b"A".to_vec()),
                    TextPart::Adjustment(-250.0),
                    TextPart::Text(b"B".to_vec()),
                ]
            );
        }

        #[test]
        fn when_extract_with_quote_operator_then_moves_to_next_line() {
            let items = extract_text_items(&operations("BT 14 TL 0 100 Td (A) Tj (B) ' ET"));
            assert_eq!(positions(&items), vec![(0.0, 100.0), (0.0, 86.0)]);
        }

        #[test]
        fn when_extract_with_spaced_quote_operator_then_moves_to_next_line() {
            let items = extract_text_items(&operations("BT 14 TL 0 100 Td 1 2 (B) \" ET"));
            assert_eq!(positions(&items), vec![(0.0, 86.0)]);
        }

        #[test]
        fn when_extract_with_concat_matrix_then_transforms_position() {
            let items = extract_text_items(&operations(
                "2 0 0 2 10 10 cm BT /F1 12 Tf 5 5 Td (A) Tj ET",
            ));
            assert_eq!(positions(&items), vec![(20.0, 20.0)]);
            assert_eq!(items[0].size, 24.0);
        }

        #[test]
        fn when_extract_with_restored_state_then_uses_saved_matrix() {
            let items = extract_text_items(&operations("q 2 0 0 2 0 0 cm Q BT 5 5 Td (A) Tj ET"));
            assert_eq!(positions(&items), vec![(5.0, 5.0)]);
        }

        #[test]
        fn when_extract_with_unbalanced_restore_then_keeps_matrix() {
            let items = extract_text_items(&operations("Q BT 1 1 Td (A) Tj ET"));
            assert_eq!(positions(&items), vec![(1.0, 1.0)]);
        }

        #[test]
        fn when_extract_with_second_text_object_then_resets_matrix() {
            let items = extract_text_items(&operations("BT 10 10 Td (A) Tj ET BT (B) Tj ET"));
            assert_eq!(positions(&items), vec![(10.0, 10.0), (0.0, 0.0)]);
        }

        #[test]
        fn when_extract_with_scaled_text_matrix_then_scales_size() {
            let items = extract_text_items(&operations("BT /F1 10 Tf 1 0 0 2 0 0 Tm (A) Tj ET"));
            assert_eq!(items[0].size, 20.0);
        }

        #[test]
        fn when_extract_with_invalid_operand_count_then_ignores_operation() {
            let items = extract_text_items(&operations("BT 1 2 3 Td (A) Tj ET"));
            assert_eq!(positions(&items), vec![(0.0, 0.0)]);
        }

        #[test]
        fn when_extract_with_non_string_operand_then_ignores_show() {
            let items = extract_text_items(&operations("BT 1 Tj ET"));
            assert_eq!(items, Vec::new());
        }

        #[test]
        fn when_extract_without_text_then_returns_no_items() {
            let items = extract_text_items(&operations("BT ET"));
            assert_eq!(items, Vec::new());
        }

        #[test]
        fn when_extract_with_invalid_matrix_operands_then_ignores_operation() {
            let items = extract_text_items(&operations("BT 1 2 3 Tm (A) Tj ET"));
            assert_eq!(positions(&items), vec![(0.0, 0.0)]);
        }

        #[test]
        fn when_extract_with_invalid_leading_operand_then_ignores_operation() {
            let items = extract_text_items(&operations("BT 1 2 TL 0 100 Td (A) Tj T* (B) Tj ET"));
            assert_eq!(positions(&items), vec![(0.0, 100.0), (0.0, 100.0)]);
        }

        #[test]
        fn when_extract_with_restored_state_then_restores_font() {
            let items = extract_text_items(&operations("BT /F1 10 Tf q /F2 20 Tf Q (A) Tj ET"));
            assert_eq!(items[0].font, "F1".to_string());
            assert_eq!(items[0].size, 10.0);
        }

        #[test]
        fn when_extract_with_nested_states_then_restores_outer_matrix() {
            let items = extract_text_items(&operations(
                "q 2 0 0 2 0 0 cm q 3 0 0 3 0 0 cm Q BT 1 1 Td (A) Tj ET Q",
            ));
            assert_eq!(positions(&items), vec![(2.0, 2.0)]);
        }

        #[test]
        fn when_extract_with_repeated_concat_matrix_then_applies_latest_first() {
            let items =
                extract_text_items(&operations("2 0 0 2 0 0 cm 1 0 0 1 10 0 cm BT (A) Tj ET"));
            assert_eq!(positions(&items), vec![(20.0, 0.0)]);
        }

        #[test]
        fn when_extract_with_move_after_text_matrix_then_scales_offset() {
            let items = extract_text_items(&operations("BT 2 0 0 2 0 0 Tm 5 0 Td (A) Tj ET"));
            assert_eq!(positions(&items), vec![(10.0, 0.0)]);
        }

        #[test]
        fn when_extract_with_text_matrix_after_move_then_replaces_offset() {
            let items = extract_text_items(&operations("BT 10 10 Td 1 0 0 1 3 4 Tm (A) Tj ET"));
            assert_eq!(positions(&items), vec![(3.0, 4.0)]);
        }

        #[test]
        fn when_extract_with_skewed_matrix_then_scales_size() {
            let items = extract_text_items(&operations("BT /F1 10 Tf 1 2 3 4 5 6 Tm (A) Tj ET"));
            assert_eq!(positions(&items), vec![(5.0, 6.0)]);
            assert_eq!(items[0].size, 50.0);
        }

        #[test]
        fn when_extract_after_end_text_then_keeps_position() {
            let items = extract_text_items(&operations("BT 10 10 Td (A) Tj ET (B) Tj"));
            assert_eq!(positions(&items), vec![(10.0, 10.0), (10.0, 10.0)]);
        }

        #[test]
        fn when_extract_with_array_containing_names_then_ignores_them() {
            let items = extract_text_items(&operations("BT [(A) /X 5 (B)] TJ ET"));
            assert_eq!(
                items[0].parts,
                vec![
                    TextPart::Text(b"A".to_vec()),
                    TextPart::Adjustment(5.0),
                    TextPart::Text(b"B".to_vec()),
                ]
            );
        }

        #[test]
        fn when_extract_with_empty_string_then_returns_empty_part() {
            let items = extract_text_items(&operations("BT () Tj ET"));
            assert_eq!(items[0].parts, vec![TextPart::Text(Vec::new())]);
        }

        #[test]
        fn when_extract_without_operands_then_ignores_show() {
            let items = extract_text_items(&operations("BT Tj ET"));
            assert_eq!(items, Vec::new());
        }

        #[test]
        fn when_extract_with_non_numeric_matrix_operand_then_ignores_operation() {
            let items = extract_text_items(&operations("/X 0 0 1 0 0 cm BT 1 1 Td (A) Tj ET"));
            assert_eq!(positions(&items), vec![(1.0, 1.0)]);
        }

        #[test]
        fn when_extract_with_non_numeric_offset_operand_then_ignores_operation() {
            let items = extract_text_items(&operations("BT (A) 5 Td (B) Tj ET"));
            assert_eq!(positions(&items), vec![(0.0, 0.0)]);
        }

        #[test]
        fn when_extract_with_non_name_font_operand_then_ignores_operation() {
            let items = extract_text_items(&operations("BT 12 /F1 Tf (A) Tj ET"));
            assert_eq!(items[0].font, String::new());
            assert_eq!(items[0].size, 0.0);
        }

        #[test]
        fn when_extract_with_invalid_move_and_leading_operands_then_ignores_operation() {
            let items = extract_text_items(&operations("BT (A) 5 TD (B) Tj ET"));
            assert_eq!(positions(&items), vec![(0.0, 0.0)]);
        }

        #[test]
        fn when_extract_with_non_string_quote_operand_then_ignores_show() {
            let items = extract_text_items(&operations("BT 14 TL 0 100 Td 5 ' (B) Tj ET"));
            assert_eq!(positions(&items), vec![(0.0, 100.0)]);
        }

        #[test]
        fn when_extract_with_non_numeric_second_matrix_operand_then_ignores_operation() {
            let items = extract_text_items(&operations("0 /X 0 1 0 0 cm BT 1 1 Td (A) Tj ET"));
            assert_eq!(positions(&items), vec![(1.0, 1.0)]);
        }
    }
}
