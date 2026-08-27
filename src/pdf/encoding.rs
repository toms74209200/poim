const STANDARD_NAME: &str = "StandardEncoding";
const WIN_ANSI_NAME: &str = "WinAnsiEncoding";
const MAC_ROMAN_NAME: &str = "MacRomanEncoding";
const GLYPH_SUFFIX: char = '.';
const UNICODE_NAME_PREFIX: &str = "uni";
const UNICODE_SCALAR_PREFIX: &str = "u";
const UNICODE_NAME_DIGITS: usize = 4;
const UNICODE_SCALAR_DIGITS: core::ops::RangeInclusive<usize> = 4..=6;
const HEX_RADIX: u32 = 16;
const CODES: usize = 256;

#[derive(Debug, Clone, PartialEq)]
pub struct Codes(Box<[Option<char>; CODES]>);

impl Codes {
    pub fn char_of(&self, code: u8) -> Option<char> {
        self.0[usize::from(code)]
    }
}

pub fn codes<'a>(
    base: Option<&str>,
    differences: impl IntoIterator<Item = (usize, &'a str)>,
) -> Codes {
    let mut codes = match base {
        Some(WIN_ANSI_NAME) => WIN_ANSI,
        Some(MAC_ROMAN_NAME) => MAC_ROMAN,
        Some(STANDARD_NAME) | Some(_) | None => STANDARD,
    };

    for (code, name) in differences {
        if let Some(code) = codes.get_mut(code) {
            *code = glyph_char(name);
        }
    }

    Codes(Box::new(codes))
}

fn glyph_char(name: &str) -> Option<char> {
    let name = name.split(GLYPH_SUFFIX).next()?;

    GLYPHS
        .binary_search_by(|(glyph, _)| (*glyph).cmp(name))
        .ok()
        .map(|index| GLYPHS[index].1)
        .or_else(|| unicode_glyph(name))
}

fn unicode_glyph(name: &str) -> Option<char> {
    let digits = match name.strip_prefix(UNICODE_NAME_PREFIX) {
        Some(digits) if digits.len() == UNICODE_NAME_DIGITS => digits,
        Some(_) => return None,
        None => match name.strip_prefix(UNICODE_SCALAR_PREFIX) {
            Some(digits) if UNICODE_SCALAR_DIGITS.contains(&digits.len()) => digits,
            _ => return None,
        },
    };

    if !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }

    char::from_u32(u32::from_str_radix(digits, HEX_RADIX).ok()?)
}

#[rustfmt::skip]
static STANDARD: [Option<char>; 256] = [
    None, None, None, None, None, None, None, None,
    None, None, None, None, None, None, None, None,
    None, None, None, None, None, None, None, None,
    None, None, None, None, None, None, None, None,
    Some(' '), Some('!'), Some('"'), Some('#'), Some('$'), Some('%'), Some('&'), Some('’'),
    Some('('), Some(')'), Some('*'), Some('+'), Some(','), Some('-'), Some('.'), Some('/'),
    Some('0'), Some('1'), Some('2'), Some('3'), Some('4'), Some('5'), Some('6'), Some('7'),
    Some('8'), Some('9'), Some(':'), Some(';'), Some('<'), Some('='), Some('>'), Some('?'),
    Some('@'), Some('A'), Some('B'), Some('C'), Some('D'), Some('E'), Some('F'), Some('G'),
    Some('H'), Some('I'), Some('J'), Some('K'), Some('L'), Some('M'), Some('N'), Some('O'),
    Some('P'), Some('Q'), Some('R'), Some('S'), Some('T'), Some('U'), Some('V'), Some('W'),
    Some('X'), Some('Y'), Some('Z'), Some('['), Some('\\'), Some(']'), Some('^'), Some('_'),
    Some('‘'), Some('a'), Some('b'), Some('c'), Some('d'), Some('e'), Some('f'), Some('g'),
    Some('h'), Some('i'), Some('j'), Some('k'), Some('l'), Some('m'), Some('n'), Some('o'),
    Some('p'), Some('q'), Some('r'), Some('s'), Some('t'), Some('u'), Some('v'), Some('w'),
    Some('x'), Some('y'), Some('z'), Some('{'), Some('|'), Some('}'), Some('~'), None,
    None, None, None, None, None, None, None, None,
    None, None, None, None, None, None, None, None,
    None, None, None, None, None, None, None, None,
    None, None, None, None, None, None, None, None,
    None, Some('¡'), Some('¢'), Some('£'), Some('⁄'), Some('¥'), Some('ƒ'), Some('§'),
    Some('¤'), Some('\''), Some('“'), Some('«'), Some('‹'), Some('›'), Some('\u{FB01}'), Some('\u{FB02}'),
    None, Some('–'), Some('†'), Some('‡'), Some('·'), None, Some('¶'), Some('•'),
    Some('‚'), Some('„'), Some('”'), Some('»'), Some('…'), Some('‰'), None, Some('¿'),
    None, Some('`'), Some('´'), Some('ˆ'), Some('˜'), Some('¯'), Some('˘'), Some('˙'),
    Some('¨'), None, Some('˚'), Some('¸'), None, Some('˝'), Some('˛'), Some('ˇ'),
    Some('—'), None, None, None, None, None, None, None,
    None, None, None, None, None, None, None, None,
    None, Some('Æ'), None, Some('ª'), None, None, None, None,
    Some('Ł'), Some('Ø'), Some('Œ'), Some('º'), None, None, None, None,
    None, Some('æ'), None, None, None, Some('ı'), None, None,
    Some('ł'), Some('ø'), Some('œ'), Some('ß'), None, None, None, None,
];

