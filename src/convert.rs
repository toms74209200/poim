use crate::epub::{self, EpubError};
use crate::markdown;
use crate::xhtml;
use crate::zip;

#[derive(Debug, Clone, PartialEq)]
pub struct ExtractedImage {
    pub path: String,
    pub data: Vec<u8>,
}

pub fn epub_to_markdown(data: &[u8]) -> Result<String, EpubError> {
    let rendered: Vec<String> = read_spine_documents(data)?
        .iter()
        .map(|(_, content)| markdown::emit(&xhtml::parse_blocks(content)))
        .filter(|rendered| !rendered.is_empty())
        .collect();

    Ok(rendered.join("\n\n"))
}

pub fn extract_images(data: &[u8]) -> Result<Vec<ExtractedImage>, EpubError> {
    let mut images: Vec<ExtractedImage> = Vec::new();

    for (document_path, content) in read_spine_documents(data)? {
        let base = parent_dir(&document_path);
        for src in markdown::collect_image_sources(&xhtml::parse_blocks(&content)) {
            let path = resolve_path(base, &src);
            if images.iter().any(|image| image.path == path) {
                continue;
            }
            if let Ok(bytes) = zip::extract_by_name(data, path.as_bytes()) {
                images.push(ExtractedImage { path, data: bytes });
            }
        }
    }

    Ok(images)
}

fn read_spine_documents(data: &[u8]) -> Result<Vec<(String, Vec<u8>)>, EpubError> {
    let opf_path = epub::find_opf_path_from_epub(data)?;
    let opf = zip::extract_by_name(data, opf_path.as_bytes()).map_err(EpubError::ZipError)?;
    let hrefs = epub::parse_spine(&opf)?;
    let base = parent_dir(&opf_path);

    let mut documents = Vec::new();
    for href in &hrefs {
        let path = resolve_path(base, href);
        let content = zip::extract_by_name(data, path.as_bytes()).map_err(EpubError::ZipError)?;
        documents.push((path, content));
    }

    Ok(documents)
}

fn parent_dir(path: &str) -> &str {
    match path.rfind('/') {
        Some(index) => &path[..index],
        None => "",
    }
}

