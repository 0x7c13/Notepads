# Fixed independent range reference

This retained reference contains 15 source/model controls, 70 manually authored
range requests and 17 strict-decoding negative controls. Source bytes, semantic
records, expected contributions, relation coordinates, unions, anchors and error
expectations remain unchanged. These are independent reference inputs; their
historical oracle approval does not qualify the current parser or product UI.

inputs/ preserves CR/LF/CRLF, NUL and EOF bytes. models/ contains complete fixture
semantic records. expected/range-goldens.json uses UTF8 source byte coordinates
and flow-local UTF16 logical coordinates. Keep original notices and source identity.

The historical checker and its scratch-contract/parent-receipt assumptions have
been archived. manifest.json retains that checker's original identity separately
from current files. The previous import record remains in the verified source
snapshot; its unavailable scratch paths are historical locators, not runnable entries.

Current implementation validation uses scripts/Test-Rust.ps1 -Layer Core or -Layer
All. Do not regenerate expected results from implementation output.
