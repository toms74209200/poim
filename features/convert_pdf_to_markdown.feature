Feature: Convert PDF to Markdown

  This is the specification every frontend must satisfy for PDF input. A PDF
  carries positioned glyph codes rather than text, so satisfying it is what
  makes those codes come back as readable Markdown.

  Scenario: Text shown on a page becomes a paragraph
    Given a PDF whose page shows "Hello world."
    When converting it
    Then the Markdown contains "Hello world."

  Scenario: A larger line becomes a heading
    Given a PDF titled "The Title" above its body text
    When converting it
    Then the Markdown contains "# The Title"

  Scenario: Japanese shown through a CID font without a ToUnicode CMap is recovered
    Given a PDF whose page shows Japanese through an Adobe-Japan1 font
    When converting it
    Then the Markdown contains "本書"

  Scenario: A file that is not a PDF is reported as an error
    Given input that is not a PDF
    When converting it
    Then an error is reported
