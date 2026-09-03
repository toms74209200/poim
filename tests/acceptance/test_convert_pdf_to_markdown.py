from pytest_bdd import given, parsers, scenarios, then, when

from lib.frontends import FORMAT_PDF

scenarios("../../features/convert_pdf_to_markdown.feature")

BODY = "Body text runs longer than the title does."
JAPANESE_CIDS = [0x0E8A, 0x097B]

HELVETICA = (
    b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
)
JAPANESE = (
    b"<< /Type /Font /Subtype /Type0 /BaseFont /Kaku /Encoding /Identity-H"
    b" /DescendantFonts [6 0 R] >>"
)
DESCENDANT = (
    b"<< /Type /Font /Subtype /CIDFontType0 /BaseFont /Kaku /CIDSystemInfo"
    b" << /Registry (Adobe) /Ordering (Japan1) /Supplement 6 >> >>"
)


def _build_pdf(
    content: str, font: bytes = HELVETICA, descendant: bytes | None = None
) -> bytes:
    page = content.encode()
    objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        (
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 612 792]"
            b" /Resources << /Font << /F1 5 0 R >> >> >>"
        ),
        b"<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>",
        b"<< /Length %d >>\nstream\n%s\nendstream" % (len(page), page),
        font,
    ]
    if descendant is not None:
        objects.append(descendant)

    body = b"%PDF-1.7\n"
    entries = [b"0000000000 65535 f \n"]
    for number, obj in enumerate(objects, start=1):
        entries.append(b"%010d 00000 n \n" % len(body))
        body += b"%d 0 obj\n%s\nendobj\n" % (number, obj)

    size = len(objects) + 1
    table = b"xref\n0 %d\n%s" % (size, b"".join(entries))
    trailer = b"trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (
        size,
        len(body),
    )
    return body + table + trailer


@given(parsers.parse('a PDF whose page shows "{text}"'))
def given_pdf_showing(pdf, text):
    pdf["bytes"] = _build_pdf(f"BT /F1 12 Tf 50 700 Td ({text}) Tj ET")


@given(parsers.parse('a PDF titled "{title}" above its body text'))
def given_pdf_with_title(pdf, title):
    pdf["bytes"] = _build_pdf(
        f"BT /F1 24 Tf 50 700 Td ({title}) Tj ET\nBT /F1 12 Tf 50 600 Td ({BODY}) Tj ET"
    )


@given("a PDF whose page shows Japanese through an Adobe-Japan1 font")
def given_pdf_with_japanese(pdf):
    codes = "".join(f"{cid:04X}" for cid in JAPANESE_CIDS)
    pdf["bytes"] = _build_pdf(
        f"BT /F1 12 Tf 50 700 Td <{codes}> Tj ET", JAPANESE, DESCENDANT
    )


@given("input that is not a PDF")
def given_not_a_pdf(pdf):
    pdf["bytes"] = b"this is not a pdf at all"


@when("converting it")
def when_converting(frontend, pdf, result):
    result["conversion"] = frontend.convert(pdf["bytes"], FORMAT_PDF)


@then(parsers.parse('the Markdown contains "{text}"'))
def then_markdown_contains(result, text):
    markdown = result["conversion"].markdown
    assert text in markdown, f"{text!r} not in:\n{markdown}"


@then("an error is reported")
def then_error_reported(result):
    conversion = result["conversion"]
    assert conversion.error, f"expected an error, got: {conversion.markdown!r}"