#[rustfmt::skip]
static WIN_ANSI: [Option<char>; 256] = [
    None, None, None, None, None, None, None, None,
    None, None, None, None, None, None, None, None,
    None, None, None, None, None, None, None, None,
    None, None, None, None, None, None, None, None,
    Some(' '), Some('!'), Some('"'), Some('#'), Some('$'), Some('%'), Some('&'), Some('\''),
    Some('('), Some(')'), Some('*'), Some('+'), Some(','), Some('-'), Some('.'), Some('/'),
    Some('0'), Some('1'), Some('2'), Some('3'), Some('4'), Some('5'), Some('6'), Some('7'),
    Some('8'), Some('9'), Some(':'), Some(';'), Some('<'), Some('='), Some('>'), Some('?'),
    Some('@'), Some('A'), Some('B'), Some('C'), Some('D'), Some('E'), Some('F'), Some('G'),
    Some('H'), Some('I'), Some('J'), Some('K'), Some('L'), Some('M'), Some('N'), Some('O'),
    Some('P'), Some('Q'), Some('R'), Some('S'), Some('T'), Some('U'), Some('V'), Some('W'),
    Some('X'), Some('Y'), Some('Z'), Some('['), Some('\\'), Some(']'), Some('^'), Some('_'),
    Some('`'), Some('a'), Some('b'), Some('c'), Some('d'), Some('e'), Some('f'), Some('g'),
    Some('h'), Some('i'), Some('j'), Some('k'), Some('l'), Some('m'), Some('n'), Some('o'),
    Some('p'), Some('q'), Some('r'), Some('s'), Some('t'), Some('u'), Some('v'), Some('w'),
    Some('x'), Some('y'), Some('z'), Some('{'), Some('|'), Some('}'), Some('~'), None,
    Some('€'), None, Some('‚'), Some('ƒ'), Some('„'), Some('…'), Some('†'), Some('‡'),
    Some('ˆ'), Some('‰'), Some('Š'), Some('‹'), Some('Œ'), None, Some('Ž'), None,
    None, Some('‘'), Some('’'), Some('“'), Some('”'), Some('•'), Some('–'), Some('—'),
    Some('˜'), Some('™'), Some('š'), Some('›'), Some('œ'), None, Some('ž'), Some('Ÿ'),
    Some(' '), Some('¡'), Some('¢'), Some('£'), Some('¤'), Some('¥'), Some('¦'), Some('§'),
    Some('¨'), Some('©'), Some('ª'), Some('«'), Some('¬'), Some('-'), Some('®'), Some('¯'),
    Some('°'), Some('±'), Some('²'), Some('³'), Some('´'), Some('µ'), Some('¶'), Some('·'),
    Some('¸'), Some('¹'), Some('º'), Some('»'), Some('¼'), Some('½'), Some('¾'), Some('¿'),
    Some('À'), Some('Á'), Some('Â'), Some('Ã'), Some('Ä'), Some('Å'), Some('Æ'), Some('Ç'),
    Some('È'), Some('É'), Some('Ê'), Some('Ë'), Some('Ì'), Some('Í'), Some('Î'), Some('Ï'),
    Some('Ð'), Some('Ñ'), Some('Ò'), Some('Ó'), Some('Ô'), Some('Õ'), Some('Ö'), Some('×'),
    Some('Ø'), Some('Ù'), Some('Ú'), Some('Û'), Some('Ü'), Some('Ý'), Some('Þ'), Some('ß'),
    Some('à'), Some('á'), Some('â'), Some('ã'), Some('ä'), Some('å'), Some('æ'), Some('ç'),
    Some('è'), Some('é'), Some('ê'), Some('ë'), Some('ì'), Some('í'), Some('î'), Some('ï'),
    Some('ð'), Some('ñ'), Some('ò'), Some('ó'), Some('ô'), Some('õ'), Some('ö'), Some('÷'),
    Some('ø'), Some('ù'), Some('ú'), Some('û'), Some('ü'), Some('ý'), Some('þ'), Some('ÿ'),
];

