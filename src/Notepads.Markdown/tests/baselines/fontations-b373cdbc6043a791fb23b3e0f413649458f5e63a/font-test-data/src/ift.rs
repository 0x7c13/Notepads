//! Test data for the IFT table
//!
//! Used for incremental font transfer. Specification:
//! <https://w3c.github.io/IFT/Overview.html>

use std::iter;

use font_types::{Int24, Tag, Uint24};

use crate::{be_buffer, bebuffer::BeBuffer};

pub static IFT_BASE: &[u8] = include_bytes!("../test_data/ttf/ift_base.ttf");

pub static CFF_FONT: &[u8] = include_bytes!("../test_data/ttf/NotoSansJP-Regular.subset.otf");
pub static CFF2_FONT: &[u8] = include_bytes!("../test_data/ttf/NotoSansJP-VF.subset.otf");

pub const CFF_FONT_CHARSTRINGS_OFFSET: u32 = 0x1b9;
pub const CFF2_FONT_CHARSTRINGS_OFFSET: u32 = 0x8f;

// Using opcode format: https://w3c.github.io/IFT/Overview.html#url-templates
pub const RELATIVE_URL_TEMPLATE: &[u8] = b"\x04foo/\x80";
pub const ABSOLUTE_URL_TEMPLATE: &[u8] = b"\x0a//foo.bar/\x80";

// Format specification: https://w3c.github.io/IFT/Overview.html#patch-map-format-1
pub fn simple_format1() -> BeBuffer {
    let mut buffer = be_buffer! {
        /* ### Header ### */
        {1u8: "format"},        // format
        0u32,                   // reserved
        [1u32, 2, 3, 4],        // compat id
        2u16,                   // max entry id
        {2u16: "max_glyph_map_entry_id"},
        (Uint24::new(7)),       // glyph count
        {0u32: "glyph_map_offset"},
        0u32,                   // feature map offset
        0b00000010u8,           // applied entry bitmap (entry 1)

        6u16,                   // url template length
        4u8,
        {b'f': "url_template[1]"},
        {b'o': "url_template[2]"},
        [b'o', b'/', 128u8], // url_template[3..6]

        {3u8: "patch_format"}, // = glyph keyed

        /* ### Glyph Map ### */
        {1u16: "glyph_map"},     // first mapped glyph
        {2u8: "entry_index[1]"},
        [1u8, 0, 1, 0, 0]        // entry index[2..6]
    };

    let offset = buffer.offset_for("glyph_map") as u32;
    buffer.write_at("glyph_map_offset", offset);

    buffer
}

pub fn format1_with_dup_urls() -> BeBuffer {
    let mut buffer = be_buffer! {
        /* ### Header ### */
        1u8,                    // format
        0u32,                   // reserved
        [1u32, 2, 3, 4],        // compat id
        4u16,                   // max entry id
        {4u16: "max_glyph_map_entry_id"},
        (Uint24::new(7)),       // glyph count
        {0u32: "glyph_map_offset"},
        0u32,                   // feature map offset
        0b00000010u8,           // applied entry bitmap (entry 1)

        9u16,                   // url template length
        [8u8, b'f', b'o', b'o', b'/', b'b', b'a', b'a', b'r'], // url_template[9]

        {3u8: "patch_format"}, // = glyph keyed

        /* ### Glyph Map ### */
        {1u16: "glyph_map"},     // first mapped glyph
        {2u8: "entry_index[1]"},
        [3u8, 4, 0, 0, 0]        // entry index[2..6]
    };

    let offset = buffer.offset_for("glyph_map") as u32;
    buffer.write_at("glyph_map_offset", offset);

    buffer
}

pub fn simple_format1_with_one_charstrings_offset() -> BeBuffer {
    let mut buffer = be_buffer! {
        /* ### Header ### */
        1u8,                    // format
        0u8, 0u8, 0u8,          // reserved
        0b00000001u8,           // has charstrings offset
        [1u32, 2, 3, 4],        // compat id
        2u16,                   // max entry id
        {2u16: "max_glyph_map_entry_id"},
        (Uint24::new(7)),       // glyph count
        {0u32: "glyph_map_offset"},
        0u32,                   // feature map offset
        0b00000010u8,           // applied entry bitmap (entry 1)

        8u16,                   // url template length
        {b'A': "url_template[0]"},
        {b'B': "url_template[1]"},
        [b'C', b'D', b'E', b'F', 0xc9, 0xa4], // url_template[2..7]

        {3u8: "patch_format"}, // = glyph keyed

        456u32, // charstrings offset [0]

        /* ### Glyph Map ### */
        {1u16: "glyph_map"},     // first mapped glyph
        {2u8: "entry_index[1]"},
        [1u8, 0, 1, 0, 0]        // entry index[2..6]
    };

    let offset = buffer.offset_for("glyph_map") as u32;
    buffer.write_at("glyph_map_offset", offset);

    buffer
}

pub fn simple_format1_with_two_charstrings_offsets() -> BeBuffer {
    let mut buffer = be_buffer! {
        /* ### Header ### */
        1u8,                    // format
        0u8, 0u8, 0u8,          // reserved
        0b00000011u8,           // has cff and cff2 charstrings offset
        [1u32, 2, 3, 4],        // compat id
        2u16,                   // max entry id
        {2u16: "max_glyph_map_entry_id"},
        (Uint24::new(7)),       // glyph count
        {0u32: "glyph_map_offset"},
        0u32,                   // feature map offset
        0b00000010u8,           // applied entry bitmap (entry 1)

        8u16,                   // url template length
        {b'A': "url_template[0]"},
        {b'B': "url_template[1]"},
        [b'C', b'D', b'E', b'F', 0xc9, 0xa4], // url_template[2..7]

        {3u8: "patch_format"}, // = glyph keyed

        456u32, // charstrings offset [0]
        789u32, // charstrings offset [1]

        /* ### Glyph Map ### */
        {1u16: "glyph_map"},     // first mapped glyph
        {2u8: "entry_index[1]"},
        [1u8, 0, 1, 0, 0]        // entry index[2..6]
    };

    let offset = buffer.offset_for("glyph_map") as u32;
    buffer.write_at("glyph_map_offset", offset);

    buffer
}

pub fn u16_entries_format1() -> BeBuffer {
    let mut buffer = be_buffer! {
      1u8,              // format
      0u32,             // reserved
      [1, 2, 3, 4u32],  // compat id

      300u16,           // max entry id
      300u16,           // max glyph map entry id

      (Uint24::new(7)), // glyph count

      {0u32: "glyph_map_offset"},
      {0u32: "feature_map_offset"},

      // applied entry bitmap (38 bytes)
      {0u8: "applied_entry_bitmap"},
      [
        0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0,
        0, 0u8
      ],

      6u16, // urlTemplateLength
      [4, b'f', b'o', b'o', b'/', 128u8],  // urlTemplate[6]

      3u8,                 // patch encoding = glyph keyed

      /* ### Glyph Map ### */
      {2u16: "glyph_map"}, // first mapped glyph

      // entryIndex[2..6]
      [80, 81, 300, 300, 80u16]
    };

    let offset = buffer.offset_for("glyph_map") as u32;
    buffer.write_at("glyph_map_offset", offset);

    buffer
}

