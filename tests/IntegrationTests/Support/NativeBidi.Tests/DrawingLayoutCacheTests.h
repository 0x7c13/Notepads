// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

ComPtr<IDWriteTextLayout> DrawingLayoutReference(Environment &env, IDWriteTextFormat *format,
    std::wstring_view text, float width, float height) {
    ComPtr<IDWriteTextLayout> result;
    CheckHR(env.factory->CreateTextLayout(text.data(), static_cast<UINT32>(text.size()),
        format, width, height, &result), "Independent uncached drawing layout");
    ComPtr<IDWriteTypography> typography;
    CheckHR(env.factory->CreateTypography(&typography), "Independent WinUI typography");
    for (const auto feature : {DWRITE_FONT_FEATURE_TAG_STANDARD_LIGATURES, DWRITE_FONT_FEATURE_TAG_CONTEXTUAL_LIGATURES,
        DWRITE_FONT_FEATURE_TAG_CONTEXTUAL_ALTERNATES, DWRITE_FONT_FEATURE_TAG_KERNING, DWRITE_FONT_FEATURE_TAG_TABULAR_FIGURES})
        CheckHR(typography->AddFontFeature({feature, 1}), "Independent font feature");
    CheckHR(result->SetTypography(typography.Get(), {0, static_cast<UINT32>(text.size())}), "Independent layout typography");
    return result;
}