#[rustfmt::skip]
static MAC_ROMAN: [Option<char>; 256] = [
    None, None, None, None, None, None, None, None,
    None, None, None, None, None, None, None, None,
    None, None, None, None, None, None, None, None,
    None, None, None, None, None, None, None, None,
    Some(' '), Some('!'), Some('"'), Some('#'), Some('$'), Some('%'), Some('&'), Some('\''),
    Some('('), Some(')'), Some('*'), Some('+'), Some(','), Some('-'), Some('.'), Some('/'),
    Some('0'), Some('1'), Some('2'), Some('3'), Some('4'), Some('5'), Some('6'), Some('7'),
    Some('8'), Some('9'), Some(':'), Some(';'), Some('<'), Some('='), Some('>'), Some('?'),
    Some('@'), Some('A'), Some('B'), Some('C'), Some('D'), Some('E'), Some('F'), Some('G'),
    Some('H'), Some('I'), Some('J'), Some('K'), Some('L'), Some('M'), Some('N'), Some('O'),
    Some('P'), Some('Q'), Some('R'), Some('S'), Some('T'), Some('U'), Some('V'), Some('W'),
    Some('X'), Some('Y'), Some('Z'), Some('['), Some('\\'), Some(']'), Some('^'), Some('_'),
    Some('`'), Some('a'), Some('b'), Some('c'), Some('d'), Some('e'), Some('f'), Some('g'),
    Some('h'), Some('i'), Some('j'), Some('k'), Some('l'), Some('m'), Some('n'), Some('o'),
    Some('p'), Some('q'), Some('r'), Some('s'), Some('t'), Some('u'), Some('v'), Some('w'),
    Some('x'), Some('y'), Some('z'), Some('{'), Some('|'), Some('}'), Some('~'), None,
    Some('Ä'), Some('Å'), Some('Ç'), Some('É'), Some('Ñ'), Some('Ö'), Some('Ü'), Some('á'),
    Some('à'), Some('â'), Some('ä'), Some('ã'), Some('å'), Some('ç'), Some('é'), Some('è'),
    Some('ê'), Some('ë'), Some('í'), Some('ì'), Some('î'), Some('ï'), Some('ñ'), Some('ó'),
    Some('ò'), Some('ô'), Some('ö'), Some('õ'), Some('ú'), Some('ù'), Some('û'), Some('ü'),
    Some('†'), Some('°'), Some('¢'), Some('£'), Some('§'), Some('•'), Some('¶'), Some('ß'),
    Some('®'), Some('©'), Some('™'), Some('´'), Some('¨'), Some('≠'), Some('Æ'), Some('Ø'),
    Some('∞'), Some('±'), Some('≤'), Some('≥'), Some('¥'), Some('µ'), Some('∂'), Some('∑'),
    Some('∏'), Some('π'), Some('∫'), Some('ª'), Some('º'), Some('Ω'), Some('æ'), Some('ø'),
    Some('¿'), Some('¡'), Some('¬'), Some('√'), Some('ƒ'), Some('≈'), Some('∆'), Some('«'),
    Some('»'), Some('…'), Some(' '), Some('À'), Some('Ã'), Some('Õ'), Some('Œ'), Some('œ'),
    Some('–'), Some('—'), Some('“'), Some('”'), Some('‘'), Some('’'), Some('÷'), Some('◊'),
    Some('ÿ'), Some('Ÿ'), Some('⁄'), Some('¤'), Some('‹'), Some('›'), Some('\u{FB01}'), Some('\u{FB02}'),
    Some('‡'), Some('·'), Some('‚'), Some('„'), Some('‰'), Some('Â'), Some('Ê'), Some('Á'),
    Some('Ë'), Some('È'), Some('Í'), Some('Î'), Some('Ï'), Some('Ì'), Some('Ó'), Some('Ô'),
    Some('\u{F8FF}'), Some('Ò'), Some('Ú'), Some('Û'), Some('Ù'), Some('ı'), Some('ˆ'), Some('˜'),
    Some('¯'), Some('˘'), Some('˙'), Some('˚'), Some('¸'), Some('˝'), Some('˛'), Some('ˇ'),
];

