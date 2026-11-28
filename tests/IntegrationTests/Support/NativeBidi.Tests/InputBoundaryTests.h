// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

void CheckDrawingByteBoundaries(Environment &env) {
    std::vector<std::string> fixtures;
    const auto addNoncharacter = [&](uint32_t scalar) {
        wchar_t wide[2]{};
        const int length = scalar > 0xffff ? 2 : 1;
        if (length == 1) {
            wide[0] = static_cast<wchar_t>(scalar);
        } else {
            wide[0] = static_cast<wchar_t>(0xd800 + ((scalar - 0x10000) >> 10));
            wide[1] = static_cast<wchar_t>(0xdc00 + ((scalar - 0x10000) & 0x3ff));
        }
        char bytes[4]{};
        const int count = WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, wide, length,
            bytes, sizeof(bytes), nullptr, nullptr);
        Require(count == (length == 1 ? 3 : 4), "Windows encodes the noncharacter fixture");
        fixtures.emplace_back(bytes, count);
    };
    for (uint32_t scalar = 0xfdd0; scalar <= 0xfdef; ++scalar) addNoncharacter(scalar);
    for (uint32_t plane = 0; plane <= 16; ++plane) {
        addNoncharacter((plane << 16) | 0xfffe);
        addNoncharacter((plane << 16) | 0xffff);
    }
    Require(fixtures.size() == 66, "Every Unicode noncharacter is represented");
    for (const auto malformed : {"\x80", "\xc0\xaf", "\xe0\x80\x80", "\xed\xa0\x80",
        "\xf4\x90\x80\x80", "\xe2", "\xe2\x82", "\xf0\x9f\x98"})
        fixtures.emplace_back(malformed);

    const auto prefix = Bytes(u8"אA");
    for (const auto &bytes : fixtures) {
        const auto text = prefix + bytes + 'B';
        std::vector<ParagraphFontRun> fonts;
        std::vector<ParagraphRepresentation> representations;
        // These are the production SurfaceD2D bridge's byte spans. The expected
        // mapping below is fixed: each unsupported byte occupies one UTF-16 unit.
        for (size_t start = 0; start < text.size();) {
            const size_t count = UTF8DrawBytes(text.data() + start, text.size() - start);
            fonts.push_back({start, count, env.format.Get()});
            if (start >= prefix.size() && start < prefix.size() + bytes.size()) {
                Require(count == 1, "Scintilla draws each unsupported byte independently");
                representations.push_back({start, count, 12});
            }
            start += count;
        }
        for (const bool rtl : {false, true}) {
            DirectWriteParagraph paragraph(env.factory.Get(), text, env.format.Get(), fonts,
                representations, Options(rtl));
            Require(paragraph.IsValid() && paragraph.Length() == text.size(),
                "Font and representation spans accept all drawing byte boundaries");
            for (size_t index = 0; index <= bytes.size() + 1; ++index) {
                Require(paragraph.Utf16Position(prefix.size() + index) == 2 + index &&
                    paragraph.BytePosition(static_cast<UINT32>(2 + index)) == prefix.size() + index,
                    "Unsupported bytes and following text retain exact independent mapping");
            }
            const auto boxes = paragraph.RangeRectangles(prefix.size(), prefix.size() + bytes.size());
            float width = 0;
            for (const auto &box : boxes) width += box.width;
            Near(width, static_cast<float>(12 * bytes.size()),
                "Every byte representation retains its requested visible width");
        }
    }
    std::cout << "PASS: all 66 noncharacters and malformed UTF-8 share drawing/font/representation boundaries.\n";
}
