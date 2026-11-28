# JPEG renderer test fixture

`upstream-progressive-dri-420-16x16.jpg` is the unchanged 479-byte byte-array fixture from
zune-jpeg `src/mcu_prog.rs` at upstream commit
`31d81fed7551c8ccea456d9d8e2b1fd8bebb6995`.
SHA256: `DB57EE4239F9E6330421D60E6FD07C925EA06EEBC48AF0E3796951E7B0BCC5BC`.

Original copyright belongs to the zune-jpeg authors; the upstream MIT license is
preserved in `ZUNE-LICENSE-MIT`. This fixture was fixed before the controlled decoder
changes. Existing prefix/raw expectations are kept in the local validation archive.
The dimension-limit test changes only the SOF height/width in memory; that derivative
tests rejection before entropy decoding and supplies no expected pixels.
