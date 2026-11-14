// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

// The production layout has no effects. Expected pixels come from a separate
// Windows DrawTextLayout with fixed, complete-cluster colour ranges. In particular,
// partially styling a ZWJ sequence must never split the original emoji or move
// following text: applying an effect to that partial range is an invalid oracle.

ComPtr<IDWriteTextLayout> ForegroundFixture(Environment &env, std::string_view text, ParagraphOptions options,
                                            float dpi, int gdi, bool ligatures) {
    auto result = Reference(env, text, options);
    if (gdi) {
        const auto wide = Wide(text);
        result.Reset();
        CheckHR(env.factory->CreateGdiCompatibleTextLayout(wide.data(), static_cast<UINT32>(wide.size()),
                                                           env.format.Get(), options.width, 100000, dpi / 96, nullptr,
                                                           gdi == 2, &result),
                "GDI-compatible independent layout");
        CheckHR(result->SetReadingDirection(options.rtl ? DWRITE_READING_DIRECTION_RIGHT_TO_LEFT
                                                        : DWRITE_READING_DIRECTION_LEFT_TO_RIGHT),
                "GDI direction");
        CheckHR(result->SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING), "GDI alignment");
        CheckHR(result->SetWordWrapping(options.wrap == ParagraphWrap::None ? DWRITE_WORD_WRAPPING_NO_WRAP
                                                                            : DWRITE_WORD_WRAPPING_WRAP),
                "GDI wrapping");
        CheckHR(result->SetLineSpacing(DWRITE_LINE_SPACING_METHOD_UNIFORM, options.lineHeight, options.baseline),
                "GDI spacing");
    }
    ComPtr<IDWriteTypography> typography;
    CheckHR(env.factory->CreateTypography(&typography), "typography");
    CheckHR(typography->AddFontFeature({DWRITE_FONT_FEATURE_TAG_STANDARD_LIGATURES, ligatures ? 1U : 0U}), "ligatures");
    CheckHR(result->SetTypography(typography.Get(), {0, UINT32_MAX}), "layout typography");
    return result;
}

