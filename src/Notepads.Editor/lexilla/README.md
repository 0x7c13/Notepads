# Maintained Lexilla subset

Pinned upstream: [Lexilla 5.5.4](https://github.com/ScintillaOrg/lexilla/tree/rel-5-5-4),
commit `8fe5438b65cffc34340ecd60357ce5a6f72eecbe`.

The editor statically compiles 13 selected lexer sources (14 modules, including
HTML and XML) and their shared lexlib sources. There is no DLL loader, catalogue,
download step or external plugin API. `include/SciLexer.h` and
`include/LexicalStyles.iface` belong to the same pinned version. The existing
Scintilla ILexer5 interface owns each lexer through its document.

`upstream-files.json` records original upstream file SHA-256 checksums, including
the license. Files retain upstream formatting and license notices. Modified
files also identify the Notepads contributions with a 2026 copyright notice.
The following local change intentionally differs from the recorded upstream checksum:

- `lexers/LexMarkdown.cxx`, `HasPrevLineContent`: recognize both CR and LF when
  scanning the preceding line, and skip the CR half of CRLF. Notepads uses CR
  internally; the upstream LF-only loop crossed blank CR lines, misclassified
  horizontal rules as setext headings and could scan to the document beginning.
  Native integration tests compare CR, LF and CRLF styling and incremental edits.

Notepads-specific lexer admission and view scheduling live in the owned
WinUIEditor/Scintilla WinUI surface, keeping upstream lexer changes minimal.