pub fn feature_map_format1() -> BeBuffer {
    let mut buffer = be_buffer! {
      /* ### Header ### */
      1u8,                    // format

      0u32,  // reserved

      [1u32, 2u32, 3u32, 4u32], // compat id

      400u16, // max entry id
      300u16, // max glyph map entry id
      (Uint24::new(7)), // glyph count

      {0u32: "glyph_map_offset"},
      {0u32: "feature_map_offset"},

      // applied entry bitmap (51 bytes) - 299 is applied
      {0u8: "applied_entries"},
      [
        0, 0, 0, 0, 0, 0, 0,           // [0, 64)
        0, 0, 0, 0, 0, 0, 0, 0,           // [64, 128)
        0, 0, 0, 0, 0, 0, 0, 0,           // [128, 192)
        0, 0, 0, 0, 0, 0, 0, 0,           // [192, 256)
        0, 0, 0, 0, 0u8
      ],
      {0b00001000u8: "applied_entries_296"},
      [
        0, 0,  // [256, 320)
        0, 0, 0, 0, 0, 0, 0, 0,           // [320, 384)
        0, 0, 0u8                         // [384, 400)
      ],

      6u16, // urlTemplateLength
      [4, b'f', b'o', b'o', b'/', 128u8],  // urlTemplate[6]

      {3u8: "patch_format"},            // patch encoding = glyph keyed

      /* ### Glyph Map ### */
      {2u16: "glyph_map"}, // first mapped glyph

      // entryIndex[2..6]
      [
        80,     // gid 2
        81,     // gid 3
        300u16  // gid 4
      ],
      {299u16: "gid5_entry"},  // gid 5
      {80u16:  "gid6_entry"},  // gid 6

      // ## Feature Map ##
      {3u16: "feature_map"}, // feature count

      // FeatureRecord[0]
      {(Tag::new(b"dlig")): "FeatureRecord[0]"}, // feature tag
      400u16,                   // first new entry index
      1u16,                     // entry map count

      // FeatureRecord[1]
      {(Tag::new(b"liga")): "FeatureRecord[1]"}, // feature tag
      384u16,                   // first new entry index
      2u16,                     // entry map count

      // FeatureRecord[2]
      [b'n', b'u', b'l', b'l'], // feature tag
      301u16,                   // first new entry index
      1u16,                     // entry map count

      // EntryMapRecord[0]: "dlig" + entry 81 => entry 400
      81u16,                    // first_entry_index
      81u16,                    // last_entry_index

      // EntryMapRecord[1]: "liga" + entry [80, 81] => entry 384
      80u16,                    // first_entry_index
      81u16,                    // last_entry_index

      // EntryMapRecord[2]: "liga" + entry [299, 300] => entry 385
      299u16,                   // first_entry_index
      300u16,                   // last_entry_index

      // EntryMapRecord[3]: "null" + entry 0 => entry 301
      0u16,                     // first_entry_index
      0u16                      // last_entry_index
    };

    let offset = buffer.offset_for("glyph_map") as u32;
    buffer.write_at("glyph_map_offset", offset);

    let offset = buffer.offset_for("feature_map") as u32;
    buffer.write_at("feature_map_offset", offset);

    buffer
}

// Format specification: https://w3c.github.io/IFT/Overview.html#patch-map-format-2
pub fn codepoints_only_format2() -> BeBuffer {
    let mut buffer = be_buffer! {
      {2u8: "format"},    // format

      0u32,               // reserved

      {1u32: "compat_id[0]"},
      {2u32: "compat_id[1]"},
      {3u32: "compat_id[2]"},
      {4u32: "compat_id[3]"},

      3u8,                // default patch encoding
      {(Uint24::new(4)): "entry_count"},
      {0u32: "entries_offset"},
      0u32,               // entry string data offset

      6u16, // urlTemplateLength
      [4, b'f', b'o', b'o', b'/', 128u8],  // urlTemplate[6]

      /* ### Entries Array ### */
      // Entry id = 1
      {0b00010000u8: "entries[0]"},           // format = CODEPOINT_BIT_1
      [0b00001101, 0b00000011, 0b00110001u8], // codepoints = [0..17]

      // Entry id = 2
      {0b01100000u8: "entries[1]"},           // format = IGNORED | CODEPOINT_BIT_2
      5u16,                                   // bias
      [0b00001101, 0b00000011, 0b00110001u8], // codepoints = [5..22]

      // Entry id = 3
      {0b00100000u8: "entries[2]"},            // format = CODEPOINT_BIT_2
      5u16,                                    // bias
      [0b00001101, 0b00000011, 0b00110001u8],  // codepoints = [5..22]

      // Entry id = 4
      {0b00110000u8: "entries[3]"},            // format = CODEPOINT_BIT_1 | CODEPOINT_BIT_2
      (Uint24::new(80_000)),                   // bias
      [0b00001101, 0b00000011, 0b00110001u8]   // codepoints = [80_005..80_022]
    };

    let offset = buffer.offset_for("entries[0]") as u32;
    buffer.write_at("entries_offset", offset);

    buffer
}

pub fn format2_with_one_charstrings_offset() -> BeBuffer {
    let mut buffer = be_buffer! {
      2u8,                // format

      0u8, 0u8, 0u8,                 // reserved
      {0b00000001u8: "field_flags"}, // has charstrings offset

      {1u32: "compat_id[0]"},
      {2u32: "compat_id[1]"},
      {3u32: "compat_id[2]"},
      {4u32: "compat_id[3]"},

      3u8,                // default patch encoding
      (Uint24::new(1)),   // entry count
      {0u32: "entries_offset"},
      0u32,               // entry string data offset

      8u16, // urlTemplateLength
      [b'A', b'B', b'C', b'D', b'E', b'F', 0xc9, 0xa4],  // urlTemplate[8]

      {456u32: "charstrings_offset"}, // charstrings offset [0]

      /* ### Entries Array ### */
      // Entry id = 1
      {0b00010000u8: "entries[0]"},           // format = CODEPOINT_BIT_1
      [0b00001101, 0b00000011, 0b00110001u8]  // codepoints = [0..17]
    };

    let offset = buffer.offset_for("entries[0]") as u32;
    buffer.write_at("entries_offset", offset);

    buffer
}

pub fn format2_with_two_charstrings_offset() -> BeBuffer {
    let mut buffer = be_buffer! {
      2u8,                // format

      0u8, 0u8, 0u8,          // reserved
      0b00000011u8,           // has cff and cff2 charstrings offset

      {1u32: "compat_id[0]"},
      {2u32: "compat_id[1]"},
      {3u32: "compat_id[2]"},
      {4u32: "compat_id[3]"},

      3u8,                // default patch encoding
      (Uint24::new(1)),   // entry count
      {0u32: "entries_offset"},
      0u32,               // entry string data offset

      8u16, // urlTemplateLength
      [b'A', b'B', b'C', b'D', b'E', b'F', 0xc9, 0xa4],  // urlTemplate[8]

      456u32, // charstrings offset [0]
      789u32, // charstrings offset [1]

      /* ### Entries Array ### */
      // Entry id = 1
      {0b00010000u8: "entries[0]"},           // format = CODEPOINT_BIT_1
      [0b00001101, 0b00000011, 0b00110001u8]  // codepoints = [0..17]
    };

    let offset = buffer.offset_for("entries[0]") as u32;
    buffer.write_at("entries_offset", offset);

    buffer
}