fn resolve_path(base: &str, href: &str) -> String {
    let combined = if base.is_empty() {
        href.to_string()
    } else {
        format!("{base}/{href}")
    };

    let mut segments: Vec<&str> = Vec::new();
    for segment in combined.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            name => segments.push(name),
        }
    }
    segments.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOCAL_FILE_HEADER_SIGNATURE: u32 = 0x04034b50;
    const CENTRAL_DIR_SIGNATURE: u32 = 0x02014b50;
    const EOCD_SIGNATURE: u32 = 0x06054b50;

    struct ZipBuilder {
        local_headers: Vec<u8>,
        entries: Vec<(Vec<u8>, u32, u32)>,
    }

    impl ZipBuilder {
        fn new() -> Self {
            Self {
                local_headers: Vec::new(),
                entries: Vec::new(),
            }
        }

        fn add(mut self, name: &str, content: &[u8]) -> Self {
            let offset = self.local_headers.len() as u32;
            let name = name.as_bytes();

            let mut header = Vec::new();
            header.extend_from_slice(&LOCAL_FILE_HEADER_SIGNATURE.to_le_bytes());
            header.extend_from_slice(&20u16.to_le_bytes());
            header.extend_from_slice(&0u16.to_le_bytes());
            header.extend_from_slice(&0u16.to_le_bytes()); // stored
            header.extend_from_slice(&0u16.to_le_bytes());
            header.extend_from_slice(&0u16.to_le_bytes());
            header.extend_from_slice(&0u32.to_le_bytes());
            header.extend_from_slice(&(content.len() as u32).to_le_bytes());
            header.extend_from_slice(&(content.len() as u32).to_le_bytes());
            header.extend_from_slice(&(name.len() as u16).to_le_bytes());
            header.extend_from_slice(&0u16.to_le_bytes());
            header.extend_from_slice(name);
            header.extend_from_slice(content);

            self.local_headers.extend_from_slice(&header);
            self.entries
                .push((name.to_vec(), content.len() as u32, offset));
            self
        }

        fn build(self) -> Vec<u8> {
            let cd_offset = self.local_headers.len() as u32;
            let mut cd = Vec::new();
            for (name, size, offset) in &self.entries {
                cd.extend_from_slice(&CENTRAL_DIR_SIGNATURE.to_le_bytes());
                cd.extend_from_slice(&20u16.to_le_bytes());
                cd.extend_from_slice(&20u16.to_le_bytes());
                cd.extend_from_slice(&0u16.to_le_bytes());
                cd.extend_from_slice(&0u16.to_le_bytes()); // stored
                cd.extend_from_slice(&0u16.to_le_bytes());
                cd.extend_from_slice(&0u16.to_le_bytes());
                cd.extend_from_slice(&0u32.to_le_bytes());
                cd.extend_from_slice(&size.to_le_bytes());
                cd.extend_from_slice(&size.to_le_bytes());
                cd.extend_from_slice(&(name.len() as u16).to_le_bytes());
                cd.extend_from_slice(&0u16.to_le_bytes());
                cd.extend_from_slice(&0u16.to_le_bytes());
                cd.extend_from_slice(&0u16.to_le_bytes());
                cd.extend_from_slice(&0u16.to_le_bytes());
                cd.extend_from_slice(&0u32.to_le_bytes());
                cd.extend_from_slice(&offset.to_le_bytes());
                cd.extend_from_slice(name);
            }

            let mut eocd = Vec::new();
            eocd.extend_from_slice(&EOCD_SIGNATURE.to_le_bytes());
            eocd.extend_from_slice(&0u16.to_le_bytes());
            eocd.extend_from_slice(&0u16.to_le_bytes());
            eocd.extend_from_slice(&(self.entries.len() as u16).to_le_bytes());
            eocd.extend_from_slice(&(self.entries.len() as u16).to_le_bytes());
            eocd.extend_from_slice(&(cd.len() as u32).to_le_bytes());
            eocd.extend_from_slice(&cd_offset.to_le_bytes());
            eocd.extend_from_slice(&0u16.to_le_bytes());

            let mut result = self.local_headers;
            result.extend_from_slice(&cd);
            result.extend_from_slice(&eocd);
            result
        }
    }

    const CONTAINER: &[u8] = br#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#;

    mod resolve_path {
        use super::*;

        #[test]
        fn when_base_is_empty_then_returns_href() {
            assert_eq!(resolve_path("", "chapter1.xhtml"), "chapter1.xhtml");
        }

        #[test]
        fn when_base_is_directory_then_joins_with_slash() {
            assert_eq!(
                resolve_path("OEBPS", "chapter1.xhtml"),
                "OEBPS/chapter1.xhtml"
            );
        }

        #[test]
        fn when_href_is_nested_then_keeps_nesting() {
            assert_eq!(
                resolve_path("OEBPS", "Text/chapter1.xhtml"),
                "OEBPS/Text/chapter1.xhtml"
            );
        }

        #[test]
        fn when_href_has_current_dir_then_removes_it() {
            assert_eq!(
                resolve_path("OEBPS", "./chapter1.xhtml"),
                "OEBPS/chapter1.xhtml"
            );
        }

        #[test]
        fn when_href_has_parent_dir_then_resolves_it() {
            assert_eq!(
                resolve_path("OEBPS/Text", "../Images/figure.xhtml"),
                "OEBPS/Images/figure.xhtml"
            );
        }
    }

    mod parent_dir {
        use super::*;

        #[test]
        fn when_path_has_directory_then_returns_it() {
            assert_eq!(parent_dir("OEBPS/content.opf"), "OEBPS");
        }

        #[test]
        fn when_path_is_nested_then_returns_full_directory() {
            assert_eq!(parent_dir("EPUB/package/content.opf"), "EPUB/package");
        }

        #[test]
        fn when_path_has_no_directory_then_returns_empty() {
            assert_eq!(parent_dir("content.opf"), "");
        }
    }

    mod epub_to_markdown {
        use super::*;

        #[test]
        fn when_single_chapter_then_converts_to_markdown() {
            let opf = br#"<package>
  <manifest><item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#;
            let chapter = b"<html><body><h1>Chapter One</h1><p>Hello world.</p></body></html>";
            let epub = ZipBuilder::new()
                .add("META-INF/container.xml", CONTAINER)
                .add("OEBPS/content.opf", opf)
                .add("OEBPS/chapter1.xhtml", chapter)
                .build();

            let markdown = epub_to_markdown(&epub).unwrap();

            assert_eq!(markdown, "# Chapter One\n\nHello world.");
        }

        #[test]
        fn when_multiple_chapters_then_joins_in_spine_order() {
            let opf = br#"<package>
  <manifest>
    <item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
    <item id="ch2" href="chapter2.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="ch2"/><itemref idref="ch1"/></spine>
</package>"#;
            let epub = ZipBuilder::new()
                .add("META-INF/container.xml", CONTAINER)
                .add("OEBPS/content.opf", opf)
                .add("OEBPS/chapter1.xhtml", b"<h1>First</h1>")
                .add("OEBPS/chapter2.xhtml", b"<h1>Second</h1>")
                .build();

            let markdown = epub_to_markdown(&epub).unwrap();

            assert_eq!(markdown, "# Second\n\n# First");
        }

        #[test]
        fn when_chapter_has_mixed_blocks_then_preserves_document_order() {
            let opf = br#"<package>
  <manifest><item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#;
            let chapter = br#"<body>
<h1>Title</h1>
<p>Intro.</p>
<ul><li>One</li><li>Two</li></ul>
<p>After list.</p>
<img src="figure.png" alt="Figure"/>
</body>"#;
            let epub = ZipBuilder::new()
                .add("META-INF/container.xml", CONTAINER)
                .add("OEBPS/content.opf", opf)
                .add("OEBPS/chapter1.xhtml", chapter)
                .build();

            let markdown = epub_to_markdown(&epub).unwrap();

            assert_eq!(
                markdown,
                "# Title\n\nIntro.\n\n- One\n- Two\n\nAfter list.\n\n![Figure](figure.png)"
            );
        }

        #[test]
        fn when_opf_at_root_then_resolves_hrefs_at_root() {
            let container = br#"<container><rootfiles>
<rootfile full-path="content.opf" media-type="application/oebps-package+xml"/>
</rootfiles></container>"#;
            let opf = br#"<package>
  <manifest><item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#;
            let epub = ZipBuilder::new()
                .add("META-INF/container.xml", container)
                .add("content.opf", opf)
                .add("chapter1.xhtml", b"<h1>Root</h1>")
                .build();

            assert_eq!(epub_to_markdown(&epub).unwrap(), "# Root");
        }

        #[test]
        fn when_spine_file_is_missing_then_returns_zip_error() {
            let opf = br#"<package>
  <manifest><item id="ch1" href="missing.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#;
            let epub = ZipBuilder::new()
                .add("META-INF/container.xml", CONTAINER)
                .add("OEBPS/content.opf", opf)
                .build();

            assert_eq!(
                epub_to_markdown(&epub),
                Err(EpubError::ZipError(crate::zip::ZipError::EntryNotFound))
            );
        }

        #[test]
        fn when_container_is_missing_then_returns_zip_error() {
            let epub = ZipBuilder::new()
                .add("OEBPS/content.opf", b"<package/>")
                .build();

            assert_eq!(
                epub_to_markdown(&epub),
                Err(EpubError::ZipError(crate::zip::ZipError::EntryNotFound))
            );
        }

        #[test]
        fn when_chapter_is_empty_then_omits_it() {
            let opf = br#"<package>
  <manifest>
    <item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
    <item id="ch2" href="chapter2.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="ch1"/><itemref idref="ch2"/></spine>
</package>"#;
            let epub = ZipBuilder::new()
                .add("META-INF/container.xml", CONTAINER)
                .add("OEBPS/content.opf", opf)
                .add("OEBPS/chapter1.xhtml", b"<html><body></body></html>")
                .add("OEBPS/chapter2.xhtml", b"<h1>Only</h1>")
                .build();

            assert_eq!(epub_to_markdown(&epub).unwrap(), "# Only");
        }
    }

    mod extract_images {
        use super::*;

        const SINGLE_CHAPTER_OPF: &[u8] = br#"<package>
  <manifest><item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#;

        #[test]
        fn when_chapter_references_image_then_extracts_it() {
            let chapter = br#"<p><img src="figure.png" alt="Figure"/></p>"#;
            let epub = ZipBuilder::new()
                .add("META-INF/container.xml", CONTAINER)
                .add("OEBPS/content.opf", SINGLE_CHAPTER_OPF)
                .add("OEBPS/chapter1.xhtml", chapter)
                .add("OEBPS/figure.png", b"PNGDATA")
                .build();

            let images = extract_images(&epub).unwrap();

            assert_eq!(
                images,
                vec![ExtractedImage {
                    path: "OEBPS/figure.png".to_string(),
                    data: b"PNGDATA".to_vec(),
                }]
            );
        }

        #[test]
        fn when_src_is_relative_to_chapter_then_resolves_from_chapter_dir() {
            let opf = br#"<package>
  <manifest><item id="ch1" href="Text/chapter1.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#;
            let chapter = br#"<img src="../Images/figure.png" alt="Figure"/>"#;
            let epub = ZipBuilder::new()
                .add("META-INF/container.xml", CONTAINER)
                .add("OEBPS/content.opf", opf)
                .add("OEBPS/Text/chapter1.xhtml", chapter)
                .add("OEBPS/Images/figure.png", b"PNGDATA")
                .build();

            let images = extract_images(&epub).unwrap();

            assert_eq!(
                images,
                vec![ExtractedImage {
                    path: "OEBPS/Images/figure.png".to_string(),
                    data: b"PNGDATA".to_vec(),
                }]
            );
        }

        #[test]
        fn when_multiple_images_then_extracts_in_document_order() {
            let chapter = br#"<img src="a.png" alt="A"/><p>text</p><img src="b.png" alt="B"/>"#;
            let epub = ZipBuilder::new()
                .add("META-INF/container.xml", CONTAINER)
                .add("OEBPS/content.opf", SINGLE_CHAPTER_OPF)
                .add("OEBPS/chapter1.xhtml", chapter)
                .add("OEBPS/a.png", b"AAA")
                .add("OEBPS/b.png", b"BBB")
                .build();

            let images = extract_images(&epub).unwrap();

            assert_eq!(
                images.iter().map(|i| i.path.as_str()).collect::<Vec<_>>(),
                vec!["OEBPS/a.png", "OEBPS/b.png"]
            );
        }

        #[test]
        fn when_same_image_referenced_from_two_chapters_then_extracts_once() {
            let opf = br#"<package>
  <manifest>
    <item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
    <item id="ch2" href="chapter2.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="ch1"/><itemref idref="ch2"/></spine>
</package>"#;
            let chapter = br#"<img src="shared.png" alt="Shared"/>"#;
            let epub = ZipBuilder::new()
                .add("META-INF/container.xml", CONTAINER)
                .add("OEBPS/content.opf", opf)
                .add("OEBPS/chapter1.xhtml", chapter)
                .add("OEBPS/chapter2.xhtml", chapter)
                .add("OEBPS/shared.png", b"SHARED")
                .build();

            let images = extract_images(&epub).unwrap();

            assert_eq!(
                images,
                vec![ExtractedImage {
                    path: "OEBPS/shared.png".to_string(),
                    data: b"SHARED".to_vec(),
                }]
            );
        }

        #[test]
        fn when_referenced_image_is_missing_then_skips_it() {
            let chapter =
                br#"<img src="missing.png" alt="Gone"/><img src="present.png" alt="Here"/>"#;
            let epub = ZipBuilder::new()
                .add("META-INF/container.xml", CONTAINER)
                .add("OEBPS/content.opf", SINGLE_CHAPTER_OPF)
                .add("OEBPS/chapter1.xhtml", chapter)
                .add("OEBPS/present.png", b"HERE")
                .build();

            let images = extract_images(&epub).unwrap();

            assert_eq!(
                images,
                vec![ExtractedImage {
                    path: "OEBPS/present.png".to_string(),
                    data: b"HERE".to_vec(),
                }]
            );
        }

        #[test]
        fn when_no_images_referenced_then_returns_empty() {
            let epub = ZipBuilder::new()
                .add("META-INF/container.xml", CONTAINER)
                .add("OEBPS/content.opf", SINGLE_CHAPTER_OPF)
                .add("OEBPS/chapter1.xhtml", b"<h1>No images</h1>")
                .add("OEBPS/unused.png", b"UNUSED")
                .build();

            assert_eq!(extract_images(&epub).unwrap(), vec![]);
        }

        #[test]
        fn when_spine_file_is_missing_then_returns_zip_error() {
            let opf = br#"<package>
  <manifest><item id="ch1" href="missing.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#;
            let epub = ZipBuilder::new()
                .add("META-INF/container.xml", CONTAINER)
                .add("OEBPS/content.opf", opf)
                .build();

            assert_eq!(
                extract_images(&epub),
                Err(EpubError::ZipError(crate::zip::ZipError::EntryNotFound))
            );
        }
    }
}
