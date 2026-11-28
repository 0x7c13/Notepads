# Fixed original color-control inputs

Copyright (c) 2026, Jiaqi (0x7c13) Liu. MIT; see LICENSE.

These original encoded fonts are fixed test inputs, not renderer-generated expected
outputs. Glyph, metric and color data and independent expectations remain unchanged.
Only name records and required checksums changed during the namespace migration;
manifest.json records current hashes and table offsets plus original identities.
manifest.json records current module-relative font locations and the SHA of the
original import record preserved in the verified external source archive. Historical
generation tools are not current in-tree validation entries.

The request/representation tests in this fork consume these retained inputs. Run
the module scripts/Test-Rust.ps1 -Layer All for the current workspace regression.