void CheckOverlappingColorClusters() {
    struct Font {
        const wchar_t *file, *family;
        const char *name;
        DWRITE_GLYPH_IMAGE_FORMATS format;
    };
    const Font fonts[]{
        {L"LegacyColr.ttf", L"Notepads Test COLRv0", "COLRv0", DWRITE_GLYPH_IMAGE_FORMATS_COLR},
        {L"LegacySvg.ttf", L"Notepads Test SVG", "SVG", DWRITE_GLYPH_IMAGE_FORMATS_SVG},
        {L"LegacyBitmap.ttf", L"Notepads Test Bitmap", "PNG", DWRITE_GLYPH_IMAGE_FORMATS_PNG},
    };
    for (const auto &font : fonts)
        for (const float dpi : {96.f, 192.f})
            for (const float opacity : {1.f, .5f}) {
                Environment env;
                LoadPrivateFont(env, font.file, font.family);
                auto options = Options(false);
                options.width = 280;
                options.lineHeight = 48;
                options.baseline = 38;
                auto actualLayout = Reference(env, Bytes(u8"\uE000\uE000\uE000"), options);
                auto reference = Reference(env, Bytes(u8"\uE000\uE000\uE000"), options);
                // Authored glyph ink is 900 design units (~29 DIPs); a 10-DIP
                // advance makes adjacent clusters' translucent layers really overlap.
                for (auto *layout : {actualLayout.Get(), reference.Get()}) {
                    ComPtr<IDWriteTextLayout1> extended;
                    CheckHR(layout->QueryInterface(IID_PPV_ARGS(&extended)), "character spacing");
                    CheckHR(extended->SetCharacterSpacing(0, -22, 0, {0, UINT32_MAX}), "overlapping cluster advances");
                }
                LegacyFormatProbe format(font.format, env.factory.Get());
                CheckHR(reference->Draw(nullptr, &format, 0, 0), "private color format");
                CheckHR(format.failure, "genuine original legacy font");
                Require(format.glyphs == 3, "three real original color glyphs");
                DWRITE_TEXT_METRICS metrics{};
                CheckHR(reference->GetMetrics(&metrics), "overlap metrics");
                Require(metrics.widthIncludingTrailingWhitespace < 40, "adjacent color glyphs genuinely overlap");
                const auto matrix = D2D1::Matrix3x2F::Translation(37.375f, 17.625f);
                const auto origin = D2D1::Point2F(10.25f, 10.5f);
                const auto clip = D2D1::RectF(0, 0, 320, 100);
                RendererTarget actual(env, dpi, matrix), expected(env, dpi, matrix);
                actual.brush->SetColor(D2D1::ColorF(D2D1::ColorF::Blue));
                actual.brush->SetOpacity(opacity);
                expected.brush->SetColor(D2D1::ColorF(D2D1::ColorF::Blue));
                expected.brush->SetOpacity(opacity);
                ComPtr<ID2D1SolidColorBrush> red, expectedRed;
                CheckHR(actual.target->CreateSolidColorBrush(D2D1::ColorF(D2D1::ColorF::Red), &red), "actual red");
                CheckHR(expected.target->CreateSolidColorBrush(D2D1::ColorF(D2D1::ColorF::Red), &expectedRed),
                        "reference red");
                red->SetOpacity(opacity);
                expectedRed->SetOpacity(opacity);
                CheckHR(reference->SetDrawingEffect(expectedRed.Get(), {1, 1}),
                        "independent adjacent-cluster color boundary");
                CheckHR(expected.Paint(env, reference.Get(), false, origin, {&clip, 1}),
                        "system overlapping color reference");
                const auto selectedReference = expected.Pixels();
                const ParagraphBrushRun color{{1, 1}, red.Get()};
                for (const bool legacy : {false, true}) {
                    LegacyTarget facade(actual.target.Get());
                    actual.target->BeginDraw();
                    actual.target->Clear(D2D1::ColorF(D2D1::ColorF::White));
                    actual.target->PushAxisAlignedClip(clip, D2D1_ANTIALIAS_MODE_ALIASED);
                    const auto drawn = DrawParagraphTextClipped(
                        env.factory.Get(), legacy ? &facade : actual.target.Get(), actualLayout.Get(), origin, clip,
                        actual.brush.Get(), D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT, nullptr, {&color, 1});
                    actual.target->PopAxisAlignedClip();
                    const auto flushed = actual.target->EndDraw();
                    const auto label = std::string(font.name) + ",dpi=" + std::to_string(dpi) +
                                       ",opacity=" + std::to_string(opacity) + ",legacy=" + std::to_string(legacy);
                    CheckHR(drawn, label);
                    CheckHR(flushed, label);
                    CheckRendererPictures(actual.Pixels(), selectedReference,
                                          label + ": overlapping colored clusters retain exact composition");
                }
                CheckHR(reference->SetDrawingEffect(nullptr, {0, UINT32_MAX}), "clear reference color");
                CheckHR(expected.Paint(env, reference.Get(), false, origin, {&clip, 1}),
                        "plain overlap system reference");
                CheckHR(actual.Paint(env, actualLayout.Get(), true, origin, {&clip, 1}),
                        "same immutable layout without overrides");
                CheckRendererPictures(actual.Pixels(), expected.Pixels(), "same layout retains no selection brushes");
            }
}