pub fn features_and_design_space_format2() -> BeBuffer {
    let mut buffer = be_buffer! {
      2u8, // format

      0u32, // reserved

      [1, 2, 3, 4u32], // compat id

      {3u8: "patch_format"}, // default patch encoding
      (Uint24::new(3)), // entry count
      {0u32: "entries_offset"},
      0u32, // entry id string data offset

      6u16, // urlTemplateLength
      [4, b'f', b'o', b'o', b'/', 128u8],  // urlTemplate[6]

      /* ### Entries Array ### */
      // Entry id = 1
      {0b00010001u8: "entries[0]"},          // format = CODEPOINT_BIT_1 | FEATURES_AND_DESIGN_SPACE

      2u8,                                // feature count = 2
      (Tag::new(b"liga")),                // feature[0] = liga
      (Tag::new(b"smcp")),                // feature[1] = smcp

      1u16,                               // design space count
      (Tag::new(b"wdth")),                // tag = wdth
      {0x8000u32: "wdth start"},          // start = 0.5
      0x10000u32,                         // end = 1.0

      [0b00001101, 0b00000011, 0b00110001u8], // codepoints = [0..17]

      // Entry id = 2
      {0b00010001u8: "entries[1]"},       // format = CODEPOINT_BIT_1 | FEATURES_AND_DESIGN_SPACE

      1u8,                                // feature count
      (Tag::new(b"rlig")),                // feature[0]

      0u16,                               // design space count

      [0b00001101, 0b00000011, 0b00110001u8], // codepoints = [0..17]

      // Entry id = 3
      {0b000100001u8: "entries[2]"},      // format = CODEPOINT_BIT_2 | FEATURES_AND_DESIGN_SPACE

      1u8,                                // feature count = 1
      (Tag::new(b"smcp")),                // feature[0] = smcp

      3u16,                               // design space count
      (Tag::new(b"wght")),                // tag = wght
      0x00C8_0000u32,                     // start = 200
      0x02BC_0000u32,                     // end = 700

      (Tag::new(b"wdth")),                // tag = wdth
      0x0u32,                             // start = 0.0
      0x8000u32,                          // end = 0.5

      (Tag::new(b"wdth")),                // tag = wdth
      0x0002_0000u32,                     // start = 2.0
      0x0002_8000u32,                     // end = 2.5

      5u16,                               // bias = 5
      [0b00001101, 0b00000011, 0b00110001u8] // codepoints = [5..22]
    };

    let offset = buffer.offset_for("entries[0]") as u32;
    buffer.write_at("entries_offset", offset);

    buffer
}

pub fn child_indices_format2() -> BeBuffer {
    let mut buffer = be_buffer! {
      2u8,                      // format

      0u32,                     // reserved

      [1, 2, 3, 4u32],          // compat id

      {3u8: "encoding"},          // default patch encoding = glyph keyed
      (Uint24::new(9)),         // entry count
      {0u32: "entries_offset"}, // entries offset
      0u32,                     // entry id string data offset

      6u16, // urlTemplateLength
      [4, b'f', b'o', b'o', b'/', 128u8],  // urlTemplate[6]

      // Entries Array

      // Entry id = 1
      {0b01100000u8: "entries[0]"},           // format = CODEPOINT_BIT_2 | IGNORED
      5u16,                                   // bias = 5
      [0b00001101, 0b00000011, 0b00110001u8], // codepoints = [5..22]

      // Entry id = 2
      {0b00100000u8: "entries[1]"},           // format = CODEPOINT_BIT_2
      50u16,                                  // bias
      [0b00001101, 0b00000011, 0b00110001u8], // codepoints = [50..67]

      // Entry id = 3
      {0b00000001u8: "entries[2]"},           // format = FEATURES_AND_DESIGN_SPACE

      1u8,                                    // feature count = 1
      (Tag::new(b"rlig")),                    // feature[0] = rlig

      1u16,                                   // design space count = 1
      (Tag::new(b"wght")),                    // tag = wght
      0x00C8_0000u32,                         // start = 200
      0x02BC_0000u32,                         // end = 700

      // Entry id = 4
      {0b00000001u8: "entries[3]"},           // format = FEATURES_AND_DESIGN_SPACE

      1u8,                                    // feature count
      (Tag::new(b"liga")),                    // feature[0] = liga

      1u16,                                   // design space count
      (Tag::new(b"wght")),                    // tag = wght
      0x0032_0000,                            // start = 50
      0x0064_0000,                            // end = 100

      // Entry id = 5
      {0b00000010u8: "entries[4]"},           // format = CHILD_INDICES
      1u8,                                    // child count
      (Uint24::new(0)),                       // child[0] = 0

      // Entry id = 6
      {0b00000010u8: "entries[5]"},           // format = CHILD_INDICES
      1u8,                                    // child count
      (Uint24::new(2)),                       // child

      // Entry id = 7
      {0b00000010u8: "entries[6]"},           // format = CHILD_INDICES
      {4u8: "entries[6]_child_count"},        // child count
      (Uint24::new(3)),                       // child[0] = 3
      {(Uint24::new(2)): "entries[6]_child"}, // child[1] = 2
      (Uint24::new(1)),                       // child[2] = 1
      (Uint24::new(0)),                       // child[3] = 0

      // Entry id = 8
      {0b00000010u8: "entries[7]"},           // format = CHILD_INDICES
      2u8,                                    // child count
      (Uint24::new(4)),                       // child[0] = 4
      (Uint24::new(5)),                       // child[1] = 5

      // Entry id = 9
      {0b00100010u8: "entries[8]"},           // format = CODEPOINT_BIT_2 | CHILD_INDICES
      1u8,                                    // child count
      (Uint24::new(0)),                       // chil[0] = 0
      100u16,                                 // bias
      [0b00001101, 0b00000011, 0b00110001u8]  // codepoints = [100..117]
    };

    let offset = buffer.offset_for("entries[0]") as u32;
    buffer.write_at("entries_offset", offset);

    buffer
}