static GLYPHS: [(&str, char); 243] = [
    ("A", 'A'),
    ("AE", 'Æ'),
    ("Aacute", 'Á'),
    ("Acircumflex", 'Â'),
    ("Adieresis", 'Ä'),
    ("Agrave", 'À'),
    ("Aring", 'Å'),
    ("Atilde", 'Ã'),
    ("B", 'B'),
    ("C", 'C'),
    ("Ccedilla", 'Ç'),
    ("D", 'D'),
    ("Delta", '∆'),
    ("E", 'E'),
    ("Eacute", 'É'),
    ("Ecircumflex", 'Ê'),
    ("Edieresis", 'Ë'),
    ("Egrave", 'È'),
    ("Eth", 'Ð'),
    ("Euro", '€'),
    ("F", 'F'),
    ("G", 'G'),
    ("H", 'H'),
    ("I", 'I'),
    ("Iacute", 'Í'),
    ("Icircumflex", 'Î'),
    ("Idieresis", 'Ï'),
    ("Igrave", 'Ì'),
    ("J", 'J'),
    ("K", 'K'),
    ("L", 'L'),
    ("Lslash", 'Ł'),
    ("M", 'M'),
    ("N", 'N'),
    ("Ntilde", 'Ñ'),
    ("O", 'O'),
    ("OE", 'Œ'),
    ("Oacute", 'Ó'),
    ("Ocircumflex", 'Ô'),
    ("Odieresis", 'Ö'),
    ("Ograve", 'Ò'),
    ("Omega", 'Ω'),
    ("Oslash", 'Ø'),
    ("Otilde", 'Õ'),
    ("P", 'P'),
    ("Q", 'Q'),
    ("R", 'R'),
    ("S", 'S'),
    ("Scaron", 'Š'),
    ("T", 'T'),
    ("Thorn", 'Þ'),
    ("U", 'U'),
    ("Uacute", 'Ú'),
    ("Ucircumflex", 'Û'),
    ("Udieresis", 'Ü'),
    ("Ugrave", 'Ù'),
    ("V", 'V'),
    ("W", 'W'),
    ("X", 'X'),
    ("Y", 'Y'),
    ("Yacute", 'Ý'),
    ("Ydieresis", 'Ÿ'),
    ("Z", 'Z'),
    ("Zcaron", 'Ž'),
    ("a", 'a'),
    ("aacute", 'á'),
    ("acircumflex", 'â'),
    ("acute", '´'),
    ("adieresis", 'ä'),
    ("ae", 'æ'),
    ("agrave", 'à'),
    ("ampersand", '&'),
    ("apple", '\u{F8FF}'),
    ("approxequal", '≈'),
    ("aring", 'å'),
    ("asciicircum", '^'),
    ("asciitilde", '~'),
    ("asterisk", '*'),
    ("at", '@'),
    ("atilde", 'ã'),
    ("b", 'b'),
    ("backslash", '\\'),
    ("bar", '|'),
    ("braceleft", '{'),
    ("braceright", '}'),
    ("bracketleft", '['),
    ("bracketright", ']'),
    ("breve", '˘'),
    ("brokenbar", '¦'),
    ("bullet", '•'),
    ("c", 'c'),
    ("caron", 'ˇ'),
    ("ccedilla", 'ç'),
    ("cedilla", '¸'),
    ("cent", '¢'),
    ("circumflex", 'ˆ'),
    ("colon", ':'),
    ("comma", ','),
    ("copyright", '©'),
    ("currency", '¤'),
    ("d", 'd'),
    ("dagger", '†'),
    ("daggerdbl", '‡'),
    ("degree", '°'),
    ("dieresis", '¨'),
    ("divide", '÷'),
    ("dollar", '$'),
    ("dotaccent", '˙'),
    ("dotlessi", 'ı'),
    ("e", 'e'),
    ("eacute", 'é'),
    ("ecircumflex", 'ê'),
    ("edieresis", 'ë'),
    ("egrave", 'è'),
    ("eight", '8'),
    ("ellipsis", '…'),
    ("emdash", '—'),
    ("endash", '–'),
    ("equal", '='),
    ("eth", 'ð'),
    ("exclam", '!'),
    ("exclamdown", '¡'),
    ("f", 'f'),
    ("fi", '\u{FB01}'),
    ("five", '5'),
    ("fl", '\u{FB02}'),
    ("florin", 'ƒ'),
    ("four", '4'),
    ("fraction", '⁄'),
    ("g", 'g'),
    ("germandbls", 'ß'),
    ("grave", '`'),
    ("greater", '>'),
    ("greaterequal", '≥'),
    ("guillemotleft", '«'),
    ("guillemotright", '»'),
    ("guilsinglleft", '‹'),
    ("guilsinglright", '›'),
    ("h", 'h'),
    ("hungarumlaut", '˝'),
    ("hyphen", '-'),
    ("i", 'i'),
    ("iacute", 'í'),
    ("icircumflex", 'î'),
    ("idieresis", 'ï'),
    ("igrave", 'ì'),
    ("infinity", '∞'),
    ("integral", '∫'),
    ("j", 'j'),
    ("k", 'k'),
    ("l", 'l'),
    ("less", '<'),
    ("lessequal", '≤'),
    ("logicalnot", '¬'),
    ("lozenge", '◊'),
    ("lslash", 'ł'),
    ("m", 'm'),
    ("macron", '¯'),
    ("mu", 'µ'),
    ("multiply", '×'),
    ("n", 'n'),
    ("nine", '9'),
    ("notequal", '≠'),
    ("ntilde", 'ñ'),
    ("numbersign", '#'),
    ("o", 'o'),
    ("oacute", 'ó'),
    ("ocircumflex", 'ô'),
    ("odieresis", 'ö'),
    ("oe", 'œ'),
    ("ogonek", '˛'),
    ("ograve", 'ò'),
    ("one", '1'),
    ("onehalf", '½'),
    ("onequarter", '¼'),
    ("onesuperior", '¹'),
    ("ordfeminine", 'ª'),
    ("ordmasculine", 'º'),
    ("oslash", 'ø'),
    ("otilde", 'õ'),
    ("p", 'p'),
    ("paragraph", '¶'),
    ("parenleft", '('),
    ("parenright", ')'),
    ("partialdiff", '∂'),
    ("percent", '%'),
    ("period", '.'),
    ("periodcentered", '·'),
    ("perthousand", '‰'),
    ("pi", 'π'),
    ("plus", '+'),
    ("plusminus", '±'),
    ("product", '∏'),
    ("q", 'q'),
    ("question", '?'),
    ("questiondown", '¿'),
    ("quotedbl", '"'),
    ("quotedblbase", '„'),
    ("quotedblleft", '“'),
    ("quotedblright", '”'),
    ("quoteleft", '‘'),
    ("quoteright", '’'),
    ("quotesinglbase", '‚'),
    ("quotesingle", '\''),
    ("r", 'r'),
    ("radical", '√'),
    ("registered", '®'),
    ("ring", '˚'),
    ("s", 's'),
    ("scaron", 'š'),
    ("section", '§'),
    ("semicolon", ';'),
    ("seven", '7'),
    ("six", '6'),
    ("slash", '/'),
    ("space", ' '),
    ("sterling", '£'),
    ("summation", '∑'),
    ("t", 't'),
    ("thorn", 'þ'),
    ("three", '3'),
    ("threequarters", '¾'),
    ("threesuperior", '³'),
    ("tilde", '˜'),
    ("trademark", '™'),
    ("two", '2'),
    ("twosuperior", '²'),
    ("u", 'u'),
    ("uacute", 'ú'),
    ("ucircumflex", 'û'),
    ("udieresis", 'ü'),
    ("ugrave", 'ù'),
    ("underscore", '_'),
    ("v", 'v'),
    ("w", 'w'),
    ("x", 'x'),
    ("y", 'y'),
    ("yacute", 'ý'),
    ("ydieresis", 'ÿ'),
    ("yen", '¥'),
    ("z", 'z'),
    ("zcaron", 'ž'),
    ("zero", '0'),
];

