// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

struct LegacyFormatProbe final : IDWriteTextRenderer {
    DWRITE_GLYPH_IMAGE_FORMATS expected;
    ComPtr<IDWriteFactory4> factory;
    HRESULT failure = S_OK;
    size_t glyphs = 0;
    UINT16 observedGlyph = 0;
    DWRITE_GLYPH_IMAGE_FORMATS observedFormats{};
    DWRITE_GLYPH_IMAGE_FORMATS faceFormats{};
    std::string layerFormats;
    LegacyFormatProbe(DWRITE_GLYPH_IMAGE_FORMATS expected_, IDWriteFactory *factory_) : expected(expected_) {
        CheckHR(factory_->QueryInterface(IID_PPV_ARGS(&factory)), "Create independent color-format inspector");
    }
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
    HRESULT STDMETHODCALLTYPE DrawGlyphRun(void *, FLOAT x, FLOAT y, DWRITE_MEASURING_MODE mode,
        const DWRITE_GLYPH_RUN *run, const DWRITE_GLYPH_RUN_DESCRIPTION *description, IUnknown *) override {
        ComPtr<IDWriteFontFace4> face;
        auto result = run->fontFace->QueryInterface(IID_PPV_ARGS(&face));
        if (FAILED(result)) return failure = result;
        faceFormats = face->GetGlyphImageFormats();
        const void *table = nullptr;
        UINT32 size = 0;
        void *context = nullptr;
        BOOL exists = FALSE;
        result = face->TryGetFontTable(DWRITE_MAKE_OPENTYPE_TAG('C', 'O', 'L', 'R'), &table, &size, &context, &exists);
        if (FAILED(result)) return failure = result;
        const auto bytes = static_cast<const unsigned char *>(table);
        const bool versionZero = exists && size >= 2 && bytes[0] == 0 && bytes[1] == 0;
        const bool legacyOnly = !exists || versionZero;
        if (exists) face->ReleaseFontTable(context);
        if (!legacyOnly || (expected == DWRITE_GLYPH_IMAGE_FORMATS_COLR && !versionZero)) return failure = E_UNEXPECTED;
        for (UINT32 index = 0; index < run->glyphCount; ++index) {
            observedGlyph = run->glyphIndices[index];
            if (run->glyphIndices[index] == 1) continue; // Our deliberately blank space glyph.
            if (run->glyphIndices[index] != 2) return failure = E_UNEXPECTED;
            DWRITE_GLYPH_IMAGE_FORMATS formats{};
            // The per-glyph image API describes image payloads, not COLRv0
            // layers. Current DirectWrite can advertise a synthesized paint
            // tree for a genuine version-zero table, hence the raw check above.
            result = face->GetGlyphImageFormats(run->glyphIndices[index], 32, 64, &formats);
            if (expected == DWRITE_GLYPH_IMAGE_FORMATS_COLR) formats = faceFormats;
            observedFormats = formats;
            if (FAILED(result)) return failure = result;
            if (!(formats & expected)) return failure = E_UNEXPECTED;
            ++glyphs;
        }
        constexpr auto legacyFormats = static_cast<DWRITE_GLYPH_IMAGE_FORMATS>(
            DWRITE_GLYPH_IMAGE_FORMATS_TRUETYPE | DWRITE_GLYPH_IMAGE_FORMATS_CFF | DWRITE_GLYPH_IMAGE_FORMATS_COLR |
            DWRITE_GLYPH_IMAGE_FORMATS_SVG | DWRITE_GLYPH_IMAGE_FORMATS_PNG | DWRITE_GLYPH_IMAGE_FORMATS_JPEG |
            DWRITE_GLYPH_IMAGE_FORMATS_TIFF | DWRITE_GLYPH_IMAGE_FORMATS_PREMULTIPLIED_B8G8R8A8);
        ComPtr<IDWriteColorGlyphRunEnumerator1> layers;
        result = factory->TranslateColorGlyphRun({x, y}, run, description, legacyFormats, mode, nullptr, 0, &layers);
        if (result == DWRITE_E_NOCOLOR) return S_OK;
        if (FAILED(result)) return failure = result;
        for (;;) {
            BOOL more = FALSE;
            result = layers->MoveNext(&more);
            if (FAILED(result)) return failure = result;
            if (!more) break;
            const DWRITE_COLOR_GLYPH_RUN1 *layer = nullptr;
            result = layers->GetCurrentRun(&layer);
            if (FAILED(result)) return failure = result;
            layerFormats += std::to_string(layer->glyphImageFormat) + ',';
        }
        return S_OK;
    }
    HRESULT STDMETHODCALLTYPE DrawUnderline(void *, FLOAT, FLOAT, const DWRITE_UNDERLINE *, IUnknown *) override { return failure = E_NOTIMPL; }
    HRESULT STDMETHODCALLTYPE DrawStrikethrough(void *, FLOAT, FLOAT, const DWRITE_STRIKETHROUGH *, IUnknown *) override { return failure = E_NOTIMPL; }
    HRESULT STDMETHODCALLTYPE DrawInlineObject(void *, FLOAT, FLOAT, IDWriteInlineObject *, BOOL, BOOL, IUnknown *) override { return failure = E_NOTIMPL; }
};

