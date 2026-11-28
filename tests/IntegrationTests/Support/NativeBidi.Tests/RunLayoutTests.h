// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

// Small contexts are exercised inside genuinely eligible production paragraphs,
// not through a test-only admission bypass. Compare just the target context while
// retaining complete paragraph context and an independent native layout.
std::string DenseRunFiller() {
    std::string prefix;
    const auto unit = Bytes(u8"ABC אבג 123 DEF ");
    while (prefix.size() < 24000) prefix += unit;
    return prefix;
}

void CheckAnalyzedLargeParagraph() {
    Environment env;
    CheckHR(env.factory->CreateTextFormat(L"Consolas", nullptr, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_STYLE_NORMAL,
        DWRITE_FONT_STRETCH_NORMAL, 36, L"en-US", env.format.ReleaseAndGetAddressOf()), "Large paragraph font");
    constexpr UINT32 length = 256 * 1024;
    const auto unit = Bytes(u8"ABC אבג مرحبا 123 😀 ");
    const auto unitLength = static_cast<UINT32>(Wide(unit).size());
    const UINT32 repeats = length / unitLength;
    std::string text;
    text.reserve(static_cast<size_t>(repeats) * unit.size() + unitLength);
    for (UINT32 i = 0; i < repeats; ++i) text += unit;
    text.append(length - repeats * unitLength, ' ');
    Require(Wide(text).size() == length, "Regular CI fixture contains exactly 256K UTF-16 units");
    auto options = Options(false);
    options.lineHeight = 48;
    options.baseline = 38;
    DirectWriteParagraph paragraph(env.factory.Get(), text, env.format.Get(), {}, {}, options);
    CheckHR(paragraph.Status(), "Regular CI large paragraph remains valid");
    Require(!paragraph.Layout() && !paragraph.GlyphRuns().empty() && paragraph.Lines().size() == 1,
        "Regular CI large paragraph must exercise the analyzed single-row backend");
    const auto reference = Reference(env, text, options);
    const auto sameCoordinate = [](float actual, float expected, const char *label) {
        const float ulp = std::abs(std::nextafter(expected, std::numeric_limits<float>::infinity()) - expected);
        Require(std::abs(actual - expected) <= std::max(0.05f, ulp), label);
    };
    const auto checkCaret = [&](ParagraphCaret actual, size_t byte, UINT32 position, bool trailing) {
        Require(actual.position == byte, "Large paragraph navigation retains the independent logical position: actual=" +
            std::to_string(actual.position) + "/" + std::to_string(actual.trailing) + ", expected=" +
            std::to_string(byte) + "/" + std::to_string(trailing));
        FLOAT x{}, y{};
        DWRITE_HIT_TEST_METRICS hit{};
        CheckHR(reference->HitTestTextPosition(trailing && position ? position - 1 : position, trailing && position,
            &x, &y, &hit), "Independent large paragraph caret");
        const auto point = paragraph.CaretPoint(actual);
        sameCoordinate(point.x, x, "Large paragraph caret retains native visual affinity");
        Near(point.y, 0, "Large paragraph caret stays on its single display row");
    };
    RendererTarget picture(env), oracle(env);
    const auto clip = D2D1::RectF(0, 0, 800, 400);
    // Sample complete source units at the beginning, middle and end. The source
    // offsets and Hebrew visual order are fixed independently of backend maps.
    // Reuse one native reference; never traverse all distant prefixes or arrows.
    for (const UINT32 repeat : {UINT32{0}, repeats / 2, repeats - 1}) {
        const UINT32 first = repeat * unitLength + 4;
        const size_t byte = static_cast<size_t>(repeat) * unit.size() + 4;
        for (const bool trailing : {false, true}) checkCaret({byte, trailing}, byte, first, trailing);
        // This builds the complete lazy navigation index and queries its distant
        // stops in both directions, including the two visual homes at run edges.
        for (const int direction : {-1, 1}) {
            auto caret = ParagraphCaret{byte + (direction > 0 ? 6 : 0), direction > 0};
            for (UINT32 step = 1; step <= 3; ++step) {
                const auto next = paragraph.VisualNeighbor(caret, direction);
                Require(next.has_value(), "Large paragraph visual navigation reaches every Hebrew cluster");
                Require((paragraph.CaretPoint(*next).x - paragraph.CaretPoint(caret).x) * direction > 0.001f,
                    "Large paragraph visual navigation makes visible directional progress");
                const UINT32 index = direction > 0 ? 3 - step : step;
                checkCaret(*next, byte + index * 2, first + index, direction < 0);
                caret = *next;
            }
        }
        auto ranges = paragraph.RangeRectangles(byte, static_cast<size_t>(repeat + 1) * unit.size());
        auto expected = ReferenceRectangles(reference.Get(), first, unitLength - 4);
        std::sort(ranges.begin(), ranges.end(), [](const auto &a, const auto &b) { return a.start < b.start; });
        std::sort(expected.begin(), expected.end(), [](const auto &a, const auto &b) { return a.textPosition < b.textPosition; });
        Require(!expected.empty() && ranges.size() == expected.size(), "Large paragraph selection retains native interval count");
        for (size_t i = 0; i < ranges.size(); ++i) {
            const auto &actual = ranges[i];
            const auto &native = expected[i];
            // Only a bounded source unit is decoded to map native UTF-16 spans;
            // a broken production byte map cannot provide its own expected value.
            const auto localStart = actual.start - static_cast<size_t>(repeat) * unit.size();
            Require(localStart <= unit.size() && actual.length <= unit.size() - localStart,
                "Large paragraph selection stays within its independent source unit");
            const auto prefix = std::string_view(unit).substr(0, localStart);
            const auto selected = std::string_view(unit).substr(localStart, actual.length);
            Require(repeat * unitLength + Wide(prefix).size() == native.textPosition &&
                Wide(selected).size() == native.length, "Large paragraph selection retains exact native logical spans");
            sameCoordinate(actual.left, native.left, "Large paragraph selection retains native left edge");
            sameCoordinate(actual.left + actual.width, native.left + native.width,
                "Large paragraph selection retains native right edge");
            Near(actual.top, 0, "Large paragraph selection stays on its display row");
            Near(actual.height, options.lineHeight, "Large paragraph selection retains uniform row height");
        }
        FLOAT x{}, y{};
        DWRITE_HIT_TEST_METRICS hit{};
        CheckHR(reference->HitTestTextPosition(first, FALSE, &x, &y, &hit), "Independent large paragraph pan anchor");
        const D2D1_POINT_2F origin{300 - x, 10};
        CheckHR(oracle.Paint(env, reference.Get(), false, origin, {&clip, 1}), "Independent large paragraph scrolled raster");
        picture.target->BeginDraw();
        picture.target->Clear(D2D1::ColorF(D2D1::ColorF::White));
        picture.target->PushAxisAlignedClip(clip, D2D1_ANTIALIAS_MODE_ALIASED);
        const auto drawn = DrawParagraphGlyphRunsClipped(env.factory.Get(), picture.target.Get(), paragraph.GlyphRuns(),
            options.baseline, origin, clip, picture.brush.Get(), D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT);
        picture.target->PopAxisAlignedClip();
        CheckHR(drawn, "Regular CI large paragraph retained-glyph rendering");
        CheckHR(picture.target->EndDraw(), "Finish regular CI large paragraph raster");
        CheckRendererPictures(picture.Pixels(), oracle.Pixels(), "Large paragraph distant pixels match Windows");
    }
    checkCaret({text.size(), false}, text.size(), length, false);
    std::cout << "PASS: regular CI 256K analyzed paragraph navigation, selection and distant pixels match Windows.\n";
}

