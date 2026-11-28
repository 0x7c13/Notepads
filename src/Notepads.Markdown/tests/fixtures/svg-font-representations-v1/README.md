# Fixed font representation inputs

Copyright (c) 2026, Jiaqi (0x7c13) Liu. MIT; see the project root LICENSE.

These original fonts, glyphs, PNG pixels and SVG documents are fixed test inputs,
not renderer-generated goldens. Their glyph, metric, pixel and numerical expectations remain fixed.
INPUTS.json binds the 18 payload files. Its supportFiles list binds current authored
metadata; archivedSupportFiles retains the recorded hashes of retired generation tools.
The original import metadata is preserved in the verified source archive.

COLR fonts map E100..E111 to gid2..19: linear/radial/sweep at 31/32/33/255/256/257
stops. The variable font has an actual nonzero wght variation store. PNG fonts
map E200 to gid2; font SVG fonts map E300 to gid2. PNG data is valid original 2x2
RGBA. Plain and gzip SVG encode the same valid glyph2 document.

The four complete COLR SVG sources include all eighteen glyphs. The four separately
named basic sources contain the first twelve linear/radial glyphs. CASES-V1.json
retains eight successful and four Unsupported expectations. No renderer output was regenerated. The 2026-11-15 namespace migration changed
only font name records, required checksums and matching SVG family attributes.
Original imported identities remain recorded alongside current input hashes.

Current validation enters through scripts/Test-Rust.ps1 -Layer Svg or -Layer All.
The fork usvg representation tests are also exercised by the full workspace suite.
