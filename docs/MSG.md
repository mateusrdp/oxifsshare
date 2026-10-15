# Outlook `.msg` support

oxifsshare converts an Outlook message (`.msg`) to a standard `.eml` file when
you open it, and hands the `.eml` to your default mail application. The `.msg`
itself is never modified. "Open original .msg" skips the conversion.

Everything is pure Rust (`cfb` for the compound file, our own MIME writer). No
Outlook, no Python, no network.

## What you get

Headers (`From`, `To`, `Cc`, `Bcc`, `Subject`, `Date`, `Message-ID`, `In-Reply-To`,
`References`), plain-text and HTML bodies (as `multipart/alternative`), inline
images (`cid:` references kept, in `multipart/related`), file attachments, and
embedded messages (as `message/rfc822`). All text is encoded so the result is
7-bit ASCII with CRLF line endings; non-ASCII text uses UTF-8.

## Where the `.eml` goes

A folder created for the running program in the system temp directory
(`oxifsshare-eml-<pid>-<n>`): mode 0700 on Unix, files 0600, removed when the
program exits. If the program was killed, leftovers (our prefix, real
directories only, older than 24 hours) are removed at the next conversion.
Converted messages contain document content, so nothing is written next to the
archive and nothing is logged.

## Known gaps (v1 of the converter)

- **RTF-only bodies.** Some messages store the body only as compressed RTF.
  These get a short notice text instead of the body (counted as `RTF-only` in
  `--check`); use "Open original .msg". Decoding compressed RTF, and
  extracting HTML from RTF-encapsulated HTML, are possible later work.
- **Named properties** are ignored (categories, custom headers kept only if the
  original transport headers are present).
- **Attachments by reference and OLE objects** are skipped and counted.
- **Recipients with only an internal (Exchange) address** get the placeholder
  `unknown@invalid` (reserved by RFC 6761) unless an SMTP address is found in
  the recipient, in the original headers, or from an SMTP-typed address.
- **Code pages.** 8-bit strings use the message code page (property 3FFD), else
  the code page of the message locale (3FF1), else the internet code page, else
  windows-1252. Single-byte Windows pages are labelled with their charset;
  windows-1252, ISO-8859-1, US-ASCII and UTF-8 are decoded to UTF-8, other
  pages are passed through labelled with the charset.
- **Attachment file names** with non-ASCII characters carry both a plain
  fallback and an RFC 2231 form. Python's `email` package prefers the fallback
  (`_` in place of the character); Thunderbird and most mail clients use the
  extended form.
- The `cfb` crate is stricter than some other readers. Of three public fuzzing
  samples, one was converted and two were rejected as "not a compound file";
  neither case panics.

## Limits (safety)

File 512 MiB, any stream 256 MiB, output 1 GiB, 5000 children per storage,
100 000 properties, embedded messages nested at most 4 deep. Exceeding a limit is
an error (`too-large`), not a crash.

## How it was verified

- Unit tests for the MIME helpers and integration tests on **synthetic**
  messages built by `oxifsshare_core::testmsg` (header injection, Unicode,
  8-bit text, inline images, embedded messages, missing SMTP, RTF-only,
  truncated and randomly corrupted files, no panics).
- Against **public sample files** (from the msg-extractor and Apache POI
  projects; kept outside the repository and never committed), comparing subject,
  body, HTML, recipients, attachments and date with the independent `extract-msg`
  library and parsing the result with Python's `email` package. All differences
  were explained: oracle mistakes (it ignores the message locale for some
  code pages), the expected `.eml`/`.msg` name of embedded messages, and the
  RTF gap above. The Python tooling is a development aid only; the repository
  contains no Python.
- Synthetic messages from the generator were also read by the independent
  library, which confirms the generator follows the file layout.

**Not verified:** behaviour of real Outlook files from your archive (none were
available), and how any particular mail client displays the result.
`oxifsshare --check` has a `[Messages]` section that tries the conversion on
every `.msg` and prints only counts and failure categories, so you can report
problems without sharing content.

## File layout used (for contributors)

`__substg1.0_<tag4hex><type4hex>` streams: `001F` UTF-16LE text, `001E` 8-bit
text, `0102` binary, `000D` embedded storage. `__properties_version1.0` has
16-byte entries after a header of 32 bytes (message), 24 (embedded message) or 8
(recipient, attachment). Recipients and attachments are the storages
`__recip_version1.0_#NNNNNNNN` and `__attach_version1.0_#NNNNNNNN`.