void CheckAnalyzedStress() {
    Environment env;
    CheckHR(env.factory->CreateTextFormat(L"Consolas", nullptr, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_STYLE_NORMAL,
        DWRITE_FONT_STRETCH_NORMAL, 36, L"en-US", env.format.ReleaseAndGetAddressOf()), "Stress geometry font");
    std::string text;
    const auto unit = Bytes(u8"ABC אבג مرحبا 123 😀 ");
    const auto unitLength = Wide(unit).size();
    while ((text.size() / unit.size() + 1) * unitLength <= 262144) text += unit;
    const auto wide = Wide(text);
    const auto boundaries = Boundaries(text);
    for (const bool rtl : {false, true}) {
        auto options = Options(rtl);
        options.lineHeight = 48;
        options.baseline = 38;
        DirectWriteParagraph paragraph(env.factory.Get(), text, env.format.Get(), {}, {}, options);
        CheckHR(paragraph.Status(), "256K stress paragraph remains valid");
        const auto reference = Reference(env, text, options);
        const auto sameCoordinate = [](float actual, float expected) {
            const float ulp = std::abs(std::nextafter(expected, std::numeric_limits<float>::infinity()) - expected);
            Require(std::abs(actual - expected) <= std::max(0.05f, ulp), "256K geometry matches native float resolution");
        };
        for (size_t sample = 0; sample <= 31; ++sample) {
            const size_t index = (boundaries.size() - 1) * sample / 31;
            const auto byte = boundaries[index];
            const UINT32 position = static_cast<UINT32>(Wide(std::string_view(text).substr(0, byte)).size());
            for (const bool trailing : {false, true}) {
                FLOAT x{}, y{};
                DWRITE_HIT_TEST_METRICS hit{};
                CheckHR(reference->HitTestTextPosition(trailing && position ? position - 1 : position, trailing && position,
                    &x, &y, &hit), "Independent 256K caret");
                sameCoordinate(paragraph.CaretPoint({byte, trailing}).x, x);
            }
        }
        const auto clip = D2D1::RectF(0, 0, 800, 400);
        for (const UINT32 position : {UINT32{0}, static_cast<UINT32>(wide.size() / 2), static_cast<UINT32>(wide.size() - 1)}) {
            FLOAT x{}, y{};
            DWRITE_HIT_TEST_METRICS hit{};
            CheckHR(reference->HitTestTextPosition(position, FALSE, &x, &y, &hit), "Independent 256K pan anchor");
            const D2D1_POINT_2F origin{300 - x, 10};
            RendererTarget picture(env), oracle(env);
            CheckHR(oracle.Paint(env, reference.Get(), false, origin, {&clip, 1}), "Native 256K scrolled pixels");
            picture.target->BeginDraw();
            picture.target->Clear(D2D1::ColorF(D2D1::ColorF::White));
            const auto drawn = paragraph.Layout() ? DrawParagraphTextClipped(env.factory.Get(), picture.target.Get(), paragraph.Layout(),
                origin, clip, picture.brush.Get(), D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT) :
                DrawParagraphGlyphRunsClipped(env.factory.Get(), picture.target.Get(), paragraph.GlyphRuns(), options.baseline,
                    origin, clip, picture.brush.Get(), D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT);
            CheckHR(drawn, "256K scrolled facade rendering");
            CheckHR(picture.target->EndDraw(), "Finish 256K scrolled raster");
            CheckRendererPictures(picture.Pixels(), oracle.Pixels(), "256K distant glyphs match native pixels");
        }
        std::cout << "PASS: " << wide.size() << " UTF16 distant geometry/raster RTL=" << rtl << ", analyzed=" << (paragraph.Layout() == nullptr) << ".\n";
    }
}

void CheckAnalyzedFallback(Environment &env, std::string_view tail, ParagraphOptions options) {
    // Probe and dispatch the exact same eligible-length input. Adding context
    // may change eligibility, so the short fixture's disposition is not reused.
    const std::string text(tail);
    const auto wide = Wide(text);
    std::unique_ptr<DirectWriteRunLayout> probe;
    const auto status = DirectWriteRunLayout::TryCreate(env.factory.Get(), wide, env.format.Get(), {}, options, probe);
    Require(status == S_OK || status == S_FALSE, "Optional font/context eligibility has no resource failure");
    DirectWriteParagraph paragraph(env.factory.Get(), text, env.format.Get(), {}, {}, options);
    CheckHR(paragraph.Status(), "Optional input is published through the complete paragraph facade");
    Require((status == S_OK) == (paragraph.Layout() == nullptr), "Facade selects the backend proved eligible for the exact input");
    const auto reference = Reference(env, text, options);
    const auto boundaries = Boundaries(text);
    std::vector<size_t> samples(boundaries.begin(), boundaries.begin() + std::min<size_t>(64, boundaries.size()));
    for (size_t i = 0; i <= 32; ++i) samples.push_back(boundaries[(boundaries.size() - 1) * i / 32]);
    std::sort(samples.begin(), samples.end());
    samples.erase(std::unique(samples.begin(), samples.end()), samples.end());
    for (const auto byte : samples) {
        const auto position = paragraph.Utf16Position(byte);
        FLOAT x{}, y{};
        DWRITE_HIT_TEST_METRICS hit{};
        CheckHR(reference->HitTestTextPosition(position, FALSE, &x, &y, &hit), "Independent optional-context caret");
        Near(paragraph.CaretPoint({byte, false}).x, x, "Optional text retains native caret geometry");
    }
    FLOAT x{}, y{};
    DWRITE_HIT_TEST_METRICS hit{};
    CheckHR(reference->HitTestTextPosition(0, FALSE, &x, &y, &hit), "Optional-context pan anchor");
    RendererTarget picture(env), oracle(env);
    const auto clip = D2D1::RectF(0, 0, 800, 400);
    const D2D1_POINT_2F origin{300 - x, 10};
    CheckHR(oracle.Paint(env, reference.Get(), false, origin, {&clip, 1}), "Independent optional-context raster");
    picture.target->BeginDraw();
    picture.target->Clear(D2D1::ColorF(D2D1::ColorF::White));
    const auto drawn = paragraph.Layout() ? DrawParagraphTextClipped(env.factory.Get(), picture.target.Get(), paragraph.Layout(),
        origin, clip, picture.brush.Get(), D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT) :
        DrawParagraphGlyphRunsClipped(env.factory.Get(), picture.target.Get(), paragraph.GlyphRuns(), options.baseline,
            origin, clip, picture.brush.Get(), D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT);
    CheckHR(drawn, "Optional-context facade rendering");
    CheckHR(picture.target->EndDraw(), "Finish optional-context raster");
    Require(DifferentPixels(picture.Pixels(), oracle.Pixels()) == 0, "Optional font/context fallback retains native pixels");
}