// Format specification: https://w3c.github.io/IFT/Overview.html#patch-map-format-2
pub fn custom_ids_format2() -> BeBuffer {
    let mut buffer = be_buffer! {
      2u8,                               // format

      0u32,                              // reserved

      {1u32: "compat_id[0]"},
      {2u32: "compat_id[1]"},
      {3u32: "compat_id[2]"},
      {4u32: "compat_id[3]"},

      3u8,                               // default patch encoding = glyph keyed
      {(Uint24::new(4)): "entry_count"}, // entry count
      {0u32: "entries_offset"},          // entries offset
      0u32,                              // entry id string data offset

      6u16, // urlTemplateLength
      [4, b'f', b'o', b'o', b'/', 128u8],  // urlTemplate[6]

      // Entries Array
      // Entry id = 0
      {0b00010100u8: "entries[0]"},           // format = CODEPOINT_BIT_1 | ID_DELTA
      {(Int24::new(-2)): "entries[0].id_delta"}, // id delta = -1
      [0b00001101, 0b00000011, 0b00110001u8], // codepoints = [0..17]

      // Entry id = 6
      {0b00100100u8: "entries[1]"},            // format = CODEPOINT_BIT_2 | ID_DELTA
      {(Int24::new(10)): "entries[1].id_delta"},           // id delta = 5
      5u16,                                   // bias
      [0b00001101, 0b00000011, 0b00110001u8], // codepoints = [5..22]

      // Entry id = 14
      {0b01000100u8: "entries[2]"},                  // format = ID_DELTA | IGNORED
      {(Int24::new(14)): "id delta - ignored entry"}, // id delta = 7

      // Entry id = 15
      {0b00101000u8: "entries[3]"},           // format = CODEPOINT_BIT_2 | PATCH_FORMAT
      {3u8: "entry[4] encoding"},             // patch encoding = Glyph Keyed
      10u16,                                  // bias
      [0b00001101, 0b00000011, 0b00110001u8]  // codepoints = [10..27]
    };

    let offset = buffer.offset_for("entries[0]") as u32;
    buffer.write_at("entries_offset", offset);

    buffer
}

// Format specification: https://w3c.github.io/IFT/Overview.html#patch-map-format-2
pub fn string_ids_format2() -> BeBuffer {
    let mut buffer = be_buffer! {
      2u8,                      // format

      0u32,                     // reserved

      [1, 2, 3, 4u32],          // compat id

      3u8,                      // default patch encoding = glyph keyed
      (Uint24::new(6)),         // entry count
      {0u32: "entries_offset"}, // entries offset
      {0u32: "string_data_offset"},                     // entry id string data offset

      6u16, // urlTemplateLength
      [4, b'f', b'o', b'o', b'/', 128u8],  // urlTemplate[6]

      /* ### Entry Data ### */

      // Entry id = ""
      {0b00000000u8: "entries"},              // format = {}

      // Entry id = abc
      0b00000100u8,                           // format = ID_DELTA
      (Uint24::new(3)),                       // id length

      // Entry id = defg
      0b00000100u8,                           // format = ID_DELTA
      (Uint24::new(4)),                       // id length

      // Entry id = defg
      0b00000000u8,                           // format = {}

      // Entry id = hij
      0b00000100u8,                           // format = ID_DELTA
      {(Uint24::new(3)): "entry[4] id length"},           // id length

      // Entry id = ""
      0b00000100u8,                           // format = ID_DELTA
      (Uint24::new(0)),                                   // id length

      /* ### String Data ### */
      {b'a': "string_data"},
      [b'b', b'c', b'd', b'e', b'f', b'g', b'h', b'i', b'j']
    };

    let offset = buffer.offset_for("string_data") as u32;
    buffer.write_at("string_data_offset", offset);

    let offset = buffer.offset_for("entries") as u32;
    buffer.write_at("entries_offset", offset);

    buffer
}

pub fn string_ids_format2_with_preloads() -> BeBuffer {
    const CONTINUE_MASK: u32 = 1 << 23;
    let mut buffer = be_buffer! {
      2u8,                      // format

      0u32,                     // reserved

      [1, 2, 3, 4u32],          // compat id

      3u8,                      // default patch encoding = glyph keyed
      (Uint24::new(5)),         // entry count
      {0u32: "entries_offset"}, // entries offset
      {0u32: "string_data_offset"},                     // entry id string data offset

      6u16, // urlTemplateLength
      [4, b'f', b'o', b'o', b'/', 128u8],  // urlTemplate[6]

      /* ### Entry Data ### */

      // Entry id = ""
      {0b00000000u8: "entries"},              // format = {}

      // Entry id = {abc, "", defg}
      0b00000100u8,                           // format = ID_DELTA
      (Uint24::new(CONTINUE_MASK | 3)),       // id length 3
      (Uint24::new(CONTINUE_MASK)),           // id length 0
      (Uint24::new(4)),                       // id length 4

      // Entry id = defg
      0b00000000u8,                           // format = {}

      // Entry id = hij
      0b00000100u8,                           // format = ID_DELTA
      {(Uint24::new(3)): "entry[4] id length"},           // id length

      // Entry id = ""
      0b00000100u8,                           // format = ID_DELTA
      (Uint24::new(0)),                                   // id length

      /* ### String Data ### */
      {b'a': "string_data"},
      [b'b', b'c', b'd', b'e', b'f', b'g', b'h', b'i', b'j']
    };

    let offset = buffer.offset_for("string_data") as u32;
    buffer.write_at("string_data_offset", offset);

    let offset = buffer.offset_for("entries") as u32;
    buffer.write_at("entries_offset", offset);

    buffer
}

// Format specification: https://w3c.github.io/IFT/Overview.html#patch-map-format-2
pub fn table_keyed_format2() -> BeBuffer {
    let mut buffer = be_buffer! {
      2u8,                // format

      0u32,               // reserved

      {1u32: "compat_id[0]"},
      {2u32: "compat_id[1]"},
      {3u32: "compat_id[2]"},
      {4u32: "compat_id[3]"},

      {1u8: "encoding"},  // default patch encoding
      {(Uint24::new(1)): "entry_count"},   // entry count
      {0u32: "entries_offset"},
      0u32,               // entry string data offset

      6u16, // urlTemplateLength
      [4u8, b'f', b'o', b'o', b'/'],
      {128u8: "url_template_var_end"}, // urlTemplate[6]

      /* ### Entries Array ### */
      // Entry id = 1
      {0b00100100u8: "entries"},              // format = CODEPOINT_BIT_2
      {(Int24::new(0)): "id_delta"},
      {0u16: "bias"},                         // bias = 0
      [0b00001101, 0b00000011, 0b00110001u8]  // codepoints = [0..17]
    };

    let offset = buffer.offset_for("entries") as u32;
    buffer.write_at("entries_offset", offset);

    buffer
}

pub fn table_keyed_format2_with_preload_urls() -> BeBuffer {
    let mut buffer = be_buffer! {
      2u8,                // format

      0u32,               // reserved

      {1u32: "compat_id[0]"},
      {2u32: "compat_id[1]"},
      {3u32: "compat_id[2]"},
      {4u32: "compat_id[3]"},

      {3u8: "encoding"},  // default patch encoding = Glyph Keyed
      (Uint24::new(4)),   // entry count
      {0u32: "entries_offset"},
      0u32,               // entry string data offset

      6u16, // urlTemplateLength
      [4, b'f', b'o', b'o', b'/', 128u8],  // urlTemplate[6]

      /* ### Entries Array ### */

      // Entry id = 1
      {0b00010000u8: "entries[0]"},           // format = CODEPOINT_BIT_1
      [0b00001101, 0b00000011, 0b00110001u8], // codepoints = [0..17]

      // Entry id = {9, 10, 6}
      {0b00010100u8: "entries[1]"},           // format = CODEPOINT_BIT_1
      (Int24::new(15)),                       // delta +7
      (Int24::new(1)),                        // delta +0
      (Int24::new(-10)),                      // delta -5
      [0b00001101, 0b00000011, 0b00110001u8], // codepoints = [0..17]

      // Entry id = {2, 3}
      {0b00010100u8: "entries[2]"},              // format = CODEPOINT_BIT_1
      (Int24::new(-11)),                      // delta -5
      (Int24::new(0)),                        // delta +0
      [0b00001101, 0b00000011, 0b00110001u8], // codepoints = [0..17]

      // Entry id = 4
      {0b00010000u8: "entries[3]"},           // format = CODEPOINT_BIT_1
      [0b00001101, 0b00000011, 0b00110001u8]  // codepoints = [0..17]
    };

    let offset = buffer.offset_for("entries[0]") as u32;
    buffer.write_at("entries_offset", offset);

    buffer
}