#[cfg(test)]
mod tests {
    use super::*;

    const STANDARD_NAME: Option<&str> = Some("StandardEncoding");
    const WIN_ANSI_NAME: Option<&str> = Some("WinAnsiEncoding");
    const MAC_ROMAN_NAME: Option<&str> = Some("MacRomanEncoding");
    const LATIN_LETTERS: core::ops::RangeInclusive<u8> = b'A'..=b'Z';
    const BASES: [Option<&str>; 3] = [STANDARD_NAME, WIN_ANSI_NAME, MAC_ROMAN_NAME];

    fn code(base: Option<&str>, code: u8) -> Option<char> {
        codes(base, []).char_of(code)
    }

    mod codes {
        use super::*;

        #[test]
        fn when_codes_with_latin_letters_then_every_base_agrees() {
            for letter in LATIN_LETTERS {
                for base in BASES {
                    assert_eq!(code(base, letter), Some(char::from(letter)));
                }
            }
        }

        #[test]
        fn when_codes_with_no_base_then_reads_as_standard() {
            assert_eq!(code(None, 0x27), Some('\u{2019}'));
        }

        #[test]
        fn when_codes_with_an_unknown_base_then_reads_as_standard() {
            assert_eq!(code(Some("Identity-H"), 0x27), Some('\u{2019}'));
        }