// Inspect every independent native callback, not only the three visible pans.
// Matching font faces and shaped arrays prevents a coincidental raster pass.
class GlyphRunOracle final : public IDWriteTextRenderer {
    std::span<const ParagraphGlyphRun> expected;
    static bool SameFace(IDWriteFontFace *a, IDWriteFontFace *b) noexcept {
        if (a == b) return true;
        ComPtr<IDWriteFontFace3> first, second;
        ComPtr<IDWriteFontFaceReference> firstReference, secondReference;
        return SUCCEEDED(a->QueryInterface(IID_PPV_ARGS(&first))) && SUCCEEDED(b->QueryInterface(IID_PPV_ARGS(&second))) &&
            SUCCEEDED(first->GetFontFaceReference(&firstReference)) && SUCCEEDED(second->GetFontFaceReference(&secondReference)) &&
            firstReference->Equals(secondReference.Get());
    }
public:
    size_t calls = 0;
    bool equal = true;
    explicit GlyphRunOracle(std::span<const ParagraphGlyphRun> runs) : expected(runs) {}
    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID id, void **value) override {
        if (!value) return E_POINTER;
        *value = nullptr;
        if (id != __uuidof(IUnknown) && id != __uuidof(IDWriteTextRenderer) && id != __uuidof(IDWritePixelSnapping)) return E_NOINTERFACE;
        *value = static_cast<IDWriteTextRenderer *>(this);
        AddRef();
        return S_OK;
    }
    ULONG STDMETHODCALLTYPE AddRef() override { return 1; }
    ULONG STDMETHODCALLTYPE Release() override { return 1; }
    HRESULT STDMETHODCALLTYPE IsPixelSnappingDisabled(void *, BOOL *value) override { *value = FALSE; return S_OK; }
    HRESULT STDMETHODCALLTYPE GetCurrentTransform(void *, DWRITE_MATRIX *value) override { *value = {1, 0, 0, 1, 0, 0}; return S_OK; }
    HRESULT STDMETHODCALLTYPE GetPixelsPerDip(void *, FLOAT *value) override { *value = 1; return S_OK; }
    HRESULT STDMETHODCALLTYPE DrawGlyphRun(void *, FLOAT x, FLOAT, DWRITE_MEASURING_MODE mode,
        const DWRITE_GLYPH_RUN *run, const DWRITE_GLYPH_RUN_DESCRIPTION *description, IUnknown *) override {
        const auto index = calls++;
        if (index >= expected.size() || !description || !run->glyphAdvances || !run->glyphOffsets) { equal = false; return S_OK; }
        const auto &candidate = expected[index];
        const auto &glyphs = candidate.glyphs;
        equal &= mode == DWRITE_MEASURING_MODE_NATURAL && x == candidate.originX &&
            run->glyphCount == glyphs.glyphCount && run->bidiLevel == glyphs.bidiLevel && run->fontEmSize == glyphs.fontEmSize &&
            run->isSideways == glyphs.isSideways && description->textPosition == candidate.text.textPosition &&
            description->stringLength == candidate.text.stringLength && SameFace(run->fontFace, glyphs.fontFace);
        if (!equal) return S_OK;
        equal &= std::equal(description->string, description->string + description->stringLength, candidate.text.string) &&
            std::equal(description->clusterMap, description->clusterMap + description->stringLength, candidate.text.clusterMap) &&
            std::equal(run->glyphIndices, run->glyphIndices + run->glyphCount, glyphs.glyphIndices) &&
            std::equal(run->glyphAdvances, run->glyphAdvances + run->glyphCount, glyphs.glyphAdvances);
        for (UINT32 i = 0; i < run->glyphCount; ++i) equal &=
            run->glyphOffsets[i].advanceOffset == glyphs.glyphOffsets[i].advanceOffset &&
            run->glyphOffsets[i].ascenderOffset == glyphs.glyphOffsets[i].ascenderOffset;
        return S_OK;
    }
    HRESULT STDMETHODCALLTYPE DrawUnderline(void *, FLOAT, FLOAT, const DWRITE_UNDERLINE *, IUnknown *) override { return E_UNEXPECTED; }
    HRESULT STDMETHODCALLTYPE DrawStrikethrough(void *, FLOAT, FLOAT, const DWRITE_STRIKETHROUGH *, IUnknown *) override { return E_UNEXPECTED; }
    HRESULT STDMETHODCALLTYPE DrawInlineObject(void *, FLOAT, FLOAT, IDWriteInlineObject *, BOOL, BOOL, IUnknown *) override { return E_UNEXPECTED; }
};

void CheckDispatchPixels(Environment &env, const std::string &bytes, ParagraphOptions options, bool analyzed) {
    const auto wide = Wide(bytes);
    DirectWriteParagraph paragraph(env.factory.Get(), bytes, env.format.Get(), {}, {}, options);
    CheckHR(paragraph.Status(), "Admission regression paragraph");
    Require((paragraph.Layout() == nullptr) == analyzed, "Dispatch selects the supported and worthwhile backend (RTL=" + std::to_string(options.rtl) + ", width=" +
        std::to_string(options.width) + ", utf16=" + std::to_string(wide.size()) + ")");
    const auto reference = Reference(env, bytes, options);
    if (analyzed) {
        GlyphRunOracle oracle(paragraph.GlyphRuns());
        CheckHR(reference->Draw(nullptr, &oracle, 0, 0), "Independent full-paragraph glyph callbacks");
        Require(oracle.equal && oracle.calls == paragraph.GlyphRuns().size(),
            "Every analyzed origin, face, glyph, advance, offset and cluster map exactly matches native callbacks");
    }
    for (UINT32 i = 0; i <= 32; ++i) {
        UINT32 position = static_cast<UINT32>(static_cast<uint64_t>(wide.size()) * i / 32);
        if (position < wide.size() && wide[position] >= 0xdc00 && wide[position] <= 0xdfff) --position;
        for (const bool trailing : {false, true}) {
            const UINT32 queried = trailing && position ? position - 1 : position;
            FLOAT x{}, y{};
            DWRITE_HIT_TEST_METRICS metric{};
            CheckHR(reference->HitTestTextPosition(queried, trailing && position, &x, &y, &metric), "Independent dispatch caret");
            const auto point = paragraph.CaretPoint({paragraph.BytePosition(position), trailing});
            const float ulp = std::abs(std::nextafter(x, std::numeric_limits<float>::infinity()) - x);
            Require(std::abs(point.x - x) <= std::max(0.05f, ulp), "Dispatch retains native distant caret geometry");
        }
    }
    const auto clip = D2D1::RectF(0, 0, 800, 400);
    for (const UINT32 position : {UINT32{0}, static_cast<UINT32>(wide.size() / 2), static_cast<UINT32>(wide.size() - 1)}) {
        FLOAT x{}, y{};
        DWRITE_HIT_TEST_METRICS metric{};
        CheckHR(reference->HitTestTextPosition(position, FALSE, &x, &y, &metric), "Independent dispatch pan");
        const D2D1_POINT_2F origin{300 - x, 8};
        RendererTarget picture(env), oracle(env);
        CheckHR(oracle.Paint(env, reference.Get(), false, origin, {&clip, 1}), "Independent system dispatch raster");
        picture.target->BeginDraw();
        picture.target->Clear(D2D1::ColorF(D2D1::ColorF::White));
        const auto drawn = paragraph.Layout() ?
            DrawParagraphTextClipped(env.factory.Get(), picture.target.Get(), paragraph.Layout(), origin, clip,
                picture.brush.Get(), D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT) :
            DrawParagraphGlyphRunsClipped(env.factory.Get(), picture.target.Get(), paragraph.GlyphRuns(), options.baseline,
                origin, clip, picture.brush.Get(), D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT);
        CheckHR(drawn, "Dispatch facade rendering");
        CheckHR(picture.target->EndDraw(), "Finish dispatch raster");
        Require(DifferentPixels(picture.Pixels(), oracle.Pixels()) == 0, "Dispatch panned pixels match actual system DrawTextLayout");
    }
}

