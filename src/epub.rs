const OPF_MEDIA_TYPE: &[u8] = b"application/oebps-package+xml";

#[derive(Debug, Clone, PartialEq)]
pub enum EpubError {
    ContainerNotFound,
    RootfileNotFound,
    SpineNotFound,
    ManifestItemNotFound,
    ZipError(crate::zip::ZipError),
}

pub fn find_opf_path(container_xml: &[u8]) -> Result<String, EpubError> {
    let xml = core::str::from_utf8(container_xml).map_err(|_| EpubError::RootfileNotFound)?;

    let mut search_from = 0;
    while let Some(tag_start) = find_tag_start(xml, "rootfile", search_from) {
        let tag_end = match xml[tag_start..].find('>') {
            Some(pos) => tag_start + pos,
            None => break,
        };
        let tag = &xml[tag_start..=tag_end];

        if let Some(media_type) = extract_attribute(tag, "media-type")
            && media_type.as_bytes() == OPF_MEDIA_TYPE
            && let Some(path) = extract_attribute(tag, "full-path")
        {
            return Ok(path.to_string());
        }
        search_from = tag_end;
    }

    Err(EpubError::RootfileNotFound)
}

pub fn find_opf_path_from_epub(data: &[u8]) -> Result<String, EpubError> {
    let container = crate::zip::extract_by_name(data, b"META-INF/container.xml")
        .map_err(EpubError::ZipError)?;
    find_opf_path(&container)
}

pub fn parse_spine(opf_xml: &[u8]) -> Result<Vec<String>, EpubError> {
    let xml = core::str::from_utf8(opf_xml).map_err(|_| EpubError::SpineNotFound)?;

    let mut manifest: Vec<(&str, &str)> = Vec::new();
    let mut search_from = 0;
    while let Some(tag_start) = find_tag_start(xml, "item", search_from) {
        let tag_end = match xml[tag_start..].find('>') {
            Some(pos) => tag_start + pos,
            None => break,
        };
        let tag = &xml[tag_start..=tag_end];

        if let Some(id) = extract_attribute(tag, "id")
            && let Some(href) = extract_attribute(tag, "href")
        {
            manifest.push((id, href));
        }
        search_from = tag_end;
    }

    let mut spine_hrefs: Vec<String> = Vec::new();
    search_from = 0;
    while let Some(tag_start) = find_tag_start(xml, "itemref", search_from) {
        let tag_end = match xml[tag_start..].find('>') {
            Some(pos) => tag_start + pos,
            None => break,
        };
        let tag = &xml[tag_start..=tag_end];

        if let Some(idref) = extract_attribute(tag, "idref") {
            let href = manifest
                .iter()
                .find(|(id, _)| *id == idref)
                .map(|(_, href)| *href)
                .ok_or(EpubError::ManifestItemNotFound)?;
            spine_hrefs.push(href.to_string());
        }
        search_from = tag_end;
    }

    if spine_hrefs.is_empty() {
        return Err(EpubError::SpineNotFound);
    }

    Ok(spine_hrefs)
}