        #[test]
        fn when_codes_with_standard_and_apostrophe_code_then_returns_right_quote() {
            assert_eq!(code(STANDARD_NAME, 0x27), Some('\u{2019}'));
        }

        #[test]
        fn when_codes_with_win_ansi_and_apostrophe_code_then_returns_apostrophe() {
            assert_eq!(code(WIN_ANSI_NAME, 0x27), Some('\''));
        }

        #[test]
        fn when_codes_with_standard_and_backquote_code_then_returns_left_quote() {
            assert_eq!(code(STANDARD_NAME, 0x60), Some('\u{2018}'));
        }

        #[test]
        fn when_codes_with_win_ansi_and_backquote_code_then_returns_grave() {
            assert_eq!(code(WIN_ANSI_NAME, 0x60), Some('`'));
        }

        #[test]
        fn when_codes_with_standard_and_ligature_code_then_returns_ligature() {
            assert_eq!(code(STANDARD_NAME, 0xAE), Some('\u{FB01}'));
        }

        #[test]
        fn when_codes_with_standard_and_lslash_code_then_returns_lslash() {
            assert_eq!(code(STANDARD_NAME, 0xE8), Some('Ł'));
        }

        #[test]
        fn when_codes_with_win_ansi_and_euro_code_then_returns_euro() {
            assert_eq!(code(WIN_ANSI_NAME, 0x80), Some('€'));
        }

        #[test]
        fn when_codes_with_win_ansi_and_no_break_space_code_then_returns_space() {
            assert_eq!(code(WIN_ANSI_NAME, 0xA0), Some(' '));
        }