// Exercise the retained-glyph backend itself at real display scales; the
// short native-layout renderer tests do not dispatch through this path.
void CheckAnalyzedDpi() {
    Environment env;
    const auto bytes = Bytes(u8"gyp مرحبا ") + DenseRunFiller();
    const auto options = Options(false);
    DirectWriteParagraph paragraph(env.factory.Get(), bytes, env.format.Get(), {}, {}, options);
    CheckHR(paragraph.Status(), "Scaled analyzed paragraph");
    Require(!paragraph.Layout(), "DPI fixture must exercise retained glyph runs");
    const auto reference = Reference(env, bytes, options);
    const auto clip = D2D1::RectF(0, 0, 500, 200);
    const std::array<D2D1_MATRIX_3X2_F, 6> transforms{{
        {1, 0, 0, 1, 0, 0}, {1, 0, 0, 1, 0.375f, 0.625f},
        {1.125f, 0, 0, 0.875f, 0.375f, 0.625f}, {1, 0, 0, -1, 0, 80.625f},
        {0, 1, -1, 0, 150.375f, 0.625f}, {1, 0.1f, 0, 1, 0.375f, 0.625f}
    }};
    const auto unsnapped = D2D1_DRAW_TEXT_OPTIONS(D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT | D2D1_DRAW_TEXT_OPTIONS_NO_SNAP);
    for (const float dpi : {96.0f, 120.0f, 144.0f, 192.0f, 288.0f}) {
        for (size_t index = 0; index < transforms.size(); ++index) {
            RendererTarget actual(env, dpi, transforms[index]), expected(env, dpi, transforms[index]);
            for (const auto origin : {D2D1::Point2F(8, 10), D2D1::Point2F(8.25f, 10.5f), D2D1::Point2F(8, -26.5f)}) {
                const auto paint = [&](D2D1_DRAW_TEXT_OPTIONS drawOptions) {
                    actual.target->BeginDraw();
                    actual.target->Clear(D2D1::ColorF(D2D1::ColorF::White));
                    actual.target->PushAxisAlignedClip(clip, D2D1_ANTIALIAS_MODE_ALIASED);
                    CheckHR(DrawParagraphGlyphRunsClipped(env.factory.Get(), actual.target.Get(), paragraph.GlyphRuns(),
                        options.baseline, origin, clip, actual.brush.Get(), drawOptions), "Scaled retained glyph rendering");
                    actual.target->PopAxisAlignedClip();
                    CheckHR(actual.target->EndDraw(), "Finish scaled retained glyph rendering");
                    return actual.Pixels();
                };
                for (const auto drawOptions : {D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT, unsnapped}) {
                    CheckHR(expected.Paint(env, reference.Get(), false, origin, {&clip, 1}, drawOptions), "Independent scaled system raster");
                    CheckRendererPictures(paint(drawOptions), expected.Pixels(),
                        "Analyzed DPI/fractional origin matches Windows at " + std::to_string(dpi) +
                        " y=" + std::to_string(origin.y) + " transform=" + std::to_string(index) +
                        " options=" + std::to_string(drawOptions));
                    // Omitting only the retained baseline correction must fail
                    // against the normally snapped system raster, with equal clips.
                    if (dpi == 96 && index == 0 && origin.y == 10.5f && drawOptions != unsnapped)
                        Require(DifferentPixels(paint(unsnapped), expected.Pixels()) > 20,
                            "Fractional baseline negative control detects omitted snapping");
                }
            }
        }
    }
}

void CheckRunAdmission() {
    CheckAnalyzedDpi();
    Environment env;
    for (const float size : {18.0f, 19.8f, 36.0f, 36.125f, 54.0f}) {
        CheckHR(env.factory->CreateTextFormat(L"Consolas", nullptr, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL, size, L"en-US", env.format.ReleaseAndGetAddressOf()), "Admission geometry font");
        std::string dense;
        const auto denseUnit = Bytes(u8"ABC אבג مرحبا 123 😀 ");
        const auto denseUnitLength = Wide(denseUnit).size();
        while (dense.size() / denseUnit.size() * denseUnitLength < 16384) dense += denseUnit;
        for (const bool rtl : {false, true}) for (const float width : {550.0f, 1000000.0f}) {
            auto options = Options(rtl);
            options.width = width;
            options.lineHeight = 72;
            options.baseline = 58;
            CheckDispatchPixels(env, dense, options, !rtl);
        }
        for (const auto unit : {u8"a", u8"א"}) {
            std::string text;
            for (size_t i = 0; i < 16384; ++i) text += Bytes(unit);
            for (const bool rtl : {false, true}) for (const float width : {550.0f, 1000000.0f}) {
                auto options = Options(rtl);
                options.width = width;
                options.lineHeight = 72;
                options.baseline = 58;
                CheckDispatchPixels(env, text, options, false);
            }
        }
    }
    for (const size_t target : {size_t{32768}, size_t{262144}}) {
        std::string text;
        const auto unit = std::string(8000, 'a') + Bytes(u8" אבג ");
        while (Wide(text).size() < target) text += unit;
        for (const bool rtl : {false, true}) CheckDispatchPixels(env, text, Options(rtl), false);
    }
    std::string sameLevel;
    while (Wide(sameLevel).size() < 16384) sameLevel += Bytes(u8"אבג مرحב ");
    // Script contexts are short, but the continuous level-1 span is long.
    // Native BiDi segmentation can cut inside a script context as well.
    for (const bool rtl : {false, true}) for (const float width : {550.0f, 1000000.0f}) {
        auto options = Options(rtl);
        options.width = width;
        CheckDispatchPixels(env, sameLevel, options, false);
    }
    const auto prefix = DenseRunFiller();
    for (const auto tail : {Bytes(u8"\u200c"), Bytes(u8"س\u200cلام"), std::string(300, 'x') + Bytes(u8"😀")}) {
        for (const bool rtl : {false, true}) {
            const auto options = Options(rtl);
            std::unique_ptr<DirectWriteRunLayout> selected;
            const auto wide = Wide(prefix);
            if (rtl) {
                CheckDispatchPixels(env, prefix + tail, options, false);
                continue;
            }
            Require(DirectWriteRunLayout::TryCreate(env.factory.Get(), wide, env.format.Get(), {}, options, selected) == S_OK && selected,
                "Late-rejection negative controls start with an eligible dense paragraph");
            const auto rejected = Wide(prefix + tail);
            Require(DirectWriteRunLayout::TryCreate(env.factory.Get(), rejected, env.format.Get(), {}, options, selected) == S_FALSE && !selected,
                "Unsupported tail rejects the entire plan and publishes no stale geometry");
            CheckDispatchPixels(env, prefix + tail, options, false);
        }
    }
    std::cout << "PASS: long-run fonts/scales, sparse islands and unsupported tails retain native geometry/pixels.\n";
}

