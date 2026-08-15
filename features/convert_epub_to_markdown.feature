Feature: Convert EPUB to Markdown

  This is the specification every frontend must satisfy. Each is built for its
  own target and verified against this same feature, so satisfying it is what
  makes the CLI and the browser agree.

  Scenario: A reference EPUB becomes Markdown
    Given the reference EPUB
    When converting it
    Then the Markdown contains "# Hefty Water"
    And the Markdown has no line broken by source indentation

  Scenario: A chapter heading and paragraph survive the conversion
    Given an EPUB whose chapter is:
      """
      <h1>Chapter One</h1><p>Hello world.</p>
      """
    When converting it
    Then the Markdown contains "# Chapter One"
    And the Markdown contains "Hello world."

  Scenario: A referenced image is extracted alongside the Markdown
    Given an EPUB whose chapter is:
      """
      <p>See <img src="fig.png" alt="Figure"/></p>
      """
    And the EPUB contains "OEBPS/fig.png"
    When converting it
    Then "OEBPS/fig.png" is extracted
    And the Markdown references the extracted image

  Scenario: A file that is not an EPUB is reported as an error
    Given input that is not an EPUB
    When converting it
    Then an error is reported