void CheckWrappedBrushRows() {
    Environment env;
    std::string text;
    for (int i = 0; i < 8; ++i)
        text += Bytes(u8"أُإِآ אבג ABC ");
    const auto wide = Wide(text);
    const auto hebrew = static_cast<UINT32>(wide.find(L"אב"));
    for (const bool rtl : {false, true})
        for (const float width : {90.f, 220.f})
            for (const float dpi : {96.f, 144.f}) {
                auto options = Options(rtl, ParagraphWrap::Word, width);
                auto original = Reference(env, text, options), reference = Reference(env, text, options);
                const auto matrix = D2D1::Matrix3x2F::Translation(37.375f, 17.625f);
                RendererTarget actual(env, dpi, matrix), expected(env, dpi, matrix);
                ComPtr<ID2D1SolidColorBrush> red, green;
                CheckHR(actual.target->CreateSolidColorBrush(D2D1::ColorF(D2D1::ColorF::Red), &red), "row red");
                CheckHR(actual.target->CreateSolidColorBrush(D2D1::ColorF(D2D1::ColorF::Green), &green), "row green");
                const std::array<ParagraphBrushRun, 2> colors{{{{0, 1}, red.Get()}, {{hebrew, 2}, green.Get()}}};
                expected.Effect(reference.Get(), {0, 2}, D2D1::ColorF(D2D1::ColorF::Red));
                expected.Effect(reference.Get(), {hebrew, 2}, D2D1::ColorF(D2D1::ColorF::Green));
                const auto rows = ReferenceRows(original.Get()), referenceRows = ReferenceRows(reference.Get());
                Require(rows.size() >= 3 && rows.size() == referenceRows.size(),
                        "independent colored oracle retains wrapping");
                const auto origin = D2D1::Point2F(10.25f, 10.5f);
                for (size_t index = 0; index < 3; ++index) {
                    const auto &line = rows[index];
                    const DWRITE_TEXT_RANGE row{line.start, line.length};
                    Require(line.start == referenceRows[index].start && line.length == referenceRows[index].length,
                            "independent colored row boundaries agree");
                    const auto clip = D2D1::RectF(0, origin.y + line.top, 340, origin.y + line.top + line.height);
                    PaintReferenceRow(expected, reference.Get(), origin, clip, row.startPosition,
                                      row.startPosition + row.length);
                    for (const bool legacy : {false, true}) {
                        LegacyTarget facade(actual.target.Get());
                        actual.target->BeginDraw();
                        actual.target->Clear(D2D1::ColorF(D2D1::ColorF::White));
                        actual.target->PushAxisAlignedClip(clip, D2D1_ANTIALIAS_MODE_ALIASED);
                        const auto drawn = DrawParagraphTextClipped(
                            env.factory.Get(), legacy ? &facade : actual.target.Get(), original.Get(), origin, clip,
                            actual.brush.Get(), D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT, &row, colors);
                        actual.target->PopAxisAlignedClip();
                        const auto flushed = actual.target->EndDraw();
                        CheckHR(drawn, "immutable row overrides");
                        CheckHR(flushed, "immutable row flush");
                        CheckRendererPictures(actual.Pixels(), expected.Pixels(),
                                              "immutable row colors/overhang match independent row renderer");
                    }
                }
            }
    std::cout << "PASS wrapped immutable row colors, Arabic marks and neighboring-row exclusion.\n";
}

