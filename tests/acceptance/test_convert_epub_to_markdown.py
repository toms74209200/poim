import zipfile
from io import BytesIO

from pytest_bdd import given, parsers, scenarios, then, when

scenarios("../../features/convert_epub_to_markdown.feature")

CONTAINER = b"""<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"""

OPF = b"""<?xml version="1.0"?>
<package version="3.0">
  <manifest><item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"""


def _build_epub(chapter: str, extra: dict[str, bytes] | None = None) -> bytes:
    buffer = BytesIO()
    with zipfile.ZipFile(buffer, "w", zipfile.ZIP_DEFLATED) as archive:
        archive.writestr("mimetype", "application/epub+zip", zipfile.ZIP_STORED)
        archive.writestr("META-INF/container.xml", CONTAINER)
        archive.writestr("OEBPS/content.opf", OPF)
        archive.writestr("OEBPS/chapter1.xhtml", chapter)
        for name, content in (extra or {}).items():
            archive.writestr(name, content)
    return buffer.getvalue()


@given("the reference EPUB")
def given_reference_epub(epub, reference_epub_path):
    epub["bytes"] = reference_epub_path.read_bytes()


@given("an EPUB whose chapter is:")
def given_epub_with_chapter(epub, docstring):
    epub["chapter"] = docstring
    epub["bytes"] = _build_epub(docstring)


@given(parsers.parse('the EPUB contains "{name}"'))
def given_epub_contains(epub, name):
    epub["bytes"] = _build_epub(epub["chapter"], {name: b"PNGDATA"})


@given("input that is not an EPUB")
def given_not_an_epub(epub):
    epub["bytes"] = b"this is not an epub at all"


@when("converting it")
def when_converting(frontend, epub, result):
    result["conversion"] = frontend.convert(epub["bytes"])


@then(parsers.parse('the Markdown contains "{text}"'))
def then_markdown_contains(result, text):
    markdown = result["conversion"].markdown
    assert text in markdown, f"{text!r} not in:\n{markdown}"


@then("the Markdown has no line broken by source indentation")
def then_no_source_indentation(result):
    for line in result["conversion"].markdown.splitlines():
        assert not line.startswith("    "), f"indentation leaked into output: {line!r}"


@then(parsers.parse('"{name}" is extracted'))
def then_extracted(result, name):
    images = result["conversion"].images
    assert name in images, f"extracted {sorted(images)}, expected {name}"


@then("the Markdown references the extracted image")
def then_markdown_references_image(result):
    conversion = result["conversion"]
    for name in conversion.images:
        assert name in conversion.markdown, (
            f"{name!r} not referenced in:\n{conversion.markdown}"
        )


@then("an error is reported")
def then_error_reported(result):
    conversion = result["conversion"]
    assert conversion.error, f"expected an error, got: {conversion.markdown!r}"
