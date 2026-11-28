# Additional code fence grammars

The Markdown preview uses one syntax set built from the unmodified Syntect 5.3.0 default newline pack and the five grammar inputs under `crates/markdown-core/assets/code-syntaxes`. The generated pack carries the default grammar notices alongside the following notices.

| Grammar | Input | License |
| --- | --- | --- |
| TypeScript and TSX | [Bat TypeScript conversion](https://github.com/sharkdp/bat/blob/dbc11b6924729c9cdfe9a2555daa1cec2baa90fa/assets/syntaxes/02_Extra/TypeScript.sublime-syntax), [Bat TypeScriptReact conversion](https://github.com/sharkdp/bat/blob/dbc11b6924729c9cdfe9a2555daa1cec2baa90fa/assets/syntaxes/02_Extra/TypsecriptReact.sublime-syntax), and [Microsoft TypeScript Sublime Plugin](https://github.com/microsoft/TypeScript-Sublime-Plugin/tree/ba45efd058df5111837e30fb9598cfc8cbd51095) | [Apache 2.0](TypeScript-LICENSE.txt) |
| PowerShell | [Bat PowerShell conversion](https://github.com/sharkdp/bat/blob/dbc11b6924729c9cdfe9a2555daa1cec2baa90fa/assets/syntaxes/02_Extra/PowerShell.sublime-syntax) and [Microsoft EditorSyntax](https://github.com/PowerShell/EditorSyntax/tree/a08b55bf1146c210f58e844be53c2aa78fd5e610) | [MIT](PowerShell-LICENSE.txt) |
| TOML | [Sublime TOML Highlighting](https://github.com/jasonwilliams/sublime_toml_highlighting/tree/f5a57e8bff694a4e6c52a491dae579aabc7427cf) | [MIT](TOML-LICENSE.txt) |
| Dockerfile | [Docker TextMate bundle](https://github.com/asbjornenge/Docker.tmbundle/tree/0f6b7bc87acf684f7b0790fd480731ffb4615b87) | [MIT](Dockerfile-LICENSE.txt) |

Notepads replaces one PowerShell regular-expression subexpression call with its equivalent ASCII hexadecimal class during generation. The original grammar files and their licenses are unchanged. The generated pack, the five input grammars, and their license files are recorded in `syntax-assets.json`.

From the Markdown module root, run `cargo run --locked --release --manifest-path scripts/code-syntax-pack/Cargo.toml` to reproduce the pack. The optional argument selects a different output file.