// Format specification: https://w3c.github.io/IFT/Overview.html#table-keyed
pub fn table_keyed_patch() -> BeBuffer {
    let mut buffer = be_buffer! {
        {(Tag::new(b"iftk")): "tag"},
        0u32,                 // reserved
        {1u32: "compat_id"},
        [2, 3, 4u32],       // compat id
        3u16,                 // patch count

        // patch_offsets[3]
        {0u32: "patch_off[0]"},
        {0u32: "patch_off[1]"},
        {0u32: "patch_off[2]"},
        {0u32: "patch_off[3]"},

        // patch[0]
        {(Tag::new(b"tab1")): "patch[0]"},
        0u8,       // flags
        {29u32: "decompressed_len[0]"},     // max decompressed length
        // brotli stream (w/ shared dict)
        [0xa1, 0xe0, 0x00, 0xc0, 0x2f, 0x3a, 0x38, 0xf4, 0x01, 0xd1, 0xaf, 0x54, 0x84, 0x14, 0x71,
         0x2a, 0x80, 0x04, 0xa2, 0x1c, 0xd3, 0xdd, 0x07u8],

         // patch[1]
        {(Tag::new(b"tab2")): "patch[1]"},
        {1u8: "flags[1]"},  // flags (REPLACEMENT)
        30u32,              // max decompressed length
        // brotli stream (w/o shared dict)
        [0xa1, 0xe8, 0x00, 0xc0, 0xef, 0x48, 0x9d, 0xfa, 0xdc, 0xf1, 0xc2, 0xac, 0xc5, 0xde, 0xe4, 0xf4,
         0xb4, 0x02, 0x48, 0x98, 0x98, 0x52, 0x64, 0xa8, 0x50, 0x20, 0x29, 0x75, 0x0bu8],

         // patch[2]
        {(Tag::new(b"tab3")): "patch[2]"},
        {2u8: "flags[2]"}, // flags (DROP)
        {0u32: "end"}      // max decompressed length
    };

    let offset = buffer.offset_for("patch[0]") as u32;
    buffer.write_at("patch_off[0]", offset);

    let offset = buffer.offset_for("patch[1]") as u32;
    buffer.write_at("patch_off[1]", offset);

    let offset = buffer.offset_for("patch[2]") as u32;
    buffer.write_at("patch_off[2]", offset);

    let offset = (buffer.offset_for("end") + 4) as u32;
    buffer.write_at("patch_off[3]", offset);

    buffer
}

// Format specification: https://w3c.github.io/IFT/Overview.html#table-keyed
pub fn noop_table_keyed_patch() -> BeBuffer {
    be_buffer! {
        {(Tag::new(b"iftk")): "tag"},
        0u32,                 // reserved
        [1, 2, 3, 4u32],       // compat id
        0u16,                 // patch count

        // patch_offsets[1]
        {0u32: "patch_off[0]"}
    }
}

// Format specification: https://w3c.github.io/IFT/Overview.html#glyph-keyed
pub fn glyph_keyed_patch_header() -> BeBuffer {
    be_buffer! {
      {(Tag::new(b"ifgk")): "format"}, // format
      0u32,                // reserved
      0u8,                 // flags (0 = u16 gids)
      {6u32: "compatibility_id"},
      [7, 8, 9u32],     // compatibility id
      {0u32: "max_uncompressed_length"}
    }
}

// Format specification: https://w3c.github.io/IFT/Overview.html#glyphpatches
pub fn noop_glyf_glyph_patches() -> BeBuffer {
    be_buffer! {
      0u32,       // glyph count
      {1u8: "table_count"},        // table count

      (Tag::new(b"glyf")),   // tables * 1

      // glyph data offsets * 1
      0u32
    }
}

// Format specification: https://w3c.github.io/IFT/Overview.html#glyphpatches
pub fn glyf_u16_glyph_patches() -> BeBuffer {
    let mut buffer = be_buffer! {
      5u32,       // glyph count
      {1u8: "table_count"},        // table count

      // glyph ids * 5
      [2, 7u16],
      {8u16: "gid_8"},
      [9u16],
      {13u16: "gid_13"},

      (Tag::new(b"glyf")),   // tables * 1

      // glyph data offsets * 6
      {0u32: "gid_2_offset"},
      {0u32: "gid_7_offset"},
      {0u32: "gid_8_offset"},
      {0u32: "gid_9_offset"},
      {0u32: "gid_13_offset"},
      {0u32: "end_offset"},

      // data blocks
      {b'a': "gid_2_data"},
      [b'b', b'c'],

      {b'd': "gid_7_data"},
      [b'e', b'f', b'g'],

      {b'h': "gid_8_and_9_data"},
      [b'i', b'j', b'k', b'l'],

      {b'm': "gid_13_data"},
      [b'n']
    };

    let offset = buffer.offset_for("gid_2_data") as u32;
    buffer.write_at("gid_2_offset", offset);

    let offset = buffer.offset_for("gid_7_data") as u32;
    buffer.write_at("gid_7_offset", offset);

    let offset = buffer.offset_for("gid_8_and_9_data") as u32;
    buffer.write_at("gid_8_offset", offset);

    let offset = buffer.offset_for("gid_8_and_9_data") as u32;
    buffer.write_at("gid_9_offset", offset);

    let offset = buffer.offset_for("gid_13_data") as u32;
    buffer.write_at("gid_13_offset", offset);
    buffer.write_at("end_offset", offset + 2);

    buffer
}

// Format specification: https://w3c.github.io/IFT/Overview.html#glyphpatches
pub fn glyf_u16_glyph_patches_2() -> BeBuffer {
    let mut buffer = be_buffer! {
      3u32,       // glyph count
      {1u8: "table_count"},        // table count

      // glyph ids * 3
      7u16,
      12u16,
      14u16,

      (Tag::new(b"glyf")),   // tables * 1

      // glyph data offsets * 6
      {0u32: "gid_7_offset"},
      {0u32: "gid_12_offset"},
      {0u32: "gid_14_offset"},
      {0u32: "end_offset"},

      // data blocks
      {b'q': "gid_7_data"},
      [b'r'],

      {b's': "gid_12_data"},
      [b't', b'u'],

      {b'v': "gid_14_data"}
    };

    let offset = buffer.offset_for("gid_7_data") as u32;
    buffer.write_at("gid_7_offset", offset);

    let offset = buffer.offset_for("gid_12_data") as u32;
    buffer.write_at("gid_12_offset", offset);

    let offset = buffer.offset_for("gid_14_data") as u32;
    buffer.write_at("gid_14_offset", offset);
    buffer.write_at("end_offset", offset + 1);

    buffer
}

