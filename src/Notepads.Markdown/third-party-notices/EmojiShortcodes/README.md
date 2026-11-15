# Emoji shortcode data

The product uses [emojis 0.9.0](https://docs.rs/crate/emojis/0.9.0) for Unicode emoji lookup. The crate declares `(MIT OR Apache-2.0) AND Unicode-3.0` and derives its shortcode aliases from [gemoji 4.1.0](https://github.com/github/gemoji/tree/v4.1.0). The [Unicode 17 emoji data](https://unicode.org/emoji/charts-17.0/emoji-released.html) determines the returned text. The preview uses the installed Windows font for drawing.

The crates.io archive SHA-256 is `0B1514CED566C94991ED9563258ECF61E311FD773BF0A3F20606830386BF66E3`. The published crate records source commit [7367564e64297773e802df60b69280a76b9dee43](https://github.com/rossmacarthur/emojis/tree/7367564e64297773e802df60b69280a76b9dee43). Cargo locks the crate for Comrak's `shortcodes` feature and the Core replay verifier. The crate uses `phf 0.13.1`, whose notices are in the MarkdownCore inventory.

| Notice | SHA-256 |
| --- | --- |
| [MIT](LICENSE-MIT) | `F9C4C77BAA3828004EE54B8A4F2DB2E88ED44A6237A493965BF551FAC0FCB62D` |
| [Apache 2.0](LICENSE-APACHE) | `C71D239DF91726FC519C6EB72D318EC65820627232B2F796219E87DCF35D0AB4` |
| [Unicode](LICENSE-UNICODE) | `F7DB81051789B729FEA528A63EC4C938FDCB93D9D61D97DC8CC2E9DF6D47F2A1` |
| [gemoji](LICENSE-GEMOJI) | `7EA1D1FD0602E6623B9A65E791499A0EA5A2F5F60F91BFAD78CBAEEE19DA26E6` |