void CheckOverrideContracts() {
    Environment env;
    RendererTarget image(env);
    RejectedBrush rejected;
    auto layout = Reference(env, "ABC", Options(false));
    const auto origin = D2D1::Point2F(10, 10);
    const auto clip = D2D1::RectF(0, 0, 400, 100);
    const ParagraphBrushRun valid{{0, 1}, image.brush.Get()};
    const auto paint = [&](std::span<const ParagraphBrushRun> colors, D2D1_DRAW_TEXT_OPTIONS options,
                           ID2D1RenderTarget *target = nullptr) {
        image.target->BeginDraw();
        image.target->Clear(D2D1::ColorF(D2D1::ColorF::White));
        image.target->PushAxisAlignedClip(clip, D2D1_ANTIALIAS_MODE_ALIASED);
        const auto drawn =
            DrawParagraphTextClipped(env.factory.Get(), target ? target : image.target.Get(), layout.Get(), origin,
                                     clip, image.brush.Get(), options, nullptr, colors);
        image.target->PopAxisAlignedClip();
        const auto flushed = image.target->EndDraw();
        CheckHR(flushed, "contract target cleanup");
        return drawn;
    };
    const auto untouched = [&] {
        const auto pixels = image.Pixels();
        Require(std::all_of(pixels.begin(), pixels.end(), [](auto value) { return value == 255; }),
                "rejected override draws no partial or fallback pixels");
    };
    for (const ParagraphBrushRun invalid : std::array<ParagraphBrushRun, 3>{
             ParagraphBrushRun{{0, 1}, nullptr}, ParagraphBrushRun{{0, 0}, image.brush.Get()},
             ParagraphBrushRun{{UINT32_MAX, 2}, image.brush.Get()}}) {
        Require(paint({&invalid, 1}, D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT) == E_INVALIDARG,
                "invalid override rejected");
        untouched();
    }
    const std::array<ParagraphBrushRun, 2> overlaps{{{{0, 2}, image.brush.Get()}, {{1, 2}, image.brush.Get()}}};
    Require(paint(overlaps, D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT) == E_INVALIDARG,
            "overlapping overrides rejected");
    untouched();
    Require(paint({&valid, 1}, D2D1_DRAW_TEXT_OPTIONS_CLIP) == E_NOTIMPL,
            "unsupported options cannot discard overrides");
    untouched();
    LegacyTarget missing(image.target.Get(), false);
    Require(paint({&valid, 1}, D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT, &missing) == E_NOINTERFACE &&
                missing.wholeLayoutDraws == 0,
            "missing capability cannot silently use original DrawTextLayout");
    untouched();
    CheckHR(layout->SetDrawingEffect(&rejected, {0, UINT32_MAX}), "existing invalid effect");
    CheckHR(paint({&valid, 1}, D2D1_DRAW_TEXT_OPTIONS_NONE), "monochrome overrides bypass effects");
    Require(rejected.brushQueries == 0, "explicit overrides do not inspect suppressed callback effects");
    Require(paint({}, D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT) == E_NOINTERFACE && rejected.brushQueries > 0,
            "no override retains existing effect contract");
    CheckHR(layout->SetDrawingEffect(nullptr, {0, UINT32_MAX}), "release borrowed diagnostic effect");
    std::cout << "PASS override input, effect and pre-draw fallback contracts.\n";
}

D2D1_MATRIX_3X2_F ProbeTransform(int variant) {
    const auto translation = D2D1::Matrix3x2F::Translation(37.375f, 17.625f);
    if (variant == 1) return D2D1::Matrix3x2F::Scale(.75f, 1.2f) * translation;
    if (variant == 2) return D2D1::Matrix3x2F(1, .1f, .2f, 1, 37.375f, 17.625f);
    if (variant == 3) return D2D1::Matrix3x2F::Rotation(10) * translation;
    return translation;
}