// Format specification: https://w3c.github.io/IFT/Overview.html#glyphpatches
pub fn glyf_u24_glyph_patches() -> BeBuffer {
    let mut buffer = be_buffer! {
      5u32,       // glyph count
      1u8,        // table count
      (Uint24::new(2)), (Uint24::new(7)), (Uint24::new(8)), (Uint24::new(9)), (Uint24::new(13)),   // glyph ids * 5
      (Tag::new(b"glyf")),   // tables * 1

      // glyph data offsets * 6
      {0u32: "gid_2_offset"},
      {0u32: "gid_7_offset"},
      {0u32: "gid_8_offset"},
      {0u32: "gid_9_offset"},
      {0u32: "gid_13_offset"},
      {0u32: "end_offset"},

      // data blocks
      {b'a': "gid_2_data"},
      [b'b', b'c'],

      {b'd': "gid_7_data"},
      [b'e', b'f', b'g'],

      {b'h': "gid_8_and_9_data"},
      [b'i', b'j', b'k', b'l'],

      {b'm': "gid_13_data"},
      [b'n']
    };

    let offset = buffer.offset_for("gid_2_data") as u32;
    buffer.write_at("gid_2_offset", offset);

    let offset = buffer.offset_for("gid_7_data") as u32;
    buffer.write_at("gid_7_offset", offset);

    let offset = buffer.offset_for("gid_8_and_9_data") as u32;
    buffer.write_at("gid_8_offset", offset);

    let offset = buffer.offset_for("gid_8_and_9_data") as u32;
    buffer.write_at("gid_9_offset", offset);

    let offset = buffer.offset_for("gid_13_data") as u32;
    buffer.write_at("gid_13_offset", offset);
    buffer.write_at("end_offset", offset + 2);

    buffer
}

// Format specification: https://w3c.github.io/IFT/Overview.html#glyphpatches
pub fn glyf_and_gvar_u16_glyph_patches() -> BeBuffer {
    let mut buffer = be_buffer! {
      3u32,       // glyph count
      2u8,        // table count
      [2, 7, 8u16],   // glyph ids * 3
      {(Tag::new(b"glyf")): "glyf_tag"}, // tables[0]
      {(Tag::new(b"gvar")): "gvar_tag"}, // tables[1]

      // glyph data offsets * 7
      {0u32: "glyf_gid_2_offset"},
      {0u32: "glyf_gid_7_offset"},
      {0u32: "glyf_gid_8_offset"},
      {0u32: "gvar_gid_2_offset"},
      {0u32: "gvar_gid_7_offset"},
      {0u32: "gvar_gid_8_offset"},
      {0u32: "end_offset"},

      // data blocks
      {b'a': "glyf_gid_2_data"},
      [b'b', b'c'],

      {b'd': "glyf_gid_7_data"},
      [b'e', b'f', b'g'],

      {b'h': "glyf_gid_8_data"},
      [b'i', b'j', b'k', b'l'],

      {b'm': "gvar_gid_2_data"},
      [b'n'],

      {b'o': "gvar_gid_7_data"},
      [b'p', b'q'],

      {b'r': "gvar_gid_8_data"}
    };

    let offset = buffer.offset_for("glyf_gid_2_data") as u32;
    buffer.write_at("glyf_gid_2_offset", offset);
    let offset = buffer.offset_for("glyf_gid_7_data") as u32;
    buffer.write_at("glyf_gid_7_offset", offset);
    let offset = buffer.offset_for("glyf_gid_8_data") as u32;
    buffer.write_at("glyf_gid_8_offset", offset);

    let offset = buffer.offset_for("gvar_gid_2_data") as u32;
    buffer.write_at("gvar_gid_2_offset", offset);
    let offset = buffer.offset_for("gvar_gid_7_data") as u32;
    buffer.write_at("gvar_gid_7_offset", offset);
    let offset = buffer.offset_for("gvar_gid_8_data") as u32;
    buffer.write_at("gvar_gid_8_offset", offset);
    buffer.write_at("end_offset", offset + 1);

    buffer
}

pub fn cff_u16_glyph_patches() -> BeBuffer {
    let mut buffer = be_buffer! {
      4u32,       // glyph count
      {1u8: "table_count"},        // table count

      // 4 glyph ids
      [1,      // first gid
       38,
       47,
       59u16], // last gid

      {(Tag::new(b"CFF ")): "tag"},   // tables * 1

      // 5 glyph data offsets
      {0u32: "gid_1_offset"},
      {0u32: "gid_38_offset"},
      {0u32: "gid_47_offset"},
      {0u32: "gid_59_offset"},
      {0u32: "end_offset"},

      // data blocks
      {b'a': "gid_1_data"},
      [b'b', b'c'],

      {b'd': "gid_38_data"},
      [b'e', b'f', b'g'],

      {b'h': "gid_47_data"},
      [b'i', b'j', b'k', b'l'],

      {b'm': "gid_59_data"},
      [b'n']
    };

    let offset = buffer.offset_for("gid_1_data") as u32;
    buffer.write_at("gid_1_offset", offset);

    let offset = buffer.offset_for("gid_38_data") as u32;
    buffer.write_at("gid_38_offset", offset);

    let offset = buffer.offset_for("gid_47_data") as u32;
    buffer.write_at("gid_47_offset", offset);

    let offset = buffer.offset_for("gid_59_data") as u32;
    buffer.write_at("gid_59_offset", offset);
    buffer.write_at("end_offset", offset + 2);

    buffer
}

