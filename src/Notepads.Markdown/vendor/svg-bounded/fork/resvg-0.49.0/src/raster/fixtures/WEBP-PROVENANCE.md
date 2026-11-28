# Fixed WebP renderer fixtures

Original image-webp sources and fixtures are pinned to commit
`08b57d473c90148dd4b37a075df365afc3dfcc2b`, version 0.2.4.
The original MIT and Apache licenses are preserved as `WEBP-LICENSE-*`.

- `upstream-red-{2x2,3x3}.webp`: exact 68-byte literals from the corresponding
  `src/decoder.rs` unit tests; original source credits ImageMagick generation.
- `upstream-color-index-30x30.webp`: original `tests/images/regression/color_index.webp`.
- `upstream-random-lossy.webp`: original `tests/images/animated/random_lossy.webp`;
  SHA256 `23A17DE1E9EC907F2D503E8EDA4036FCDB87FF55AC7F2A7468CF05DFBDB6D7F6`.
- `upstream-color-index-30x30-premultiplied.rgba`: exact 3600-byte slice of the
  frozen old decoder observation for the color-index fixture, measured before
  controlled decoding was implemented. SHA256
  `24737F3482FC4A7696EDFD3DC553DA0D8D02A1DF0E23F8D659128349D16F5F27`.

The upstream repository declares MIT OR Apache-2.0; no separate terms were found
for these test inputs. Original copyright and terms remain with their authors.
The dimension-limit test only changes header dimensions in memory; it supplies
no expected pixels or claim that the unchanged entropy describes a larger image.
