# Fixed GIF renderer fixtures

`upstream-2x2.gif` is the original image-gif 0.14.2 sample from commit
`a0127e2acbef333398414bb61d097c19c1b09d8a` at `tests/samples/2x2.gif`.
SHA256: `F3CA4E1221C1938B71DE31D64454D29131C6147F779E10F2EDACDC8FC5F57D49`.
Original MIT and Apache licenses are preserved as `GIF-LICENSE-*`.
The repository declares MIT OR Apache-2.0; no separate fixture terms were found.

The other four inputs are fixed derivatives of that sample, measured with the
original parser and first-frame decoder before controlled renderer integration:

- `gif-d02-no-trailer.gif`: remove the final trailer at offset 43.
- `gif-d02-netscape-after-first.gif`: replace the trailer with a valid NETSCAPE2.0
  loop extension followed by the trailer.
- `gif-d02-second-metadata-no-payload.gif`: replace the trailer with the complete
  second image descriptor/min-code header and a declared but missing payload.
- `gif-d02-invalid-first-lzw.gif`: replace compressed bytes 39..42 with FF FF FF.

Tests also change the logical screen, frame offset, transparency flag or frame
dimensions explicitly in memory. The original LZW data is retained. The oversized
dimension input has no expected pixels. The offset/transparency assertion follows
the frozen original first-frame pixels and the existing binary-alpha semantics;
it does not introduce logical-screen compositing.
