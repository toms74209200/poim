# Design Document: poim — A PDF / EPUB → Markdown Conversion Engine

poim is an engine that converts PDF / EPUB into Markdown. The core is implemented exactly once, and both the CLI and the browser call into the same code. In the browser, files are converted entirely on the client and never sent anywhere. The conversion logic is a single implementation; the frontends (CLI / Web) are thin wrappers around it, and there can be several of them.

> The name is **poim** — a blend of *portable* + *compendium*. It captures the goal in the name itself: condensing heavy source material (PDF / EPUB) into easy-to-handle plain text you can carry anywhere.

## Context

Markdown has become the de facto intermediate format for feeding LLMs, managing documentation, and generating static sites. Primary sources, however, are usually distributed as PDF or EPUB, so bridging the two is a routine need.

None of the existing conversion options run entirely in the browser. pandoc and native PyMuPDF assume local execution, while web-service converters upload files to a server. The former carries the burden of distribution and installation; the latter means handing confidential documents to a third party. There is no option that runs entirely on the client and can also be reused from the CLI with the same engine.

## Motivation

Existing document-conversion options force the user into one of two choices: "install a native tool" or "send the file to a server." The former involves the overhead of setting up an environment; the latter means handing confidential documents to a third party. There is no option that runs entirely on the client and can also reuse the same engine from the CLI.

poim removes this dichotomy. The engine is distributed as wasm, and the browser converts files locally. The same engine is also embedded into a CLI binary for batch processing and CI. There is a single promise to the user — your input never leaves your hands.

## Goals

- Provide a single engine that converts PDF / EPUB into Markdown
- Run identical core code in both the CLI and the browser
- Have the browser version complete entirely on the client, with no external file transmission
- Keep external crate dependencies at zero and the wasm binary small
- Serialize multi-column PDFs in the correct reading order

## Non-Goals

- **Reverse conversion**: Markdown → PDF / EPUB is out of scope. Conversion is one-directional only.
- **OCR**: Scanned PDFs without a text layer are not handled. Recognizing characters in images is a separate problem and is incompatible with the zero-dependency policy.
- **Reproducing multi-column layout**: Markdown has no concept of multiple columns. PDFs with two or more columns will not have their layout reproduced; only the reading order is serialized (§Solution).
- **Encrypted PDFs**: Removing password protection or DRM is out of scope for the MVP.

## Solution

### Core abstraction: an intermediate block representation

PDF and EPUB have completely different file structures, but the information needed to render them as Markdown is shared — a sequence of blocks: headings, paragraphs, lists, links, emphasis, tables, and images. Each parser is responsible only up to producing this intermediate representation (IR), and there is exactly one emitter that assembles Markdown from the IR.

```
PDF  ─┐
      ├→ [parser] → IR (sequence of blocks) → [emitter] → Markdown
EPUB ─┘                                            └→ image list
```

This way, adding an input format means adding just one parser, and the Markdown-formatting logic is never duplicated. This IR is the system's unit of identity.

### Interface

The boundary with wasm is limited to passing length-prefixed byte slices. The wasm module exports only linear-memory allocation/deallocation and the conversion itself.

```
// wasm ABI (raw extern "C")
alloc(len: usize) -> ptr
free(ptr, len)
convert(ptr, len, format: u32) -> packed_result_ptr
// packed_result = [md_len][md_bytes][image_count][images...]
```

On top of this low-level ABI sits a thin wrapper for each environment. The command name is `poim`.

```
cli:  poim input.pdf -o out.md       # single file
      poim ./books/ -o ./md/         # batch over a directory
      poim in.pdf --images imgs/     # write out images
web:  file picker / drag-and-drop → convert → preview → download
```

Both the CLI and the browser call the same `convert`. The differences are confined to input/output handling (file system / Blob).

### Column serialization

Naively extracting a two-column document interleaves the left and right lines. poim estimates the column boundary from the x-coordinate distribution of text blocks, then reorders column by column from top to bottom, normalizing the reading order into "all of the left column → all of the right column." This is normalization of order, not reproduction of layout.