/// <https://learn.microsoft.com/en-us/typography/opentype/spec/gvar>
pub fn short_gvar_with_shared_tuples() -> BeBuffer {
    // This gvar has the correct header and tuple structure but the per glyph variation data is not valid.
    // Meant for testing with IFT glyph keyed patching which treats the per glyph data as opaque blobs.
    let mut buffer = be_buffer! {
      // HEADER
      1u16, // major version
      0u16, // minor version
      1u16, // axis count
      3u16, // sharedTupleCount
      {0u32: "shared_tuples_offset"},
      15u16, // glyph count
      0u16,  // flags
      {0u32: "glyph_variation_data_offset"},

      // OFFSETS
      {0u16: "glyph_offset[0]"},
      {0u16: "glyph_offset[1]"},
      {0u16: "glyph_offset[2]"},
      {0u16: "glyph_offset[3]"},
      {0u16: "glyph_offset[4]"},
      {0u16: "glyph_offset[5]"},
      {0u16: "glyph_offset[6]"},
      {0u16: "glyph_offset[7]"},
      {0u16: "glyph_offset[8]"},
      {0u16: "glyph_offset[9]"},
      {0u16: "glyph_offset[10]"},
      {0u16: "glyph_offset[11]"},
      {0u16: "glyph_offset[12]"},
      {0u16: "glyph_offset[13]"},
      {0u16: "glyph_offset[14]"},
      {0u16: "glyph_offset[15]"},

      // SHARED TUPLES
      {42u16: "sharedTuples[0]"},
      13u16,
      25u16,

      // GLYPH VARIATION DATA
      {1u8: "glyph_0"}, [2, 3, 4u8],
      {5u8: "glyph_8"}, [6, 7, 8, 9u8], {10u8: "end"}
    };

    let offset = buffer.offset_for("sharedTuples[0]") as u32;
    buffer.write_at("shared_tuples_offset", offset);

    let data_offset = buffer.offset_for("glyph_0");
    buffer.write_at("glyph_variation_data_offset", data_offset as u32);

    let glyph0_offset = ((buffer.offset_for("glyph_0") - data_offset) / 2) as u16;
    let glyph8_offset = ((buffer.offset_for("glyph_8") - data_offset) / 2) as u16;
    let end_offset = ((buffer.offset_for("end") + 1 - data_offset) / 2) as u16;

    buffer.write_at("glyph_offset[0]", glyph0_offset);
    buffer.write_at("glyph_offset[1]", glyph8_offset);
    buffer.write_at("glyph_offset[2]", glyph8_offset);
    buffer.write_at("glyph_offset[3]", glyph8_offset);
    buffer.write_at("glyph_offset[4]", glyph8_offset);
    buffer.write_at("glyph_offset[5]", glyph8_offset);
    buffer.write_at("glyph_offset[6]", glyph8_offset);
    buffer.write_at("glyph_offset[7]", glyph8_offset);
    buffer.write_at("glyph_offset[8]", glyph8_offset);
    buffer.write_at("glyph_offset[9]", end_offset);
    buffer.write_at("glyph_offset[10]", end_offset);
    buffer.write_at("glyph_offset[11]", end_offset);
    buffer.write_at("glyph_offset[12]", end_offset);
    buffer.write_at("glyph_offset[13]", end_offset);
    buffer.write_at("glyph_offset[14]", end_offset);
    buffer.write_at("glyph_offset[15]", end_offset);

    buffer
}

/// <https://learn.microsoft.com/en-us/typography/opentype/spec/gvar>
pub fn long_gvar_with_shared_tuples() -> BeBuffer {
    // This gvar has the correct header and tuple structure but the per glyph variation data is not valid.
    // Meant for testing with IFT glyph keyed patching which treats the per glyph data as opaque blobs.
    let mut buffer = be_buffer! {
      // HEADER
      1u16, // major version
      0u16, // minor version
      1u16, // axis count
      3u16, // sharedTupleCount
      {0u32: "shared_tuples_offset"},
      15u16, // glyph count
      0b00000000_00000001u16,  // flags
      {0u32: "glyph_variation_data_offset"},

      // OFFSETS
      {0u32: "glyph_offset[0]"},
      {0u32: "glyph_offset[1]"},
      {0u32: "glyph_offset[2]"},
      {0u32: "glyph_offset[3]"},
      {0u32: "glyph_offset[4]"},
      {0u32: "glyph_offset[5]"},
      {0u32: "glyph_offset[6]"},
      {0u32: "glyph_offset[7]"},
      {0u32: "glyph_offset[8]"},
      {0u32: "glyph_offset[9]"},
      {0u32: "glyph_offset[10]"},
      {0u32: "glyph_offset[11]"},
      {0u32: "glyph_offset[12]"},
      {0u32: "glyph_offset[13]"},
      {0u32: "glyph_offset[14]"},
      {0u32: "glyph_offset[15]"},

      // SHARED TUPLES
      {42u16: "sharedTuples[0]"},
      13u16,
      25u16,

      // GLYPH VARIATION DATA
      {1u8: "glyph_0"}, [2, 3, 4u8],
      {5u8: "glyph_8"}, [6, 7, 8, 9u8], {10u8: "end"}
    };

    let offset = buffer.offset_for("sharedTuples[0]") as u32;
    buffer.write_at("shared_tuples_offset", offset);

    let data_offset = buffer.offset_for("glyph_0");
    buffer.write_at("glyph_variation_data_offset", data_offset as u32);

    let glyph0_offset = (buffer.offset_for("glyph_0") - data_offset) as u32;
    let glyph8_offset = (buffer.offset_for("glyph_8") - data_offset) as u32;
    let end_offset = (buffer.offset_for("end") + 1 - data_offset) as u32;

    buffer.write_at("glyph_offset[0]", glyph0_offset);
    buffer.write_at("glyph_offset[1]", glyph8_offset);
    buffer.write_at("glyph_offset[2]", glyph8_offset);
    buffer.write_at("glyph_offset[3]", glyph8_offset);
    buffer.write_at("glyph_offset[4]", glyph8_offset);
    buffer.write_at("glyph_offset[5]", glyph8_offset);
    buffer.write_at("glyph_offset[6]", glyph8_offset);
    buffer.write_at("glyph_offset[7]", glyph8_offset);
    buffer.write_at("glyph_offset[8]", glyph8_offset);
    buffer.write_at("glyph_offset[9]", end_offset);
    buffer.write_at("glyph_offset[10]", end_offset);
    buffer.write_at("glyph_offset[11]", end_offset);
    buffer.write_at("glyph_offset[12]", end_offset);
    buffer.write_at("glyph_offset[13]", end_offset);
    buffer.write_at("glyph_offset[14]", end_offset);
    buffer.write_at("glyph_offset[15]", end_offset);

    buffer
}

pub fn short_gvar_with_no_shared_tuples() -> BeBuffer {
    // This gvar has the correct header and tuple structure but the per glyph variation data is not valid.
    // Meant for testing with IFT glyph keyed patching which treats the per glyph data as opaque blobs.
    let mut buffer = be_buffer! {
      // HEADER
      1u16,  // major version
      0u16,  // minor version
      1u16,  // axis count
      {0u16: "shared_tuple_count"},
      {0u32: "shared_tuples_offset"},
      15u16, // glyph count
      0u16,  // flags
      {0u32: "glyph_variation_data_offset"},

      // OFFSETS
      {0u16: "glyph_offset[0]"},
      {0u16: "glyph_offset[1]"},
      {0u16: "glyph_offset[2]"},
      {0u16: "glyph_offset[3]"},
      {0u16: "glyph_offset[4]"},
      {0u16: "glyph_offset[5]"},
      {0u16: "glyph_offset[6]"},
      {0u16: "glyph_offset[7]"},
      {0u16: "glyph_offset[8]"},
      {0u16: "glyph_offset[9]"},
      {0u16: "glyph_offset[10]"},
      {0u16: "glyph_offset[11]"},
      {0u16: "glyph_offset[12]"},
      {0u16: "glyph_offset[13]"},
      {0u16: "glyph_offset[14]"},
      {0u16: "glyph_offset[15]"},

      // GLYPH VARIATION DATA
      {1u8: "glyph_0"}, [2, 3, 4u8],
      {5u8: "glyph_8"}, [6, 7, 8, 9u8], {10u8: "end"}
    };

    let data_offset = buffer.offset_for("glyph_0");
    buffer.write_at("shared_tuples_offset", data_offset as u32);
    buffer.write_at("glyph_variation_data_offset", data_offset as u32);

    let glyph0_offset = ((buffer.offset_for("glyph_0") - data_offset) / 2) as u16;
    let glyph8_offset = ((buffer.offset_for("glyph_8") - data_offset) / 2) as u16;
    let end_offset = ((buffer.offset_for("end") + 1 - data_offset) / 2) as u16;

    buffer.write_at("glyph_offset[0]", glyph0_offset);
    buffer.write_at("glyph_offset[1]", glyph8_offset);
    buffer.write_at("glyph_offset[2]", glyph8_offset);
    buffer.write_at("glyph_offset[3]", glyph8_offset);
    buffer.write_at("glyph_offset[4]", glyph8_offset);
    buffer.write_at("glyph_offset[5]", glyph8_offset);
    buffer.write_at("glyph_offset[6]", glyph8_offset);
    buffer.write_at("glyph_offset[7]", glyph8_offset);
    buffer.write_at("glyph_offset[8]", glyph8_offset);
    buffer.write_at("glyph_offset[9]", end_offset);
    buffer.write_at("glyph_offset[10]", end_offset);
    buffer.write_at("glyph_offset[11]", end_offset);
    buffer.write_at("glyph_offset[12]", end_offset);
    buffer.write_at("glyph_offset[13]", end_offset);
    buffer.write_at("glyph_offset[14]", end_offset);
    buffer.write_at("glyph_offset[15]", end_offset);

    buffer
}

