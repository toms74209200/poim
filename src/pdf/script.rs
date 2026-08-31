const WIDE_RANGES: [(char, char); 6] = [
    ('\u{2E80}', '\u{A4CF}'),
    ('\u{AC00}', '\u{D7AF}'),
    ('\u{F900}', '\u{FAFF}'),
    ('\u{FE30}', '\u{FE4F}'),
    ('\u{FF00}', '\u{FF60}'),
    ('\u{20000}', '\u{3FFFF}'),
];

pub fn is_wide(character: char) -> bool {
    WIDE_RANGES
        .iter()
        .any(|(low, high)| (*low..=*high).contains(&character))
}

pub fn separable(before: &str, after: &str) -> bool {
    let (Some(last), Some(first)) = (before.chars().last(), after.chars().next()) else {
        return false;
    };

    !last.is_whitespace() && !first.is_whitespace() && !is_wide(last) && !is_wide(first)
}

#[cfg(test)]
mod tests {
    use super::*;

    mod is_wide {
        use super::*;

        #[test]
        fn when_is_wide_with_kana_then_returns_true() {
            assert!(is_wide('あ'));
        }

        #[test]
        fn when_is_wide_with_kanji_then_returns_true() {
            assert!(is_wide('本'));
        }

        #[test]
        fn when_is_wide_with_full_width_punctuation_then_returns_true() {
            assert!(is_wide('、'));
        }

        #[test]
        fn when_is_wide_with_latin_then_returns_false() {
            assert!(!is_wide('A'));
        }
    }

    mod separable {
        use super::*;

        #[test]
        fn when_separable_with_latin_on_both_sides_then_returns_true() {
            assert!(separable("Hello", "world"));
        }

        #[test]
        fn when_separable_with_japanese_on_either_side_then_returns_false() {
            assert!(!separable("定理", "証明"));
            assert!(!separable("Lean", "は"));
            assert!(!separable("方々が", "Lean"));
        }

        #[test]
        fn when_separable_with_a_space_already_there_then_returns_false() {
            assert!(!separable("Hello ", "world"));
            assert!(!separable("Hello", " world"));
        }

        #[test]
        fn when_separable_with_nothing_on_either_side_then_returns_false() {
            assert!(!separable("", "world"));
            assert!(!separable("Hello", ""));
        }
    }
}
