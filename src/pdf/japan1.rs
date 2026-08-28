include!(concat!(env!("OUT_DIR"), "/japan1_table.rs"));

const UNMAPPED: u16 = 0;

pub fn push(cid: u32, text: &mut String) -> bool {
    let Some(code) = usize::try_from(cid)
        .ok()
        .and_then(|index| CHARS.get(index))
        .copied()
    else {
        return false;
    };
    if code != UNMAPPED {
        let Some(character) = char::from_u32(u32::from(code)) else {
            return false;
        };
        text.push(character);
        return true;
    }

    let Ok(at) = SEQUENCES.binary_search_by_key(&(cid as u16), |(cid, _)| *cid) else {
        return false;
    };
    text.push_str(SEQUENCES[at].1);

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pushed(cid: u32) -> Option<String> {
        let mut text = String::new();
        push(cid, &mut text).then_some(text)
    }

    mod push {
        use super::*;

        #[test]
        fn when_push_with_space_cid_then_writes_space() {
            assert_eq!(pushed(1), Some(" ".to_string()));
        }

        #[test]
        fn when_push_with_latin_cid_then_writes_that_letter() {
            assert_eq!(pushed(34), Some("A".to_string()));
        }

        #[test]
        fn when_push_with_kana_cid_then_writes_that_kana() {
            assert_eq!(pushed(843), Some("あ".to_string()));
        }

        #[test]
        fn when_push_with_kanji_cid_then_writes_that_kanji() {
            assert_eq!(pushed(0x0E8A), Some("本".to_string()));
        }

        #[test]
        fn when_push_with_sequence_cid_then_writes_that_text() {
            assert_eq!(pushed(8189), Some("sec".to_string()));
        }

        #[test]
        fn when_push_with_supplementary_cid_then_writes_that_char() {
            assert_eq!(pushed(7641), Some("𨳝".to_string()));
        }

        #[test]
        fn when_push_with_notdef_cid_then_writes_nothing() {
            assert_eq!(pushed(0), None);
        }

        #[test]
        fn when_push_with_cid_beyond_the_collection_then_writes_nothing() {
            assert_eq!(pushed(CHARS.len() as u32), None);
            assert_eq!(pushed(u32::MAX), None);
        }

        #[test]
        fn when_push_twice_then_appends() {
            let mut text = String::new();
            push(0x0E8A, &mut text);
            push(0x097B, &mut text);

            assert_eq!(text, "本書".to_string());
        }

        #[test]
        fn when_push_with_every_sequence_cid_then_writes_that_sequence() {
            for (cid, sequence) in SEQUENCES {
                assert_eq!(pushed(u32::from(cid)), Some(sequence.to_string()));
            }
        }
    }
}
