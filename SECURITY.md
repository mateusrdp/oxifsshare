# Security policy

## Supported versions

Only the latest release receives fixes.

## Reporting a vulnerability

Use GitHub's **private vulnerability reporting** (Security tab > Report a vulnerability).
Do not open a public issue for a vulnerability.

**Never attach real archive data**, file names, or document contents to a report.
Run `oxifsshare --check ARCHIVE` and share only its output; it contains counts and
column names, never values.

## Scope and design

- The app is offline: it makes no network connections and has no telemetry.
- It never renders or executes documents. It passes a validated path to the
  operating system's default application (`xdg-open` on Linux).
- Before opening, the path must be a regular file whose canonical location is
  inside the archive folder and whose extension is on an allow-list; other
  extensions need explicit confirmation. Symbolic links are not indexed.
- Malformed or hostile index files must produce an error, never a panic.
  Reports of crashes, hangs or memory blow-ups on crafted input are welcome.

This is not legal advice.