fn find_tag_start(xml: &str, tag_name: &str, from: usize) -> Option<usize> {
    let haystack = &xml[from..];
    let mut pos = 0;
    while pos < haystack.len() {
        if let Some(lt) = haystack[pos..].find('<') {
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
        } else {
            return None;
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

    mod find_opf_path {
        use super::*;

        #[test]
        fn when_parse_with_standard_container_then_returns_opf_path() {
            let xml = br#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#;

            let path = find_opf_path(xml).unwrap();

            assert_eq!(path, "OEBPS/content.opf");
        }

        #[test]
        fn when_parse_with_single_quotes_then_returns_opf_path() {
            let xml = b"<container><rootfiles><rootfile full-path='book.opf' media-type='application/oebps-package+xml'/></rootfiles></container>";

            let path = find_opf_path(xml).unwrap();

            assert_eq!(path, "book.opf");
        }

        #[test]
        fn when_parse_with_reversed_attribute_order_then_returns_opf_path() {
            let xml = br#"<container><rootfiles><rootfile media-type="application/oebps-package+xml" full-path="content.opf"/></rootfiles></container>"#;

            let path = find_opf_path(xml).unwrap();

            assert_eq!(path, "content.opf");
        }

        #[test]
        fn when_parse_with_nested_path_then_returns_full_path() {
            let xml = br#"<container><rootfiles><rootfile full-path="EPUB/package/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#;

            let path = find_opf_path(xml).unwrap();

            assert_eq!(path, "EPUB/package/content.opf");
        }

        #[test]
        fn when_parse_with_multiple_rootfiles_then_returns_opf_one() {
            let xml = br#"<container><rootfiles>
<rootfile full-path="other.pdf" media-type="application/pdf"/>
<rootfile full-path="book.opf" media-type="application/oebps-package+xml"/>
</rootfiles></container>"#;

            let path = find_opf_path(xml).unwrap();

            assert_eq!(path, "book.opf");
        }

        #[test]
        fn when_parse_with_extra_whitespace_then_returns_opf_path() {
            let xml = br#"<container>
  <rootfiles>
    <rootfile
      full-path = "OEBPS/content.opf"
      media-type = "application/oebps-package+xml"
    />
  </rootfiles>
</container>"#;

            let path = find_opf_path(xml).unwrap();

            assert_eq!(path, "OEBPS/content.opf");
        }

        #[test]
        fn when_parse_with_no_rootfile_then_returns_error() {
            let xml = br#"<container><rootfiles></rootfiles></container>"#;

            assert_eq!(find_opf_path(xml), Err(EpubError::RootfileNotFound));
        }

        #[test]
        fn when_parse_with_wrong_media_type_then_returns_error() {
            let xml = br#"<container><rootfiles><rootfile full-path="doc.pdf" media-type="application/pdf"/></rootfiles></container>"#;

            assert_eq!(find_opf_path(xml), Err(EpubError::RootfileNotFound));
        }
    }

    mod parse_spine {
        use super::*;

        #[test]
        fn when_parse_with_standard_opf_then_returns_hrefs_in_spine_order() {
            let opf = br#"<?xml version="1.0" encoding="UTF-8"?>
<package version="3.0">
  <manifest>
    <item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
    <item id="ch2" href="chapter2.xhtml" media-type="application/xhtml+xml"/>
    <item id="ch3" href="chapter3.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="ch1"/>
    <itemref idref="ch3"/>
    <itemref idref="ch2"/>
  </spine>
</package>"#;

            let hrefs = parse_spine(opf).unwrap();

            assert_eq!(
                hrefs,
                vec!["chapter1.xhtml", "chapter3.xhtml", "chapter2.xhtml"]
            );
        }

        #[test]
        fn when_parse_with_single_spine_item_then_returns_single_href() {
            let opf = br#"<package>
  <manifest><item id="content" href="main.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="content"/></spine>
</package>"#;

            let hrefs = parse_spine(opf).unwrap();

            assert_eq!(hrefs, vec!["main.xhtml"]);
        }

        #[test]
        fn when_parse_with_nested_href_then_returns_full_path() {
            let opf = br#"<package>
  <manifest><item id="ch1" href="Text/chapter1.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#;

            let hrefs = parse_spine(opf).unwrap();

            assert_eq!(hrefs, vec!["Text/chapter1.xhtml"]);
        }

        #[test]
        fn when_parse_with_no_spine_then_returns_error() {
            let opf = br#"<package>
  <manifest><item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/></manifest>
</package>"#;

            assert_eq!(parse_spine(opf), Err(EpubError::SpineNotFound));
        }

        #[test]
        fn when_parse_with_empty_spine_then_returns_error() {
            let opf = br#"<package>
  <manifest><item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine></spine>
</package>"#;

            assert_eq!(parse_spine(opf), Err(EpubError::SpineNotFound));
        }

        #[test]
        fn when_parse_with_unresolved_idref_then_returns_error() {
            let opf = br#"<package>
  <manifest><item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="missing"/></spine>
</package>"#;

            assert_eq!(parse_spine(opf), Err(EpubError::ManifestItemNotFound));
        }

        #[test]
        fn when_parse_with_manifest_items_without_href_then_skips_them() {
            let opf = br#"<package>
  <manifest>
    <item id="toc" media-type="application/x-dtbncx+xml"/>
    <item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#;

            let hrefs = parse_spine(opf).unwrap();

            assert_eq!(hrefs, vec!["chapter1.xhtml"]);
        }
    }

    mod extract_attribute {
        use super::*;

        #[test]
        fn when_extract_with_double_quotes_then_returns_value() {
            let tag = r#"<rootfile full-path="content.opf" />"#;
            assert_eq!(extract_attribute(tag, "full-path"), Some("content.opf"));
        }

        #[test]
        fn when_extract_with_single_quotes_then_returns_value() {
            let tag = "<rootfile full-path='content.opf' />";
            assert_eq!(extract_attribute(tag, "full-path"), Some("content.opf"));
        }

        #[test]
        fn when_extract_with_missing_attr_then_returns_none() {
            let tag = r#"<rootfile media-type="text/xml" />"#;
            assert_eq!(extract_attribute(tag, "full-path"), None);
        }

        #[test]
        fn when_extract_with_partial_name_match_then_skips() {
            let tag = r#"<tag data-full-path="wrong" full-path="right" />"#;
            assert_eq!(extract_attribute(tag, "full-path"), Some("right"));
        }
    }
}
