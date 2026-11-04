// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

#include "LegacyTarget.h"

// This reference enumerates the independently created DirectWrite layout.
// It has no production bounds, font-format choice, row map or cached effects.
// Its all-row output is first checked against the original Windows renderer.
struct ReferenceRowRenderer final : IDWriteTextRenderer {
    RendererTarget &image;
    UINT32 first, last;
    HRESULT failure = S_OK;
    size_t included = 0, excluded = 0;
    ReferenceRowRenderer(RendererTarget &image_, UINT32 first_, UINT32 last_) :
        image(image_), first(first_), last(last_) {}
    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID id, void **value) override {
        if (!value) return E_POINTER;
        *value = nullptr;
        if (id != __uuidof(IUnknown) && id != __uuidof(IDWritePixelSnapping) && id != __uuidof(IDWriteTextRenderer)) return E_NOINTERFACE;
        *value = static_cast<IDWriteTextRenderer *>(this);
        AddRef();
        return S_OK;
    }
    ULONG STDMETHODCALLTYPE AddRef() override { return 1; }
    ULONG STDMETHODCALLTYPE Release() override { return 1; }
    HRESULT STDMETHODCALLTYPE IsPixelSnappingDisabled(void *, BOOL *value) override { *value = FALSE; return S_OK; }
    HRESULT STDMETHODCALLTYPE GetCurrentTransform(void *, DWRITE_MATRIX *value) override {
        D2D1_MATRIX_3X2_F matrix{};
        image.target->GetTransform(&matrix);
        *value = {matrix._11, matrix._12, matrix._21, matrix._22, matrix._31, matrix._32};
        return S_OK;
    }
    HRESULT STDMETHODCALLTYPE GetPixelsPerDip(void *, FLOAT *value) override {
        float x = 0, y = 0;
        image.target->GetDpi(&x, &y);
        *value = x / 96;
        return S_OK;
    }
    HRESULT STDMETHODCALLTYPE DrawGlyphRun(void *, FLOAT x, FLOAT y, DWRITE_MEASURING_MODE mode,
        const DWRITE_GLYPH_RUN *run, const DWRITE_GLYPH_RUN_DESCRIPTION *description, IUnknown *effect) override {
        if (FAILED(failure)) return failure;
        if (!run || !description) return failure = E_UNEXPECTED;
        const uint64_t start = description->textPosition;
        const uint64_t end = start + description->stringLength;
        if (start >= last || end <= first) { ++excluded; return S_OK; }
        if (start < first || end > last) return failure = E_UNEXPECTED;
        ++included;
        ComPtr<ID2D1Brush> brush;
        if (effect) {
            const auto result = effect->QueryInterface(IID_PPV_ARGS(&brush));
            if (FAILED(result)) return failure = result;
        } else brush = image.brush;
        if (image.modern)
            image.modern->DrawGlyphRunWithColorSupport({x, y}, run, description, brush.Get(), nullptr, 0, mode);
        else
            image.target->DrawGlyphRun({x, y}, run, brush.Get(), mode);
        return S_OK;
    }
    HRESULT STDMETHODCALLTYPE DrawUnderline(void *, FLOAT, FLOAT, const DWRITE_UNDERLINE *, IUnknown *) override { return failure = E_NOTIMPL; }
    HRESULT STDMETHODCALLTYPE DrawStrikethrough(void *, FLOAT, FLOAT, const DWRITE_STRIKETHROUGH *, IUnknown *) override { return failure = E_NOTIMPL; }
    HRESULT STDMETHODCALLTYPE DrawInlineObject(void *, FLOAT, FLOAT, IDWriteInlineObject *, BOOL, BOOL, IUnknown *) override { return failure = E_NOTIMPL; }
};