void CheckOffscreenColoredOverhang() {
    Environment env;
    ComPtr<IDWriteTextFormat> italic;
    CheckHR(env.factory->CreateTextFormat(L"Times New Roman", nullptr, DWRITE_FONT_WEIGHT_NORMAL,
                                          DWRITE_FONT_STYLE_ITALIC, DWRITE_FONT_STRETCH_NORMAL, 36, L"en-US", &italic),
            "italic fixture");
    env.format = italic;
    std::string text;
    for (int i = 0; i < 60; ++i)
        text += "jf";
    const auto options = Options(false);
    auto original = Reference(env, text, options), reference = Reference(env, text, options);
    const auto matrix = ProbeTransform(0);
    DWRITE_OVERHANG_METRICS overhang{};
    CheckHR(original->GetOverhangMetrics(&overhang), "independent italic overhang");
    Require(overhang.left > .5f, "first italic glyph really extends before its logical cell");
    const std::array<std::pair<D2D1_POINT_2F, D2D1_RECT_F>, 2> views{
        {{D2D1::Point2F(-200.25f, 10.5f), D2D1::RectF(20, 0, 50, 100)},
         {D2D1::Point2F(10.25f, 10.5f), D2D1::RectF(10.25f - overhang.left - 1, 0, 10.25f, 100)}}};
    for (const float dpi : {96.f, 144.f, 192.f}) {
        RendererTarget actual(env, dpi, matrix), expected(env, dpi, matrix);
        ComPtr<ID2D1SolidColorBrush> red, green;
        CheckHR(actual.target->CreateSolidColorBrush(D2D1::ColorF(D2D1::ColorF::Red), &red), "offscreen red");
        CheckHR(actual.target->CreateSolidColorBrush(D2D1::ColorF(D2D1::ColorF::Green), &green), "offscreen green");
        std::vector<ParagraphBrushRun> colors;
        for (UINT32 i = 0; i < text.size(); ++i) {
            colors.push_back({{i, 1}, i % 2 ? green.Get() : red.Get()});
            expected.Effect(reference.Get(), {i, 1}, D2D1::ColorF(i % 2 ? D2D1::ColorF::Green : D2D1::ColorF::Red));
        }
        for (const auto &[origin, clip] : views) {
            CheckHR(expected.Paint(env, reference.Get(), false, origin, {&clip, 1}),
                    "offscreen original clipped reference");
            for (const bool legacy : {false, true}) {
                LegacyTarget facade(actual.target.Get());
                actual.target->BeginDraw();
                actual.target->Clear(D2D1::ColorF(D2D1::ColorF::White));
                actual.target->PushAxisAlignedClip(clip, D2D1_ANTIALIAS_MODE_ALIASED);
                const auto drawn = DrawParagraphTextClipped(env.factory.Get(), legacy ? &facade : actual.target.Get(),
                                                            original.Get(), origin, clip, actual.brush.Get(),
                                                            D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT, nullptr, colors);
                actual.target->PopAxisAlignedClip();
                const auto flushed = actual.target->EndDraw();
                CheckHR(drawn, "offscreen multi-colour slices");
                CheckHR(flushed, "offscreen flush");
                CheckRendererPictures(actual.Pixels(), expected.Pixels(),
                                      "slice visibility preserves clipped italic overhang", 1);
            }
        }
    }
    std::cout << "PASS many offscreen colour slices and italic edge ink.\n";
}

void CheckByteStyleRanges() {
    Environment env;
    const auto text = Bytes(u8"Aא😀B");
    DirectWriteParagraph paragraph(env.factory.Get(), text, env.format.Get(), {}, {}, Options(false));
    CheckHR(paragraph.Status(), "UTF-8 byte-style fixture");
    // These fixed offsets include the interior of a two-byte scalar and a
    // four-byte scalar. A byte segment ending inside one covers no UTF-16 text.
    constexpr std::array<UINT32, 9> positions{0, 1, 1, 2, 2, 2, 2, 4, 5};
    for (size_t index = 0; index < positions.size(); ++index)
        Require(paragraph.Utf16Position(index) == positions[index], "Byte-style endpoints floor to scalar boundaries");
    RendererTarget actual(env), expected(env);
    ComPtr<ID2D1SolidColorBrush> red, blue;
    CheckHR(actual.target->CreateSolidColorBrush(D2D1::ColorF(D2D1::ColorF::Red), &red), "Byte-style red brush");
    CheckHR(actual.target->CreateSolidColorBrush(D2D1::ColorF(D2D1::ColorF::Blue), &blue), "Byte-style blue brush");
    std::vector<ParagraphBrushRun> raw, visible;
    for (size_t index = 0; index < text.size(); ++index) {
        const auto start = paragraph.Utf16Position(index);
        const auto end = paragraph.Utf16Position(index + 1);
        const ParagraphBrushRun run{{start, end - start}, index % 2 ? blue.Get() : red.Get()};
        raw.push_back(run);
        if (run.text.length) visible.push_back(run);
    }
    Require(raw.size() == 8 && visible.size() == 4, "Only four byte styles cover actual UTF-16 scalars");
    const auto origin = D2D1::Point2F(12, 10);
    const auto clip = D2D1::RectF(0, 0, 400, 100);
    const auto paint = [&](std::span<const ParagraphBrushRun> foreground) {
        actual.target->BeginDraw();
        actual.target->Clear(D2D1::ColorF(D2D1::ColorF::White));
        actual.target->PushAxisAlignedClip(clip, D2D1_ANTIALIAS_MODE_ALIASED);
        const auto result = DrawParagraphTextClipped(env.factory.Get(), actual.target.Get(), paragraph.Layout(),
            origin, clip, actual.brush.Get(), D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT, nullptr, foreground);
        actual.target->PopAxisAlignedClip();
        CheckHR(actual.target->EndDraw(), "End UTF-8 byte-style drawing");
        return result;
    };
    Require(paint(raw) == E_INVALIDARG, "Raw per-byte styles expose the strict zero-length range contract");
    const auto blank = actual.Pixels();
    Require(std::all_of(blank.begin(), blank.end(), [](auto channel) { return channel == 255; }),
        "Invalid byte styles fail before any pixels");
    CheckHR(paint(visible), "Nonempty scalar spans preserve valid byte-style coverage");
    auto reference = Reference(env, text, Options(false));
    expected.Effect(reference.Get(), {0, 4}, D2D1::ColorF(D2D1::ColorF::Red));
    expected.Effect(reference.Get(), {4, 1}, D2D1::ColorF(D2D1::ColorF::Blue));
    CheckHR(expected.Paint(env, reference.Get(), false, origin, {&clip, 1}), "Independent scalar-colour reference");
    CheckRendererPictures(actual.Pixels(), expected.Pixels(), "Byte styles retain the same complete-scalar colours");
}

