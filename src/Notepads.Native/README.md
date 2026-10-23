# Notepads native editor

This directory is maintained as part of Notepads. It is based on
[WinUIEdit](https://github.com/BreeceW/WinUIEdit), imported from commit
`d9f60dbca65c2af54ced590d873d9438de32ce77`, with Scintilla 5.6.3.

Only the UWP editor, compiled Scintilla backend, interface generator and
required interface inputs are retained. No submodule or patch step is used.
Handwritten API extensions, streaming, journaling and diff comparison live in
`WinUIEditor/`; sparse display mapping and rendering live in `scintilla/`.
Keep upstream licenses and formatting when maintaining these sources.
Unmodified upstream files keep their original notices. Modified files retain
those notices and add Notepads' 2026 contribution attribution. When WinUIEdit
has no per-file notice, its Breece Walker copyright and license are cited
explicitly. New Notepads source uses its own creation year. The interface
generator emits the same attribution for the generated wrappers.

Upstream licenses are preserved in `LICENSE`, `scintilla/License.txt` and
`lexilla/License.txt`. A pinned Lexilla 5.5.4 subset is statically linked for
syntax highlighting; its interface file also feeds the generator. See the
[retained lexer inventory and local change](lexilla/README.md).

Restore packages with `scripts/Initialize-Editor.ps1` from the repository
root. Build `src/Notepads.sln`. Regenerate the WinRT interface with
`Tools/Interface.bat`; extension fragments are handwritten and retained by
the generator.