void PaintReferenceRow(RendererTarget &image, IDWriteTextLayout *layout, D2D1_POINT_2F origin,
    D2D1_RECT_F clip, UINT32 first, UINT32 last) {
    image.target->BeginDraw();
    image.target->Clear(D2D1::ColorF(D2D1::ColorF::White));
    image.target->PushAxisAlignedClip(clip, D2D1_ANTIALIAS_MODE_ALIASED);
    ReferenceRowRenderer renderer(image, first, last);
    const auto result = layout->Draw(nullptr, &renderer, origin.x, origin.y);
    image.target->PopAxisAlignedClip();
    const auto ended = image.target->EndDraw();
    CheckHR(result, "Draw independent complete glyph callbacks");
    CheckHR(renderer.failure, "Independent reference has no swallowed callback failure");
    CheckHR(ended, "End independent row drawing");
    Require(renderer.included > 0, "Independent row reference actually includes glyph callbacks");
}

HRESULT PaintProductionRow(Environment &env, RendererTarget &image, IDWriteTextLayout *layout,
    D2D1_POINT_2F origin, D2D1_RECT_F clip, const DWRITE_TEXT_RANGE *row,
    ID2D1RenderTarget *capabilities = nullptr,
    D2D1_DRAW_TEXT_OPTIONS options = D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT) {
    image.target->BeginDraw();
    image.target->Clear(D2D1::ColorF(D2D1::ColorF::White));
    image.target->PushAxisAlignedClip(clip, D2D1_ANTIALIAS_MODE_ALIASED);
    const auto result = DrawParagraphTextClipped(env.factory.Get(), capabilities ? capabilities : image.target.Get(),
        layout, origin, clip, image.brush.Get(), options, row);
    image.target->PopAxisAlignedClip();
    const auto ended = image.target->EndDraw();
    return FAILED(result) ? result : ended;
}

struct RejectedBrush final : IUnknown {
    unsigned brushQueries = 0;
    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID id, void **value) override {
        if (!value) return E_POINTER;
        *value = nullptr;
        if (id == __uuidof(ID2D1Brush)) ++brushQueries;
        if (id != __uuidof(IUnknown)) return E_NOINTERFACE;
        *value = static_cast<IUnknown *>(this);
        AddRef();
        return S_OK;
    }
    ULONG STDMETHODCALLTYPE AddRef() override { return 1; }
    ULONG STDMETHODCALLTYPE Release() override { return 1; }
};

ParagraphOptions EditorFontOptions(Environment &env, float size, bool rtl) {
    env.format.Reset();
    CheckHR(env.factory->CreateTextFormat(L"Consolas", nullptr, DWRITE_FONT_WEIGHT_NORMAL,
        DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, size, L"en-US", &env.format), "Create configured editor font");
    ComPtr<IDWriteTextLayout> metricLayout;
    CheckHR(env.factory->CreateTextLayout(L"X", 1, env.format.Get(), 1000, 1000, &metricLayout), "Measure configured font");
    DWRITE_LINE_METRICS metric{};
    UINT32 count = 0;
    CheckHR(metricLayout->GetLineMetrics(&metric, 1, &count), "Read configured-font line metrics");
    auto options = Options(rtl, ParagraphWrap::Word, 220);
    options.baseline = std::ceil(metric.baseline);
    options.lineHeight = options.baseline + std::ceil(metric.height - metric.baseline);
    return options;
}