void CheckRunLayout(Environment &env) {
    CheckAnalyzedLargeParagraph();
    CheckRunAdmission();
    for (const size_t length : {16383U, 16384U, 16385U, 32768U}) {
        std::string text;
        const auto letter = Bytes(u8"א");
        for (size_t i = 0; i < length; ++i) text += letter;
        for (const bool rtl : {false, true}) {
            const auto options = Options(rtl);
            std::unique_ptr<DirectWriteRunLayout> direct;
            Require(DirectWriteRunLayout::TryCreate(env.factory.Get(), Wide(text), env.format.Get(), {}, options, direct) == S_FALSE && !direct,
                "Long homogeneous contexts retain native segmentation and origins");
            DirectWriteParagraph paragraph(env.factory.Get(), text, env.format.Get(), {}, {}, options);
            CheckHR(paragraph.Status(), "Long homogeneous paragraph remains valid");
            Require(paragraph.Layout() != nullptr, "Cheap homogeneous paragraphs always use native layout");
            const auto next = paragraph.VisualNeighbor({0, false}, -1);
            Require(next && next->position == letter.size(), "Long homogeneous paragraph retains its first visual arrow");
        }
    }
    const auto prefix = DenseRunFiller();
    size_t accepted = 0, fallback = 0;
    for (const auto fixture : {u8"abc", u8"אבג", u8"ABC אבג 123 DEF", u8"ABC אבג",
        u8"مرحبا بالعالم 123 ABC", u8"السَّلَامُ عَلَيْكُمْ لا",
        u8"אב😀ג e\u0301\u0323", u8"日本語 אבג 中文", u8"(אבג) [123] ABC", u8"אבג  ", u8"abc אבג\u00a0 ",
        u8"office AV ffi", u8"क्षेत्र हिन्दी العربية", u8"abc 👩‍💻 العربية",
        u8"س\u200cلام", u8"س\u200dلام", u8"ل\u200cا", u8"ل\u200dا", u8"\u200c", u8"\u200d",
        u8"\u200cabc", u8"abc\u200d", u8"❤\ufe0e", u8"❤\ufe0f", u8"\ufe0f", u8"abc\ufe0f", u8"一\U000e0100"}) {
        const auto bytes = Bytes(fixture) + prefix;
        const auto targetLength = static_cast<UINT32>(Wide(Bytes(fixture)).size());
        const auto wide = Wide(bytes);
        for (const bool rtl : {false, true}) {
            const auto options = Options(rtl);
            std::unique_ptr<DirectWriteRunLayout> actual;
            const auto result = DirectWriteRunLayout::TryCreate(env.factory.Get(), wide, env.format.Get(), {}, options, actual);
            if (result == S_FALSE) {
                Require((rtl || (fixture != std::u8string_view(u8"abc") && fixture != std::u8string_view(u8"אבג") &&
                    fixture != std::u8string_view(u8"ABC אבג 123 DEF"))) && !actual,
                    "Core default-font fixtures must use the analyzed backend");
                ++fallback;
                CheckAnalyzedFallback(env, bytes, options);
                continue;
            }
            Require(result == S_OK && actual, "Complete analyzed paragraph is supported: " + Bytes(fixture));
            ++accepted;
            const auto reference = Reference(env, bytes, options);
            DWRITE_TEXT_METRICS expected{};
            CheckHR(reference->GetMetrics(&expected), "Independent analyzed width");
            Near(actual->Width(), expected.widthIncludingTrailingWhitespace, "Analyzed width: " + Bytes(fixture));
            const auto nativeRows = ReferenceRows(reference.Get());
            Require(nativeRows.size() == 1, "Analyzed fixture has exactly one native row");
            UINT32 rowCount = 0;
            DWRITE_LINE_METRICS row{};
            CheckHR(reference->GetLineMetrics(&row, 1, &rowCount), "Independent analyzed trailing whitespace");
            Require(actual->TrailingWhitespace() == row.trailingWhitespaceLength, "Analyzed trailing whitespace matches native classification");
            for (UINT32 position = 0; position <= targetLength; ++position) {
                if (position < wide.size() && wide[position] >= 0xdc00 && wide[position] <= 0xdfff) continue;
                for (const bool trailing : {false, true}) {
                    const UINT32 queried = trailing && position ? position - 1 : position;
                    FLOAT x{}, y{};
                    DWRITE_HIT_TEST_METRICS metric{};
                    CheckHR(reference->HitTestTextPosition(queried, trailing && position, &x, &y, &metric), "Independent analyzed caret");
                    Near(actual->TextPosition(queried, trailing && position), x,
                        "Analyzed caret " + Bytes(fixture) + " at " + std::to_string(position) + "/" + std::to_string(trailing));
                }
            }
            FLOAT suffixX{}, suffixY{};
            DWRITE_HIT_TEST_METRICS suffixHit{};
            CheckHR(reference->HitTestTextPosition(0, FALSE, &suffixX, &suffixY, &suffixHit), "Target context pan anchor");
            const D2D1_POINT_2F origin{400 - suffixX, 4};
            for (float x = suffixX - 400; x < suffixX + 400; x += 3.25f) {
                BOOL trailing{}, inside{};
                DWRITE_HIT_TEST_METRICS metric{};
                CheckHR(reference->HitTestPoint(x, options.lineHeight / 2, &trailing, &inside, &metric), "Independent analyzed hit");
                bool actualTrailing = false;
                const auto hit = actual->HitPoint(x, actualTrailing);
                Require(hit.textPosition == metric.textPosition && hit.length == metric.length && actualTrailing == (trailing != FALSE),
                    "Analyzed point hit " + Bytes(fixture) + " at " + std::to_string(x));
            }
            for (UINT32 position = 0; position < targetLength; ++position) {
                const float x = actual->TextPosition(position, false);
                BOOL trailing{}, inside{};
                DWRITE_HIT_TEST_METRICS metric{};
                CheckHR(reference->HitTestPoint(x, options.lineHeight / 2, &trailing, &inside, &metric), "Native exact-cluster-edge hit");
                bool actualTrailing = false;
                const auto hit = actual->HitPoint(x, actualTrailing);
                Require(hit.textPosition == metric.textPosition && hit.length == metric.length && actualTrailing == (trailing != FALSE),
                    "Exact and shared cluster edges retain native hit ownership: " + Bytes(fixture) + " at " + std::to_string(position));
            }
            for (UINT32 start = 0; start < targetLength; ++start) {
                if (wide[start] >= 0xdc00 && wide[start] <= 0xdfff) continue;
                for (const UINT32 length : {UINT32{1}, targetLength - start}) {
                    auto ranges = actual->Range(start, length);
                    auto expectedRanges = ReferenceRectangles(reference.Get(), start, length);
                    const auto logicalOrder = [](const DWRITE_HIT_TEST_METRICS &a, const DWRITE_HIT_TEST_METRICS &b) { return a.textPosition < b.textPosition; };
                    std::sort(ranges.begin(), ranges.end(), logicalOrder);
                    std::sort(expectedRanges.begin(), expectedRanges.end(), logicalOrder);
                    Require(ranges.size() == expectedRanges.size(), "Analyzed selection intervals " + Bytes(fixture));
                    for (size_t i = 0; i < ranges.size(); ++i) {
                        Require(ranges[i].textPosition == expectedRanges[i].textPosition &&
                            ranges[i].length == expectedRanges[i].length && ranges[i].bidiLevel == expectedRanges[i].bidiLevel,
                            "Analyzed selection retains exact logical spans and bidi levels");
                        const auto label = Bytes(fixture) + " at " + std::to_string(start) + "/" + std::to_string(length) +
                            " nativeSpan=" + std::to_string(expectedRanges[i].textPosition) + "/" + std::to_string(expectedRanges[i].length) +
                            " nativeWidth=" + std::to_string(expectedRanges[i].width);
                        Near(ranges[i].left, expectedRanges[i].left, "Analyzed range left " + label);
                        Near(ranges[i].width, expectedRanges[i].width, "Analyzed range width " + label);
                    }
                }
            }
            RendererTarget picture(env), oracle(env);
            const auto clip = D2D1::RectF(0, 0, 800, 400);
            CheckHR(oracle.Paint(env, reference.Get(), false, origin, {&clip, 1}, D2D1_DRAW_TEXT_OPTIONS_NONE), "Independent analyzed raster");
            picture.target->BeginDraw();
            picture.target->Clear(D2D1::ColorF(D2D1::ColorF::White));
            picture.target->PushAxisAlignedClip(clip, D2D1_ANTIALIAS_MODE_ALIASED);
            CheckHR(DrawParagraphGlyphRunsClipped(env.factory.Get(), picture.target.Get(), actual->GlyphRuns(),
                options.baseline, origin, clip, picture.brush.Get(), D2D1_DRAW_TEXT_OPTIONS_NONE), "Shared analyzed glyph renderer");
            picture.target->PopAxisAlignedClip();
            CheckHR(picture.target->EndDraw(), "Analyzed glyph raster");
            Require(DifferentPixels(picture.Pixels(), oracle.Pixels()) == 0, "Analyzed raster equals independent DirectWrite: " + Bytes(fixture));
        }
    }
    Require(accepted >= 6, "Core analyzed oracles cannot all fall back");
    std::cout << "PASS: analyzed short contexts " << accepted << " accepted in eligible dense paragraphs, " << fallback << " verified native fallbacks.\n";
    {
        Environment italic;
        CheckHR(italic.factory->CreateTextFormat(L"Segoe UI", nullptr, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_STYLE_ITALIC,
            DWRITE_FONT_STRETCH_NORMAL, 32, L"en-US", italic.format.ReleaseAndGetAddressOf()), "Italic compositing font");
        bool orderMatters = false;
        for (const auto fixture : {u8"aאf", u8"fאf", u8"fאבf", u8"אfב", u8"aאj", u8"aאJ"}) {
            const auto bytes = Bytes(fixture) + prefix;
            const auto targetLength = static_cast<UINT32>(Wide(Bytes(fixture)).size());
            const auto wide = Wide(bytes);
            auto options = Options(false);
            options.lineHeight = 48;
            options.baseline = 38;
            std::unique_ptr<DirectWriteRunLayout> actual;
            Require(DirectWriteRunLayout::TryCreate(italic.factory.Get(), wide, italic.format.Get(), {}, options, actual) == S_OK,
                "Italic mixed-direction compositing fixture is supported");
            const auto reference = Reference(italic, bytes, options);
            RendererTarget picture(italic), oracle(italic);
            ComPtr<ID2D1SolidColorBrush> red, expectedRed;
            CheckHR(picture.target->CreateSolidColorBrush(D2D1::ColorF(D2D1::ColorF::Red), &red), "Italic selected foreground");
            CheckHR(oracle.target->CreateSolidColorBrush(D2D1::ColorF(D2D1::ColorF::Red), &expectedRed), "Independent italic foreground");
            const UINT32 selected = targetLength - 1;
            CheckHR(reference->SetDrawingEffect(expectedRed.Get(), {selected, 1}), "Independent italic selection");
            const ParagraphBrushRun colors[]{{{0, selected}, picture.brush.Get()}, {{selected, 1}, red.Get()},
                {{selected + 1, static_cast<UINT32>(wide.size()) - selected - 1}, picture.brush.Get()}};
            const auto clip = D2D1::RectF(0, 0, 800, 400);
            FLOAT targetX{}, targetY{};
            DWRITE_HIT_TEST_METRICS targetHit{};
            CheckHR(reference->HitTestTextPosition(selected, FALSE, &targetX, &targetY, &targetHit), "Italic target pan anchor");
            const D2D1_POINT_2F origin{400 - targetX, 0};
            CheckHR(oracle.Paint(italic, reference.Get(), false, origin, {&clip, 1}), "Native overlapping ink raster");
            const auto paint = [&](std::span<const ParagraphGlyphRun> runs) {
                picture.target->BeginDraw();
                picture.target->Clear(D2D1::ColorF(D2D1::ColorF::White));
                CheckHR(DrawParagraphGlyphRunsClipped(italic.factory.Get(), picture.target.Get(), runs, options.baseline,
                    origin, clip, picture.brush.Get(), D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT, colors), "Italic mixed glyph rendering");
                CheckHR(picture.target->EndDraw(), "Finish italic glyph raster");
                return picture.Pixels();
            };
            Require(DifferentPixels(paint(actual->GlyphRuns()), oracle.Pixels()) == 0, "Logical callback order preserves native overlapping ink");
            std::vector<ParagraphGlyphRun> reversed(actual->GlyphRuns().begin(), actual->GlyphRuns().end());
            std::reverse(reversed.begin(), reversed.end());
            orderMatters |= DifferentPixels(paint(reversed), oracle.Pixels()) != 0;
        }
        Require(orderMatters, "Reversing actual mixed-direction callbacks changes overlapping colored pixels");
    }
    for (const auto fixture : {u8"a\tb", u8"a\u200eb", u8"a\u200fb", u8"a\u061cb", u8"\u2067אבג\u2069",
        u8"a\u2028b", u8"a\u2029b", u8"a\u0085b", u8"\u200b", u8"\u00ad", u8"\u0001"}) {
        const auto wide = Wide(prefix + Bytes(fixture));
        std::unique_ptr<DirectWriteRunLayout> unsupported;
        const auto hr = DirectWriteRunLayout::TryCreate(env.factory.Get(), wide, env.format.Get(), {}, Options(false), unsupported);
        Require(hr == S_FALSE && !unsupported, "Unsupported paragraph remains on the native layout before publication");
    }
    {
        const auto options = Options(false, ParagraphWrap::Word);
        std::unique_ptr<DirectWriteRunLayout> unsupported;
        Require(DirectWriteRunLayout::TryCreate(env.factory.Get(), L"abc אבג", env.format.Get(), {}, options, unsupported) == S_FALSE,
            "Wrapped paragraphs retain the accepted line-aware native shaper");
    }
    {
        // The emoji fallback cuts a single Latin analyzer context. Certifying
        // it would exceed the bounded full-context proof, so no partial layout
        // may escape. A short equivalent is covered by the positive oracles.
        const std::wstring text = Wide(prefix) + std::wstring(200, L'x') + L"😀" + std::wstring(200, L'y');
        std::unique_ptr<DirectWriteRunLayout> unsupported;
        Require(DirectWriteRunLayout::TryCreate(env.factory.Get(), text, env.format.Get(), {}, Options(false), unsupported) == S_FALSE && !unsupported,
            "Oversized cross-font shaping contexts retain native layout before publication");
    }
    {
        const auto bytes = prefix + Bytes(u8"a אבג DEF مرحبا b");
        const auto wide = Wide(bytes);
        ComPtr<IDWriteTextFormat> styled;
        CheckHR(env.factory->CreateTextFormat(L"Consolas", nullptr, DWRITE_FONT_WEIGHT_BOLD, DWRITE_FONT_STYLE_ITALIC,
            DWRITE_FONT_STRETCH_NORMAL, 32, L"en-US", &styled), "Create independent mixed-format font");
        const ParagraphFormatRange span{2, 7, styled.Get()};
        for (const bool rtl : {false, true}) {
            const auto options = Options(rtl);
            const auto reference = Reference(env, bytes, options);
            CheckHR(reference->SetFontFamilyName(L"Consolas", {2, 7}), "Reference mixed font family");
            CheckHR(reference->SetFontSize(32, {2, 7}), "Reference mixed font size");
            CheckHR(reference->SetFontWeight(DWRITE_FONT_WEIGHT_BOLD, {2, 7}), "Reference mixed font weight");
            CheckHR(reference->SetFontStyle(DWRITE_FONT_STYLE_ITALIC, {2, 7}), "Reference mixed font style");
            std::unique_ptr<DirectWriteRunLayout> actual;
            Require(DirectWriteRunLayout::TryCreate(env.factory.Get(), wide, env.format.Get(), {&span, 1}, options, actual) == S_FALSE && !actual,
                "Actual font changes retain native cross-style shaping context");
        }
        CheckHR(env.factory->CreateTextFormat(L"Notepads missing font family fixture", nullptr, DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, 24, L"en-US", &styled), "Create missing-family format");
        std::unique_ptr<DirectWriteRunLayout> unsupported;
        Require(DirectWriteRunLayout::TryCreate(env.factory.Get(), wide, styled.Get(), {}, Options(false), unsupported) == S_FALSE,
            "Missing requested families retain the native substitution policy");
    }
    {
        const auto bytes = Bytes(u8"السَّلَامُ عَلَيْكُمْ لا") + prefix;
        const auto wide = Wide(bytes);
        ComPtr<IDWriteTextFormat> equivalent;
        CheckHR(env.factory->CreateTextFormat(L"Segoe UI", nullptr, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL, 24, L"en-US", &equivalent), "Create equivalent shaping style");
        std::vector<ParagraphFormatRange> styles;
        for (UINT32 i = 0; i < wide.size(); ++i) styles.push_back({i, 1, i % 2 ? equivalent.Get() : env.format.Get()});
        for (const bool rtl : {false, true}) {
            std::unique_ptr<DirectWriteRunLayout> actual;
            const auto result = DirectWriteRunLayout::TryCreate(env.factory.Get(), wide, env.format.Get(), styles, Options(rtl), actual);
            if (rtl) {
                Require(result == S_FALSE && !actual, "RTL-base equivalent styles retain native alignment");
                CheckAnalyzedFallback(env, bytes, Options(rtl));
                continue;
            }
            Require(result == S_OK && actual, "Equivalent style identities do not split Arabic shaping");
            const auto reference = Reference(env, bytes, Options(rtl));
            for (UINT32 i = 0; i <= Wide(Bytes(u8"السَّلَامُ عَلَيْكُمْ لا")).size(); ++i) {
                FLOAT x{}, y{};
                DWRITE_HIT_TEST_METRICS hit{};
                CheckHR(reference->HitTestTextPosition(i, FALSE, &x, &y, &hit), "Equivalent-style independent caret");
                Near(actual->TextPosition(i, false), x, "Equivalent styles retain Arabic joining and marks");
            }
        }
    }
    for (const auto pattern : {u8"ABC אבג 123 DEF ", u8"ABC אבג مرحبا 123 😀 ", u8"office السَّلَامُ عَلَيْكُمْ لا e\u0301\u0323 👩‍💻 "}) {
        std::string longText;
        const auto unit = Bytes(pattern);
        while (longText.size() < 45000) longText += unit;
        const auto wide = Wide(longText);
        for (const bool rtl : {false, true}) {
            const auto options = Options(rtl);
            std::unique_ptr<DirectWriteRunLayout> probe;
            const auto eligibility = DirectWriteRunLayout::TryCreate(env.factory.Get(), wide, env.format.Get(), {}, options, probe);
            Require(eligibility == S_OK || eligibility == S_FALSE, "Exact long fixture eligibility has no resource failure");
            if (!rtl && pattern == std::u8string_view(u8"ABC אבג 123 DEF "))
                Require(eligibility == S_OK, "Core long paragraph must select the analyzed backend");
            probe.reset();
            DirectWriteParagraph paragraph(env.factory.Get(), longText, env.format.Get(), {}, {}, options);
            CheckHR(paragraph.Status(), "Automatic long paragraph backend");
            Require((eligibility == S_OK) == (paragraph.Layout() == nullptr), "Automatic long dispatch matches exact-input eligibility");
            const auto reference = Reference(env, longText, options);
            DWRITE_TEXT_METRICS metrics{};
            CheckHR(reference->GetMetrics(&metrics), "Independent long paragraph width");
            // Both APIs expose float coordinates; one ULP is the representational
            // resolution at a distant position, not a relaxed pixel-error budget.
            const auto sameCoordinate = [](float actual, float expected, const char *label) {
                const float ulp = std::abs(std::nextafter(expected, std::numeric_limits<float>::infinity()) - expected);
                Require(std::abs(actual - expected) <= std::max(0.05f, ulp),
                    std::string(label) + ": actual=" + std::to_string(actual) + ", expected=" + std::to_string(expected));
            };
            sameCoordinate(paragraph.WidthIncludingTrailingWhitespace(), metrics.widthIncludingTrailingWhitespace, "Long intrinsic width");
            const auto boundaries = Boundaries(longText);
            for (size_t index = 0; index < boundaries.size(); index += std::max<size_t>(1, boundaries.size() / 53)) {
                const auto byte = boundaries[index];
                const auto position = static_cast<UINT32>(Wide(std::string_view(longText).substr(0, byte)).size());
                for (const bool trailing : {false, true}) {
                    FLOAT x{}, y{};
                    DWRITE_HIT_TEST_METRICS hit{};
                    CheckHR(reference->HitTestTextPosition(trailing && position ? position - 1 : position, trailing && position,
                        &x, &y, &hit), "Independent distant caret");
                    const auto point = paragraph.CaretPoint({byte, trailing});
                    sameCoordinate(point.x, x, ("Distant caret rtl=" + std::to_string(rtl) + ",position=" + std::to_string(position) +
                        ",trailing=" + std::to_string(trailing)).c_str());
                    const size_t endIndex = std::min(index + 17, boundaries.size() - 1);
                    auto ranges = paragraph.RangeRectangles(byte, boundaries[endIndex]);
                    const UINT32 end = static_cast<UINT32>(Wide(std::string_view(longText).substr(0, boundaries[endIndex])).size());
                    auto expectedRanges = ReferenceRectangles(reference.Get(), position, end - position);
                    std::sort(ranges.begin(), ranges.end(), [](const auto &a, const auto &b) { return a.start < b.start; });
                    std::sort(expectedRanges.begin(), expectedRanges.end(), [](const auto &a, const auto &b) { return a.textPosition < b.textPosition; });
                    Require(ranges.size() == expectedRanges.size(), "Distant selection has exact native interval count");
                    for (size_t i = 0; i < ranges.size(); ++i) {
                        const auto &actual = ranges[i];
                        const auto &expected = expectedRanges[i];
                        Require(paragraph.Utf16Position(actual.start) == expected.textPosition &&
                            paragraph.Utf16Position(actual.start + actual.length) == expected.textPosition + expected.length,
                            "Distant selection retains native logical spans");
                        sameCoordinate(actual.left, expected.left, "Distant selection left");
                        // Subtracting two distant float coordinates limits interval
                        // width resolution to the ULP of its endpoints.
                        sameCoordinate(actual.left + actual.width, expected.left + expected.width, ("Distant selection right rtl=" + std::to_string(rtl) + ",start=" + std::to_string(position) + ",span=" + std::to_string(expected.textPosition) + "/" + std::to_string(expected.length) + ",widths=" + std::to_string(actual.width) + "/" + std::to_string(expected.width)).c_str());
                    }
                }
            }
            const auto clip = D2D1::RectF(0, 0, 800, 400);
            for (const UINT32 position : {UINT32{0}, static_cast<UINT32>(wide.size() / 2), static_cast<UINT32>(wide.size() - 1)}) {
                FLOAT x{}, y{};
                DWRITE_HIT_TEST_METRICS hit{};
                CheckHR(reference->HitTestTextPosition(position, FALSE, &x, &y, &hit), "Independent horizontal pan anchor");
                const D2D1_POINT_2F origin{300 - x, 10};
                RendererTarget picture(env), oracle(env);
                CheckHR(oracle.Paint(env, reference.Get(), false, origin, {&clip, 1}), "Independent scrolled color glyph raster");
                picture.target->BeginDraw();
                picture.target->Clear(D2D1::ColorF(D2D1::ColorF::White));
                picture.target->PushAxisAlignedClip(clip, D2D1_ANTIALIAS_MODE_ALIASED);
                const auto draw = paragraph.Layout() ? DrawParagraphTextClipped(env.factory.Get(), picture.target.Get(), paragraph.Layout(),
                    origin, clip, picture.brush.Get(), D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT) :
                    DrawParagraphGlyphRunsClipped(env.factory.Get(), picture.target.Get(), paragraph.GlyphRuns(), options.baseline,
                        origin, clip, picture.brush.Get(), D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT);
                CheckHR(draw, "Analyzed/native scrolled color glyph raster");
                picture.target->PopAxisAlignedClip();
                CheckHR(picture.target->EndDraw(), "Finish analyzed scrolled raster");
                Require(DifferentPixels(picture.Pixels(), oracle.Pixels()) == 0, "Distant scrolled color glyphs equal native paragraph pixels");
                ComPtr<ID2D1SolidColorBrush> red, expectedRed;
                CheckHR(picture.target->CreateSolidColorBrush(D2D1::ColorF(D2D1::ColorF::Red), &red), "Long selected foreground");
                CheckHR(oracle.target->CreateSolidColorBrush(D2D1::ColorF(D2D1::ColorF::Red), &expectedRed), "Independent selected foreground");
                const DWRITE_TEXT_RANGE selected{hit.textPosition, hit.length};
                CheckHR(reference->SetDrawingEffect(expectedRed.Get(), selected), "Independent complete-cluster selection");
                CheckHR(oracle.Paint(env, reference.Get(), false, origin, {&clip, 1}), "Independent scrolled selected raster");
                std::vector<ParagraphBrushRun> colors;
                if (selected.startPosition) colors.push_back({{0, selected.startPosition}, picture.brush.Get()});
                colors.push_back({selected, red.Get()});
                const UINT32 after = selected.startPosition + selected.length;
                if (after < wide.size()) colors.push_back({{after, static_cast<UINT32>(wide.size()) - after}, picture.brush.Get()});
                picture.target->BeginDraw();
                picture.target->Clear(D2D1::ColorF(D2D1::ColorF::White));
                picture.target->PushAxisAlignedClip(clip, D2D1_ANTIALIAS_MODE_ALIASED);
                const auto selectedDraw = paragraph.Layout() ? DrawParagraphTextClipped(env.factory.Get(), picture.target.Get(), paragraph.Layout(),
                    origin, clip, picture.brush.Get(), D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT, nullptr, colors) :
                    DrawParagraphGlyphRunsClipped(env.factory.Get(), picture.target.Get(), paragraph.GlyphRuns(), options.baseline,
                        origin, clip, picture.brush.Get(), D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT, colors);
                CheckHR(selectedDraw, "Long immutable selected foreground");
                picture.target->PopAxisAlignedClip();
                CheckHR(picture.target->EndDraw(), "Finish long selected raster");
                Require(DifferentPixels(picture.Pixels(), oracle.Pixels()) == 0, "Distant selected text has exact native pixels");
                CheckHR(reference->SetDrawingEffect(nullptr, {0, UINT32_MAX}), "Clear independent selection effect");
            }
        }
    }
}