        #[test]
        fn when_codes_with_win_ansi_and_soft_hyphen_code_then_returns_hyphen() {
            assert_eq!(code(WIN_ANSI_NAME, 0xAD), Some('-'));
        }

        #[test]
        fn when_codes_with_mac_roman_and_no_break_space_code_then_returns_space() {
            assert_eq!(code(MAC_ROMAN_NAME, 0xCA), Some(' '));
        }

        #[test]
        fn when_codes_with_mac_roman_and_currency_code_then_returns_currency() {
            assert_eq!(code(MAC_ROMAN_NAME, 0xDB), Some('¤'));
        }

        #[test]
        fn when_codes_with_mac_roman_and_bullet_code_then_returns_bullet() {
            assert_eq!(code(MAC_ROMAN_NAME, 0xA5), Some('•'));
        }

        #[test]
        fn when_codes_with_control_code_then_returns_none() {
            for base in BASES {
                assert_eq!(code(base, 0x00), None);
            }
        }

        #[test]
        fn when_codes_with_standard_and_unassigned_code_then_returns_none() {
            assert_eq!(code(STANDARD_NAME, 0xC0), None);
        }

        #[test]
        fn when_codes_with_win_ansi_and_unassigned_code_then_returns_none() {
            assert_eq!(code(WIN_ANSI_NAME, 0x81), None);
        }

        #[test]
        fn when_codes_with_differences_then_overrides_those_codes() {
            let codes = codes(WIN_ANSI_NAME, [(0x41, "bullet"), (0x42, "uni65E5")]);

            assert_eq!(codes.char_of(0x41), Some('•'));
            assert_eq!(codes.char_of(0x42), Some('日'));
            assert_eq!(codes.char_of(0x43), Some('C'));
        }

        #[test]
        fn when_codes_with_an_unknown_glyph_in_differences_then_drops_that_code() {
            let codes = codes(WIN_ANSI_NAME, [(0x41, "nosuchglyph")]);

            assert_eq!(codes.char_of(0x41), None);
        }

        #[test]
        fn when_codes_with_a_difference_beyond_the_table_then_keeps_the_others() {
            let codes = codes(WIN_ANSI_NAME, [(512, "bullet")]);

            assert_eq!(codes.char_of(0x41), Some('A'));
        }
    }

    mod glyph_char {
        use super::*;

        #[test]
        fn when_glyph_char_with_standard_name_then_returns_char() {
            assert_eq!(glyph_char("Lslash"), Some('Ł'));
        }

        #[test]
        fn when_glyph_char_with_ligature_name_then_returns_ligature() {
            assert_eq!(glyph_char("fi"), Some('\u{FB01}'));
        }

        #[test]
        fn when_glyph_char_with_variant_suffix_then_returns_base_char() {
            assert_eq!(glyph_char("a.sc"), Some('a'));
        }

        #[test]
        fn when_glyph_char_with_uni_name_then_returns_char() {
            assert_eq!(glyph_char("uni0041"), Some('A'));
        }

        #[test]
        fn when_glyph_char_with_scalar_name_then_returns_char() {
            assert_eq!(glyph_char("u1F600"), Some('\u{1F600}'));
        }

        #[test]
        fn when_glyph_char_with_short_uni_name_then_returns_none() {
            assert_eq!(glyph_char("uni41"), None);
        }

        #[test]
        fn when_glyph_char_with_non_hex_uni_name_then_returns_none() {
            assert_eq!(glyph_char("unicorn"), None);
        }

        #[test]
        fn when_glyph_char_with_surrogate_name_then_returns_none() {
            assert_eq!(glyph_char("uniD800"), None);
        }

        #[test]
        fn when_glyph_char_with_notdef_then_returns_none() {
            assert_eq!(glyph_char(".notdef"), None);
        }

        #[test]
        fn when_glyph_char_with_unknown_name_then_returns_none() {
            assert_eq!(glyph_char("nosuchglyph"), None);
        }

        #[test]
        fn when_glyph_char_with_every_table_entry_then_returns_that_entry() {
            for (name, char) in GLYPHS {
                assert_eq!(glyph_char(name), Some(char));
            }
        }
    }
}