pub fn short_gvar_near_maximum_offset_size() -> BeBuffer {
    // This is a short offset gvar table whose glyph data is at the maximum representable size with short offsets

    // This gvar has the correct header and tuple structure but the per glyph variation data is not valid.
    // Meant for testing with IFT glyph keyed patching which treats the per glyph data as opaque blobs.
    let buffer = be_buffer! {
      // HEADER
      1u16,  // major version
      0u16,  // minor version
      1u16,  // axis count
      {0u16: "shared_tuple_count"},
      {0u32: "shared_tuples_offset"},
      15u16, // glyph count
      0u16,  // flags
      {0u32: "glyph_variation_data_offset"},

      // OFFSETS
      {0u16: "glyph_offset[0]"},
      {0u16: "glyph_offset[1]"},
      {0u16: "glyph_offset[2]"},
      {0u16: "glyph_offset[3]"},
      {0u16: "glyph_offset[4]"},
      {0u16: "glyph_offset[5]"},
      {0u16: "glyph_offset[6]"},
      {0u16: "glyph_offset[7]"},
      {0u16: "glyph_offset[8]"},
      {0u16: "glyph_offset[9]"},
      {0u16: "glyph_offset[10]"},
      {0u16: "glyph_offset[11]"},
      {0u16: "glyph_offset[12]"},
      {0u16: "glyph_offset[13]"},
      {0u16: "glyph_offset[14]"},
      {0u16: "glyph_offset[15]"},

      // GLYPH VARIATION DATA
      {1u8: "glyph_0"}
    };

    // Glyph 0
    let mut buffer = buffer.extend(iter::repeat_n(1u8, 131065));

    let data_offset = buffer.offset_for("glyph_0");
    buffer.write_at("shared_tuples_offset", data_offset as u32);
    buffer.write_at("glyph_variation_data_offset", data_offset as u32);

    buffer.write_at("glyph_offset[0]", 0u16);
    buffer.write_at("glyph_offset[1]", 65533u16);
    buffer.write_at("glyph_offset[2]", 65533u16);
    buffer.write_at("glyph_offset[3]", 65533u16);
    buffer.write_at("glyph_offset[4]", 65533u16);
    buffer.write_at("glyph_offset[5]", 65533u16);
    buffer.write_at("glyph_offset[6]", 65533u16);
    buffer.write_at("glyph_offset[7]", 65533u16);
    buffer.write_at("glyph_offset[8]", 65533u16);
    buffer.write_at("glyph_offset[9]", 65533u16);
    buffer.write_at("glyph_offset[10]", 65533u16);
    buffer.write_at("glyph_offset[11]", 65533u16);
    buffer.write_at("glyph_offset[12]", 65533u16);
    buffer.write_at("glyph_offset[13]", 65533u16);
    buffer.write_at("glyph_offset[14]", 65533u16);
    buffer.write_at("glyph_offset[15]", 65533u16);

    buffer
}

/// <https://learn.microsoft.com/en-us/typography/opentype/spec/gvar>
pub fn out_of_order_gvar_with_shared_tuples() -> BeBuffer {
    let mut buffer = be_buffer! {
      // HEADER
      1u16, // major version
      0u16, // minor version
      1u16, // axis count
      3u16, // sharedTupleCount
      {0u32: "shared_tuples_offset"},
      15u16, // glyph count
      0u16,  // flags
      {0u32: "glyph_variation_data_offset"},

      // OFFSETS
      {0u16: "glyph_offset[0]"},
      {0u16: "glyph_offset[1]"},
      {0u16: "glyph_offset[2]"},
      {0u16: "glyph_offset[3]"},
      {0u16: "glyph_offset[4]"},
      {0u16: "glyph_offset[5]"},
      {0u16: "glyph_offset[6]"},
      {0u16: "glyph_offset[7]"},
      {0u16: "glyph_offset[8]"},
      {0u16: "glyph_offset[9]"},
      {0u16: "glyph_offset[10]"},
      {0u16: "glyph_offset[11]"},
      {0u16: "glyph_offset[12]"},
      {0u16: "glyph_offset[13]"},
      {0u16: "glyph_offset[14]"},
      {0u16: "glyph_offset[15]"},

      // GLYPH VARIATION DATA
      {1u8: "glyph_0"}, [2, 3, 4u8],
      {5u8: "glyph_8"}, [6, 7, 8, 9u8], {10u8: "end"},

      // SHARED TUPLES
      {42u16: "sharedTuples[0]"},
      13u16,
      25u16
    };

    let offset = buffer.offset_for("sharedTuples[0]") as u32;
    buffer.write_at("shared_tuples_offset", offset);

    let data_offset = buffer.offset_for("glyph_0");
    buffer.write_at("glyph_variation_data_offset", data_offset as u32);

    let glyph0_offset = ((buffer.offset_for("glyph_0") - data_offset) / 2) as u16;
    let glyph8_offset = ((buffer.offset_for("glyph_8") - data_offset) / 2) as u16;
    let end_offset = ((buffer.offset_for("end") + 1 - data_offset) / 2) as u16;

    buffer.write_at("glyph_offset[0]", glyph0_offset);
    buffer.write_at("glyph_offset[1]", glyph8_offset);
    buffer.write_at("glyph_offset[2]", glyph8_offset);
    buffer.write_at("glyph_offset[3]", glyph8_offset);
    buffer.write_at("glyph_offset[4]", glyph8_offset);
    buffer.write_at("glyph_offset[5]", glyph8_offset);
    buffer.write_at("glyph_offset[6]", glyph8_offset);
    buffer.write_at("glyph_offset[7]", glyph8_offset);
    buffer.write_at("glyph_offset[8]", glyph8_offset);
    buffer.write_at("glyph_offset[9]", end_offset);
    buffer.write_at("glyph_offset[10]", end_offset);
    buffer.write_at("glyph_offset[11]", end_offset);
    buffer.write_at("glyph_offset[12]", end_offset);
    buffer.write_at("glyph_offset[13]", end_offset);
    buffer.write_at("glyph_offset[14]", end_offset);
    buffer.write_at("glyph_offset[15]", end_offset);

    buffer
}