void LoadPrivateFont(Environment &env, const wchar_t *file, const wchar_t *family) {
    wchar_t executable[MAX_PATH]{};
    const auto length = GetModuleFileNameW(nullptr, executable, MAX_PATH);
    Require(length > 0 && length < MAX_PATH, "Find the native test executable for private font fixtures");
    std::wstring path(executable, length);
    path.resize(path.find_last_of(L"\\/") + 1);
    path += L"Fonts\\";
    path += file;
    ComPtr<IDWriteFactory3> factory;
    CheckHR(env.factory.As(&factory), "Private font collections are available on the minimum supported Windows");
    ComPtr<IDWriteFontFaceReference> face;
    CheckHR(factory->CreateFontFaceReference(path.c_str(), nullptr, 0, DWRITE_FONT_SIMULATIONS_NONE, &face), "Open original test font privately");
    ComPtr<IDWriteFontSetBuilder> builder;
    CheckHR(factory->CreateFontSetBuilder(&builder), "Create private test font set builder");
    CheckHR(builder->AddFontFaceReference(face.Get()), "Add original font face to the private collection");
    ComPtr<IDWriteFontSet> set;
    CheckHR(builder->CreateFontSet(&set), "Build private font set");
    ComPtr<IDWriteFontCollection1> collection;
    CheckHR(factory->CreateFontCollectionFromFontSet(set.Get(), &collection), "Create private font collection");
    env.format.Reset();
    CheckHR(factory->CreateTextFormat(family, collection.Get(), DWRITE_FONT_WEIGHT_NORMAL,
        DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, 32, L"en-US", &env.format), "Select actual private color font");
}