void CheckDrawingLayoutCache(Environment &env) {
    {
        auto format = env.format;
        DirectWriteTextWidthCache cache(env.factory.Get(), format.Get());
        for (const auto text : {L"", L"1", L"99", L"100", L"1234567890", L"00001", L" 42 ", L"中文 🙂", L"e\u0301 עברית",
            L"This is longer than the short-label cache admission limit"}) {
            const auto reference = DrawingLayoutReference(env, format.Get(), text, 10000, 1000);
            DWRITE_TEXT_METRICS expected{};
            CheckHR(reference->GetMetrics(&expected), "Independent width query");
            Require(cache.Get(text) == expected.widthIncludingTrailingWhitespace && cache.Get(text) == expected.widthIncludingTrailingWhitespace,
                "Numeric, trailing-space, Unicode and oversized width queries preserve exact geometry");
        }
        for (unsigned int i = 0; i < 2048; ++i) {
            const auto number = std::to_wstring(i);
            const auto reference = DrawingLayoutReference(env, format.Get(), number, 10000, 1000);
            DWRITE_TEXT_METRICS expected{};
            CheckHR(reference->GetMetrics(&expected), "Width collision oracle");
            Require(cache.Get(number) == expected.widthIncludingTrailingWhitespace,
                "Eviction and hash collisions never return another label's width");
        }
    }
    bool cacheable = true;
    Require(!CreateDrawingTextLayout(env.factory.Get(), env.format.Get(), L"failure", -1, 40, &cacheable) && !cacheable,
        "Creation failures explicitly refuse cache admission");
    Require(CreateDrawingTextLayout(env.factory.Get(), env.format.Get(), L"valid", 200, 40, &cacheable) && cacheable,
        "Only a fully configured layout is eligible for reuse");
    constexpr std::wstring_view text = L"office AV 012 中文日本語 e\u0301 🙂 אבג مرحبا";
    ComPtr<IDWriteTextLayout> held;
    for (const float size : {14.f, 28.f}) {
        ComPtr<IDWriteTextFormat> format;
        CheckHR(env.factory->CreateTextFormat(L"Consolas", nullptr, DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, size, L"en-US", &format), "Cache font fixture");
        CheckHR(format->SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP), "Cache uses ordinary unwrapped fragments");
        DirectWriteTextLayoutCache cache(env.factory.Get(), format.Get());
        DirectWriteTextWidthCache widths(env.factory.Get(), format.Get());
        const auto widthReference = DrawingLayoutReference(env, format.Get(), text, 10000, 1000);
        DWRITE_TEXT_METRICS widthMetrics{};
        CheckHR(widthReference->GetMetrics(&widthMetrics), "New font width oracle");
        Require(widths.Get(text) == widthMetrics.widthIncludingTrailingWhitespace,
            "A realised font owns its own width measurements");
        auto layout = cache.Get(text, 600, 50);
        Require(layout && layout.Get() == cache.Get(text, 600, 50).Get(), "Identical draws reuse the actual layout object");
        held = layout;
        auto narrow = cache.Get(text, 160, 50);
        auto shortBox = cache.Get(text, 600, 30);
        Require(narrow && shortBox && narrow.Get() != layout.Get() && shortBox.Get() != layout.Get(),
            "Both width and height participate in layout identity");
        Near(narrow->GetMaxWidth(), 160, "Width-specific layout retained");
        Near(shortBox->GetMaxHeight(), 30, "Height-specific layout retained");
        const auto reference = DrawingLayoutReference(env, format.Get(), text, 600, 50);
        for (const float dpi : {96.f, 144.f, 288.f})
        for (const auto quality : {D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE, D2D1_TEXT_ANTIALIAS_MODE_CLEARTYPE}) {
            RendererTarget actual(env, dpi), expected(env, dpi);
            actual.target->SetTextAntialiasMode(quality);
            expected.target->SetTextAntialiasMode(quality);
            for (const auto origin : {D2D1::Point2F(4, 4), D2D1::Point2F(-23, -7.5f)})
            for (const auto color : {D2D1::ColorF::Black, D2D1::ColorF::Red}) {
                actual.brush->SetColor(D2D1::ColorF(color));
                expected.brush->SetColor(D2D1::ColorF(color));
                const D2D1_RECT_F clip{0, 0, 230, 60};
                CheckHR(actual.Paint(env, cache.Get(text, 600, 50).Get(), false, origin, {&clip, 1}), "Paint retained layout");
                CheckHR(expected.Paint(env, reference.Get(), false, origin, {&clip, 1}), "Paint independent layout");
                CheckRendererPictures(actual.Pixels(), expected.Pixels(), "Cached text matches at changed origins, brushes, clips, DPI and quality");
            }
        }
        auto edit = cache.Get(L"changed 中文", 600, 50);
        Require(edit && edit.Get() != layout.Get(), "An edited fragment cannot reuse old text");
        RendererTarget edited(env), stale(env);
        const D2D1_RECT_F clip{0, 0, 600, 100};
        CheckHR(edited.Paint(env, edit.Get(), false, {4, 4}, {&clip, 1}), "Paint changed text");
        CheckHR(stale.Paint(env, layout.Get(), false, {4, 4}, {&clip, 1}), "Paint stale text negative control");
        Require(DifferentPixels(edited.Pixels(), stale.Pixels()) > 100, "The raster oracle detects stale text reuse");
        for (float x : {-10.f, 10.f, 80.f, 400.f}) {
            BOOL actualTrailing = FALSE, actualInside = FALSE, expectedTrailing = FALSE, expectedInside = FALSE;
            DWRITE_HIT_TEST_METRICS actual{}, expected{};
            CheckHR(layout->HitTestPoint(x, 10, &actualTrailing, &actualInside, &actual), "Retained layout hit geometry");
            CheckHR(reference->HitTestPoint(x, 10, &expectedTrailing, &expectedInside, &expected), "Independent layout hit geometry");
            Require(actual.textPosition == expected.textPosition && actual.length == expected.length &&
                actualTrailing == expectedTrailing && actualInside == expectedInside, "Reuse preserves logical hit and cluster boundaries");
            Near(actual.left, expected.left, "Reuse preserves glyph location");
        }

        for (size_t i = 0; i < 2000; ++i) {
            const auto unique = std::to_wstring(i) + L" 中文 cache pressure";
            auto value = cache.Get(unique, 600, 50);
            Require(value && cache.Size() <= DirectWriteTextLayoutCache::MaximumEntries &&
                cache.TextUnits() <= DirectWriteTextLayoutCache::MaximumTextUnits, "Distinct fragments cannot grow retention without bounds");
            Require(value.Get() == cache.Get(unique, 600, 50).Get(), "Pressure does not prevent immediate reuse");
            if (i % 257 == 0) {
                const auto independent = DrawingLayoutReference(env, format.Get(), unique, 600, 50);
                CheckHR(edited.Paint(env, value.Get(), false, {4, 4}, {&clip, 1}), "Paint fragment under cache pressure");
                CheckHR(stale.Paint(env, independent.Get(), false, {4, 4}, {&clip, 1}), "Paint independent pressure reference");
                CheckRendererPictures(edited.Pixels(), stale.Pixels(), "Eviction and hash collisions preserve the requested text");
            }
        }
        for (size_t i = 0; i < DirectWriteTextLayoutCache::MaximumTextUnits / 3000 + 16; ++i) {
            const auto unique = std::wstring(3000, static_cast<wchar_t>(L'一' + i));
            Require(static_cast<bool>(cache.Get(unique, 600, 50)), "Large admitted fragment");
            Require(cache.TextUnits() <= DirectWriteTextLayoutCache::MaximumTextUnits, "Text budget evicts before reaching the object limit");
        }
        const auto boundary = std::wstring(DirectWriteTextLayoutCache::MaximumEntryUnits, L'a');
        const auto admitted = cache.Get(boundary, 600, 50);
        Require(admitted && cache.Get(boundary, 600, 50).Get() == admitted.Get(), "The maximum admitted fragment is reusable");
        const size_t count = cache.Size(), units = cache.TextUnits();
        const auto oversized = std::wstring(DirectWriteTextLayoutCache::MaximumEntryUnits + 1, L'字');
        const auto first = cache.Get(oversized, 600, 50);
        Require(first && cache.Get(oversized, 600, 50).Get() != first.Get(), "Oversized fragments use uncached layout");
        Require(cache.Size() == count && cache.TextUnits() == units, "Bypass preserves useful cached fragments");
        Require(!cache.Get(L"failed", -1, 50), "Invalid layout creation fails normally");
        Require(cache.Size() == count && cache.TextUnits() == units, "Failed creation leaves no poisoned slot");
        Require(static_cast<bool>(cache.Get(L"failed", 600, 50)), "Valid retry succeeds");
    }
    DWRITE_TEXT_METRICS metrics{};
    CheckHR(held->GetMetrics(&metrics), "An in-use layout remains valid after its cache is destroyed");
    Require(metrics.width > 10, "Surviving reference has real geometry");
    std::cout << "PASS: bounded drawing-layout reuse, eviction and independent glyph pixels.\n";
}