void CheckImmutableForeground() {
    Environment env;
    ComPtr<IDWriteTextFormat> font;
    CheckHR(env.factory->CreateTextFormat(L"Calibri", nullptr, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_STYLE_NORMAL,
                                          DWRITE_FONT_STRETCH_NORMAL, 36, L"en-US", &font),
            "font");
    env.format = font;
    struct Fixture {
        const char *name;
        std::u8string_view text;
        DWRITE_TEXT_RANGE input, expected;
        bool rtl, ligatures;
        bool legacyParity = true;
    };
    const Fixture fixtures[]{
        {"lam", u8"لا", {0, 1}, {0, 2}, true, true},
        {"alef", u8"لا", {1, 1}, {0, 0}, true, true},
        {"ffi-first", u8"office ffi", {1, 1}, {1, 3}, false, true},
        {"ffi-second", u8"office ffi", {2, 1}, {0, 0}, false, true},
        {"ffi-disabled", u8"office ffi", {1, 1}, {1, 1}, false, false},
        {"base", u8"a\u0301b", {0, 1}, {0, 2}, false, true},
        {"mark", u8"a\u0301b", {1, 1}, {0, 0}, false, true},
        {"arabic-base", u8"أُإِآ", {0, 1}, {0, 2}, true, true},
        {"arabic-mark", u8"أُإِآ", {1, 1}, {0, 0}, true, true},
        {"zwj-first", u8"👩‍💻X", {0, 2}, {0, 5}, false, true, false},
        {"zwj-middle", u8"👩‍💻X", {2, 1}, {0, 0}, false, true, false},
        {"surrogate-high", u8"😀X", {0, 1}, {0, 2}, false, true, false},
        {"surrogate-low", u8"😀X", {1, 1}, {0, 0}, false, true, false},
        {"hebrew-middle", u8"אבג XYZ", {1, 1}, {1, 1}, true, true},
        {"mixed", u8"abc אבג def", {2, 4}, {2, 4}, false, true},
    };
    size_t cases = 0;
    for (const auto &fixture : fixtures)
        for (const float dpi : {96.f, 144.f, 192.f})
            for (const int gdi : {0, 1, 2})
                for (const int affine : {0, 1, 2, 3}) {
                    const auto label = std::string(fixture.name) + ",dpi=" + std::to_string(dpi) +
                                       ",gdi=" + std::to_string(gdi) + ",affine=" + std::to_string(affine);
                    const auto transform = ProbeTransform(affine);
                    auto options = Options(fixture.rtl);
                    options.width = 280;
                    options.lineHeight = 48;
                    options.baseline = 38;
                    const auto text = Bytes(fixture.text);
                    auto original = ForegroundFixture(env, text, options, dpi, gdi, fixture.ligatures);
                    auto reference = ForegroundFixture(env, text, options, dpi, gdi, fixture.ligatures);
                    const auto origin = D2D1::Point2F(10.25f, 10.5f);
                    const auto clip = D2D1::RectF(0, 0, 320, 100);
                    RendererTarget actual(env, dpi, transform), expected(env, dpi, transform);
                    ComPtr<ID2D1SolidColorBrush> red;
                    CheckHR(actual.target->CreateSolidColorBrush(D2D1::ColorF(D2D1::ColorF::Red), &red),
                            "override red");
                    const ParagraphBrushRun overrideRun{fixture.input, red.Get()};
                    if (fixture.expected.length)
                        expected.Effect(reference.Get(), fixture.expected, D2D1::ColorF(D2D1::ColorF::Red));
                    CheckHR(expected.Paint(env, reference.Get(), false, origin, {&clip, 1}),
                            "original Windows fixed-cluster reference");
                    const auto rowsBefore = ReferenceRows(original.Get());
                    const auto rectsBefore =
                        ReferenceRectangles(original.Get(), 0, static_cast<UINT32>(Wide(text).size()));
                    for (const bool legacy : {false, true}) {
                        // Current system emoji uses COLRv1. Context4 intentionally has
                        // a different format choice; private v0/SVG/PNG fixtures below
                        // prove legacy parity without claiming equivalent v1 pixels.
                        if (legacy && !fixture.legacyParity) continue;
                        LegacyTarget facade(actual.target.Get());
                        actual.target->BeginDraw();
                        actual.target->Clear(D2D1::ColorF(D2D1::ColorF::White));
                        actual.target->PushAxisAlignedClip(clip, D2D1_ANTIALIAS_MODE_ALIASED);
                        const auto drawn = DrawParagraphTextClipped(
                            env.factory.Get(), legacy ? &facade : actual.target.Get(), original.Get(), origin, clip,
                            actual.brush.Get(), D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT, nullptr, {&overrideRun, 1});
                        actual.target->PopAxisAlignedClip();
                        const auto flushed = actual.target->EndDraw();
                        CheckHR(drawn, label + ": painter");
                        CheckHR(flushed, label + ": flush");
                        CheckRendererPictures(actual.Pixels(), expected.Pixels(),
                                              label + ",legacy=" + std::to_string(legacy) +
                                                  ": immutable foreground system pixel reference");
                        const auto rowsAfter = ReferenceRows(original.Get());
                        const auto rectsAfter =
                            ReferenceRectangles(original.Get(), 0, static_cast<UINT32>(Wide(text).size()));
                        Require(rowsAfter.size() == rowsBefore.size() && rectsAfter.size() == rectsBefore.size(),
                                label + ": immutable row/selection geometry");
                        for (size_t i = 0; i < rectsBefore.size(); ++i)
                            Require(rectsBefore[i].left == rectsAfter[i].left &&
                                        rectsBefore[i].top == rectsAfter[i].top &&
                                        rectsBefore[i].width == rectsAfter[i].width,
                                    label + ": exact immutable visual rectangles");
                        for (UINT32 i = 0; i < Wide(text).size(); ++i) {
                            ComPtr<IUnknown> effect;
                            CheckHR(original->GetDrawingEffect(i, &effect), "inspect untouched effect");
                            Require(!effect, "no paint effects retained");
                        }
                        ++cases;
                    }
                }
    CheckOverlappingColorClusters();
    CheckWrappedBrushRows();
    CheckOverrideContracts();
    CheckOffscreenColoredOverhang();
    CheckByteStyleRanges();
    std::cout << "PASS: " << cases << " immutable foreground pixel/geometry cases plus overlapping color clusters.\n";
}