void CheckRowRendering() {
    Environment env;
    std::string text;
    for (int index = 0; index < 40; ++index) text += Bytes(u8"أُإِآ أّ ");
    const D2D1_POINT_2F origin{10, 10};
    constexpr DWRITE_TEXT_RANGE all{0, UINT32_MAX};
    size_t visibleNeighborCases = 0;
    for (const float size : {14.0f, 14.0f * 96 / 72, 28.0f}) {
        for (const bool rtl : {false, true}) {
            const auto options = EditorFontOptions(env, size, rtl);
            RejectedBrush rejected;
            auto actualLayout = Reference(env, text, options);
            auto expectedLayout = Reference(env, text, options);
            auto systemLayout = Reference(env, text, options);
            const auto rows = ReferenceRows(expectedLayout.Get());
            Require(rows.size() >= 3, "Real editor metrics produce neighboring wrapped Arabic rows");
            RendererTarget actual(env), expected(env), system(env);
            for (size_t index = 0; index < 2; ++index) {
                const auto &line = rows[index];
                const DWRITE_TEXT_RANGE row{line.start, line.length};
                const auto clip = D2D1::RectF(0, origin.y + line.top, 260, origin.y + line.top + line.height);
                for (const bool multicolor : {false, true}) {
                    actual.brush->SetColor(D2D1::ColorF(D2D1::ColorF::Red));
                    expected.brush->SetColor(D2D1::ColorF(D2D1::ColorF::Red));
                    system.brush->SetColor(D2D1::ColorF(D2D1::ColorF::Red));
                    if (multicolor) {
                        // Distinct row-adjacent colors make leaked neighbor ink observable.
                        actual.Effect(actualLayout.Get(), all, D2D1::ColorF(D2D1::ColorF::Blue));
                        expected.Effect(expectedLayout.Get(), all, D2D1::ColorF(D2D1::ColorF::Blue));
                        system.Effect(systemLayout.Get(), all, D2D1::ColorF(D2D1::ColorF::Blue));
                        actual.Effect(actualLayout.Get(), row, D2D1::ColorF(D2D1::ColorF::Red));
                        expected.Effect(expectedLayout.Get(), row, D2D1::ColorF(D2D1::ColorF::Red));
                        system.Effect(systemLayout.Get(), row, D2D1::ColorF(D2D1::ColorF::Red));
                        actual.Effect(actualLayout.Get(), {row.startPosition, 2}, D2D1::ColorF(D2D1::ColorF::Green));
                        expected.Effect(expectedLayout.Get(), {row.startPosition, 2}, D2D1::ColorF(D2D1::ColorF::Green));
                        system.Effect(systemLayout.Get(), {row.startPosition, 2}, D2D1::ColorF(D2D1::ColorF::Green));
                    }
                    CheckHR(system.Paint(env, systemLayout.Get(), false, origin, {&clip, 1}), "Draw independent original system rows");
                    PaintReferenceRow(expected, expectedLayout.Get(), origin, clip, 0, UINT32_MAX);
                    CheckRendererPictures(expected.Pixels(), system.Pixels(), "All-row reference first matches original Windows rendering");
                    PaintReferenceRow(expected, expectedLayout.Get(), origin, clip, row.startPosition, row.startPosition + row.length);
                    if (DifferentPixels(expected.Pixels(), system.Pixels()) > 0) ++visibleNeighborCases;
                    for (const bool legacy : {false, true}) {
                        LegacyTarget facade(actual.target.Get());
                        CheckHR(PaintProductionRow(env, actual, actualLayout.Get(), origin, clip, &row, legacy ? &facade : nullptr),
                            "Draw only the requested display row");
                        CheckRendererPictures(actual.Pixels(), expected.Pixels(), "Requested row excludes neighboring Arabic ink");
                        if (legacy) Require(facade.queries7 > 0 && facade.queries4 > 0 && facade.wholeLayoutDraws == 0,
                            "Forced legacy row uses Context4 without whole-paragraph fallback");
                    }
                    FLOAT x = 0, y = 0;
                    DWRITE_HIT_TEST_METRICS hit{};
                    CheckHR(expectedLayout->HitTestTextPosition(row.startPosition, FALSE, &x, &y, &hit),
                        "Locate an independent glyph cell for block-caret clipping");
                    Require(hit.width > 0, "Block-caret clip contains a real glyph cell");
                    const auto caretClip = D2D1::RectF(origin.x + hit.left, clip.top,
                        origin.x + hit.left + hit.width, clip.bottom);
                    PaintReferenceRow(expected, expectedLayout.Get(), origin, caretClip, row.startPosition, row.startPosition + row.length);
                    for (const bool legacy : {false, true}) {
                        LegacyTarget facade(actual.target.Get());
                        CheckHR(PaintProductionRow(env, actual, actualLayout.Get(), origin, caretClip, &row, legacy ? &facade : nullptr),
                            "Draw the row through a narrower block-caret clip");
                        CheckRendererPictures(actual.Pixels(), expected.Pixels(), "Block-caret clip preserves only its row's glyphs");
                    }
                    CheckHR(actualLayout->SetDrawingEffect(nullptr, all), "Clear actual row colors");
                    CheckHR(expectedLayout->SetDrawingEffect(nullptr, all), "Clear reference row colors");
                    CheckHR(systemLayout->SetDrawingEffect(nullptr, all), "Clear original system row colors");
                }
            }

            const DWRITE_TEXT_RANGE row{rows[0].start, rows[0].length};
            const auto clip = D2D1::RectF(0, origin.y, 260, origin.y + rows[0].height);
            CheckHR(actualLayout->SetDrawingEffect(&rejected, {rows[1].start, rows[1].length}), "Install off-row invalid brush");
            CheckHR(PaintProductionRow(env, actual, actualLayout.Get(), origin, clip, &row), "Off-row effects are never processed");
            Require(rejected.brushQueries == 0, "Logical row rejection occurs before drawing-effect QueryInterface");
            PaintReferenceRow(expected, expectedLayout.Get(), origin, clip, row.startPosition, row.startPosition + row.length);
            CheckRendererPictures(actual.Pixels(), expected.Pixels(), "Off-row invalid effect leaves accepted row intact");
            CheckHR(actualLayout->SetDrawingEffect(&rejected, row), "Move invalid effect into accepted row");
            Require(PaintProductionRow(env, actual, actualLayout.Get(), origin, clip, &row) == E_NOINTERFACE && rejected.brushQueries > 0,
                "Accepted-row effect errors propagate instead of repainting with the system renderer");
            CheckHR(actualLayout->SetDrawingEffect(nullptr, all), "Release borrowed rejected effect");
            LegacyTarget unavailable(actual.target.Get(), false);
            Require(PaintProductionRow(env, actual, actualLayout.Get(), origin, clip, &row, &unavailable) == E_NOINTERFACE &&
                unavailable.wholeLayoutDraws == 0, "Missing required row capability fails without whole-layout fallback");
            const auto pixels = actual.Pixels();
            Require(std::all_of(pixels.begin(), pixels.end(), [](unsigned char value) { return value == 255; }),
                "Capability failure occurs before any glyph pixels are drawn");
        }
    }
    Require(visibleNeighborCases > 0, "Overhang fixture rejects the old whole-layout-through-row-clip implementation");

    RendererTarget image(env);
    LegacyTarget legacy(image.target.Get());
    const auto clip = D2D1::RectF(0, 0, 260, 100);
    for (const bool underline : {false, true}) {
        auto decorated = Reference(env, "plain decorated", Options(false));
        CheckHR(underline ? decorated->SetUnderline(TRUE, {6, 3}) : decorated->SetStrikethrough(TRUE, {6, 3}),
            "Install a late decoration on the forced Context4 layout");
        Require(PaintProductionRow(env, image, decorated.Get(), origin, clip, nullptr, &legacy) == E_NOTIMPL,
            "Context4 rejects layout decorations under the same contract as Context7");
    }
    FailingInline failure;
    auto inlineLayout = Reference(env, "aXb", Options(false));
    CheckHR(inlineLayout->SetInlineObject(&failure, {1, 1}), "Install forced Context4 callback failure");
    Require(PaintProductionRow(env, image, inlineLayout.Get(), origin, clip, nullptr, &legacy) == E_FAIL && failure.calls == 1,
        "Context4 preserves the first callback failure without retrying a partial draw");
    CheckHR(inlineLayout->SetInlineObject(nullptr, all), "Release borrowed forced Context4 inline object");
    Require(legacy.queries4 > 0 && legacy.queries7 > 0 && legacy.wholeLayoutDraws == 0,
        "Legacy decoration and callback errors never escape through system fallback");
    std::cout << "PASS: modern and forced legacy row rendering excludes real adjacent Arabic ink and rejects off-row effects early.\n";
}