void CheckLegacyColorFonts() {
    struct Fixture { const wchar_t *file; const wchar_t *family; DWRITE_GLYPH_IMAGE_FORMATS format; const char *name; };
    const Fixture fixtures[]{
        {L"LegacyColr.ttf", L"Notepads Test COLRv0", DWRITE_GLYPH_IMAGE_FORMATS_COLR, "COLRv0"},
        {L"LegacySvg.ttf", L"Notepads Test SVG", DWRITE_GLYPH_IMAGE_FORMATS_SVG, "SVG"},
        {L"LegacyBitmap.ttf", L"Notepads Test Bitmap", DWRITE_GLYPH_IMAGE_FORMATS_PNG, "PNG bitmap"},
    };
    for (const auto &fixture : fixtures) {
        Environment env;
        LoadPrivateFont(env, fixture.file, fixture.family);
        auto options = Options(false);
        options.lineHeight = 40;
        options.baseline = 32;
        auto actualLayout = Reference(env, Bytes(u8"\uE000 \uE000 \uE000"), options);
        auto expectedLayout = Reference(env, Bytes(u8"\uE000 \uE000 \uE000"), options);
        LegacyFormatProbe probe(fixture.format, env.factory.Get());
        CheckHR(expectedLayout->Draw(nullptr, &probe, 0, 0), "Inspect actual chosen legacy fixture glyphs");
        CheckHR(probe.failure, std::string(fixture.name) + ": actual font format is present and has no COLRv1 representation; glyph=" +
            std::to_string(probe.observedGlyph) + ", formats=" + std::to_string(probe.observedFormats) +
            ", faceFormats=" + std::to_string(probe.faceFormats));
        Require(probe.glyphs == 3, "All three private-use characters resolve to the intended original color glyph");
        for (const float dpi : {96.0f, 192.0f}) {
            const auto transform = D2D1::Matrix3x2F::Translation(37.375f, 17.625f);
            RendererTarget actual(env, dpi, transform), expected(env, dpi, transform);
            const auto origin = D2D1::Point2F(10.25f, 10.5f);
            const auto clip = D2D1::RectF(0, 0, 260, 100);
            for (const float opacity : {1.0f, 0.5f}) {
                actual.brush->SetOpacity(opacity);
                expected.brush->SetOpacity(opacity);
                std::array<std::vector<unsigned char>, 2> foregroundControls;
                size_t colorIndex = 0;
                for (const auto color : {D2D1::ColorF(D2D1::ColorF::Blue), D2D1::ColorF(D2D1::ColorF::Green)}) {
                    actual.brush->SetColor(color);
                    expected.brush->SetColor(color);
                    size_t snapIndex = 0;
                    for (const auto snapping : {D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_DRAW_TEXT_OPTIONS_DISABLE_COLOR_BITMAP_SNAPPING}) {
                        const auto drawOptions = static_cast<D2D1_DRAW_TEXT_OPTIONS>(D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT | snapping);
                        CheckHR(expected.Paint(env, expectedLayout.Get(), false, origin, {&clip, 1}, drawOptions), "Draw original Windows color-font reference");
                        const auto reference = expected.Pixels();
                        if (colorIndex == 0) foregroundControls[snapIndex] = reference;
                        else if (fixture.format != DWRITE_GLYPH_IMAGE_FORMATS_PNG)
                            Require(DifferentPixels(reference, foregroundControls[snapIndex]) > 100,
                                std::string(fixture.name) + ": fixture contains real foreground-colored pixels; difference=" + std::to_string(DifferentPixels(reference, foregroundControls[snapIndex])) + ", dpi=" + std::to_string(dpi) + ", opacity=" + std::to_string(opacity));
                        else
                            Require(DifferentPixels(reference, foregroundControls[snapIndex]) == 0,
                                "Bitmap fixture draws embedded colors independently of the foreground brush");
                        size_t chromatic = 0;
                        for (size_t index = 0; index < reference.size(); index += 4) {
                            const auto [low, high] = std::minmax({reference[index], reference[index + 1], reference[index + 2]});
                            if (high - low > 32) ++chromatic;
                        }
                        Require(chromatic > 100, std::string(fixture.name) + ": reference contains actual chromatic pixels");
                        CheckHR(expected.Paint(env, expectedLayout.Get(), false, origin, {&clip, 1}, D2D1_DRAW_TEXT_OPTIONS_NONE),
                            "Draw legacy font's monochrome negative control");
                        Require(DifferentPixels(reference, expected.Pixels()) > 100,
                            std::string(fixture.name) + ": color oracle rejects monochrome fallback");
                        for (const bool legacy : {false, true}) {
                            LegacyTarget facade(actual.target.Get());
                            CheckHR(PaintProductionRow(env, actual, actualLayout.Get(), origin, clip, nullptr,
                                legacy ? &facade : nullptr, drawOptions), std::string(fixture.name) + ": render actual color format; legacy=" +
                                std::to_string(legacy) + ", enumerated=" + probe.layerFormats);
                            CheckRendererPictures(actual.Pixels(), reference,
                                std::string(fixture.name) + ": color layers, foreground, alpha and snapping match original Windows pixels");
                            if (legacy) Require(facade.queries7 > 0 && facade.queries4 > 0 && facade.wholeLayoutDraws == 0,
                                "Legacy color oracle exercises Context4 translation without system fallback");
                        }
                        ++snapIndex;
                    }
                    ++colorIndex;
                }
            }
        }
        std::cout << "PASS: privately loaded original " << fixture.name << " font matches system color rendering through Context4.\n";
    }
}