### Dependency policy

No external crates are used. This is both a constraint and the central design principle. Unpacking ZIP (EPUB) and FlateDecode (PDF streams) are both DEFLATE, so a single inflate implementation is written in-house and shared by both. The XML/XHTML parser, the PDF object parser, and the argument parser are also implemented in-house using only the standard library. The standard library (`std`) is used, but third-party crate dependencies are zero.

## Alternative Solutions

### Depend on existing libraries (serde_json / quick-xml / lopdf, etc.)

**Pros**: Faster to implement. The parsers are mature and have accumulated handling for edge cases.  
**Cons**: The wasm binary bloats. Each crate pulls in transitive dependencies, and control over the whole tree is lost. PDF-oriented crates in particular often assume a native environment and don't compile cleanly for wasm32.  
**Decision**: Implement in-house. Share inflate, and implement parsers only to the extent needed for Markdown conversion. Prioritize size and control over the convenience of dependencies.

### Use wasm-bindgen

**Pros**: A typed JS boundary can be generated automatically, making it easy to pass strings and objects.  
**Cons**: It contradicts the zero-dependency policy. The generated glue and its dependencies inflate the wasm and make the ABI opaque. All the boundary requires is a round trip of byte slices, for which this is overkill.  
**Decision**: Hand-write a raw `extern "C"` ABI. Export only alloc/free and convert, and keep the JS-side loader to a few dozen hand-written lines.

### Go `no_std`

**Pros**: Dependencies can be cut even further, yielding the smallest binary in theory.  
**Cons**: Since the work is string-processing-heavy, `alloc` is mandatory, and the reduction doesn't justify the complexity of `no_std + alloc`. On the wasm32 target, using `std` compiles without any problems.  
**Decision**: Use `std`. The goal of zero crate dependencies is compatible with using `std`.

### Write it in C or Zig

**Pros**: Smaller wasm output and a lighter toolchain.  
**Cons**: A parser handles large amounts of unvalidated input, an area where the cost of memory safety bites directly. Rust's `enum`/`match` can enforce exhaustiveness of block kinds and token kinds at compile time, preventing missed branches in the parser.  
**Decision**: Rust. The `wasm32-unknown-unknown` target is mature and can emit small wasm while staying dependency-free.

## Implementation Conventions

### Memory ABI

At the JS/wasm boundary, the caller writes input into a region it obtained via `alloc` and passes the pointer and length to `convert`. The return value is a single buffer packed with length prefixes, which the caller parses and then frees. All strings are UTF-8. The ownership-transfer rule is fixed in one direction, eliminating double-frees by design.

### Sharing inflate

DEFLATE decompression is needed both for EPUB's ZIP entries and PDF's FlateDecode streams. inflate is implemented as a single module and called from both parsers. Compression (deflate) is unnecessary and is not implemented.

### Argument parsing and file I/O

The CLI is built from only `std::env::args` and `std::fs`, with no argument-parser crate brought in. It is kept to a simple set of flags with no subcommands, confining functionality to a range where in-house parsing suffices.

## Concerns

**The effort of implementing PDF with zero dependencies.** This is the biggest risk. PDF requires parsing the xref table, object streams, and content-stream operators; writing it from scratch is a far greater burden than EPUB. The plan secures an escape route by completing EPUB first to settle the IR and emitter, then expanding PDF coverage incrementally.

**Font encoding.** PDF text sometimes resolves to Unicode only by going through a CID font or a ToUnicode CMap. Ignoring this produces garbled extraction. For the MVP, the target is PDFs with standard encodings and a ToUnicode map, with any unresolvable cases reported as explicit gaps.

**Correctness of the inflate implementation.** A bug in the in-house inflate breaks both EPUB and PDF at once. A regression suite is established against an RFC 1951–conformant test corpus, and this part alone is verified exhaustively.

**wasm size.** Size was one of the main reasons for choosing zero dependencies. The wasm byte size is measured on every build, and any increase is monitored.
