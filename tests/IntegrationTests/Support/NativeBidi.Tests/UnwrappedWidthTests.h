// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

void CheckUnwrappedWidth(Environment &env) {
    // The core uses this finite Scintilla sentinel for unwrapped LTR paragraphs.
    // Compare it against an independently configured Windows viewport layout.
    constexpr float unwrappedWidth = static_cast<float>(0x7ffffff);
    for (const auto fixture : {u8"abc אבג def", u8"\tאבג\t123\t ", u8"abc \u202eDEF\u202c tail   ",
        u8"abc 👩‍💻 العربية 日本語 \t end  ", u8"\u2067אבג\u2069 abc", u8""}) {
        const auto text = Bytes(fixture);
        auto referenceOptions = Options(false, ParagraphWrap::None, 550);
        referenceOptions.leadingPadding = 5;
        referenceOptions.trailingPadding = 38;
        const auto reference = Reference(env, text, referenceOptions);
        const auto wide = Wide(text);
        for (const float width : {1.0f, unwrappedWidth}) {
            auto options = referenceOptions;
            options.width = width;
            DirectWriteParagraph actual(env.factory.Get(), text, env.format.Get(), {}, {}, options);
            CheckHR(actual.Status(), "Stable unwrapped paragraph width");
            DWRITE_TEXT_METRICS expected{};
            CheckHR(reference->GetMetrics(&expected), "Independent unwrapped metrics");
            Near(actual.WidthIncludingTrailingWhitespace(), expected.widthIncludingTrailingWhitespace, "Intrinsic width ignores viewport");
            Require(actual.Lines().size() == expected.lineCount, "No-wrap row count ignores viewport");
            for (const auto byte : Boundaries(text)) {
                const auto utf16 = static_cast<UINT32>(Wide(text.substr(0, byte)).size());
                for (const bool trailing : {false, true}) {
                    FLOAT x{}, y{};
                    DWRITE_HIT_TEST_METRICS metrics{};
                    CheckHR(reference->HitTestTextPosition(trailing && utf16 ? utf16 - 1 : utf16,
                        trailing && utf16, &x, &y, &metrics), "Independent unwrapped caret");
                    const auto point = actual.CaretPoint({byte, trailing});
                    Near(point.x, x + options.leadingPadding, "Unwrapped caret X ignores viewport");
                    Near(point.y, 0, "Unwrapped caret remains in the uniform row");
                }
            }
            for (float x = -20; x < expected.widthIncludingTrailingWhitespace + 20; x += 7.25f) {
                BOOL trailing{}, inside{};
                DWRITE_HIT_TEST_METRICS metrics{};
                CheckHR(reference->HitTestPoint(x, 15, &trailing, &inside, &metrics), "Independent overflow hit");
                const auto hit = actual.HitTest(x + options.leadingPadding, 15);
                Require(actual.Utf16Position(hit.position) == metrics.textPosition + (trailing ? metrics.length : 0) &&
                    hit.trailing == (trailing != FALSE), "Hit position and affinity ignore viewport bounds");
            }
            const auto rectangles = actual.RangeRectangles(0, text.size());
            const auto referenceRectangles = ReferenceRectangles(reference.Get(), 0, static_cast<UINT32>(wide.size()));
            if (text.empty()) {
                // DirectWrite reports a degenerate caret metric for an empty
                // range. The paragraph API returns selection ink only; its
                // caret position is checked independently above.
                Require(rectangles.empty(), "An empty unwrapped selection has no highlight rectangles");
                Require(referenceRectangles.size() == 1 && referenceRectangles.front().length == 0 &&
                    referenceRectangles.front().width == 0, "The empty Windows range describes only its caret");
            } else {
                Require(rectangles.size() == referenceRectangles.size(),
                    "Unwrapped selection interval count ignores viewport: bytes=" + std::to_string(text.size()) +
                    ", width=" + std::to_string(width) + ", actual=" + std::to_string(rectangles.size()) +
                    ", reference=" + std::to_string(referenceRectangles.size()));
            }
            for (size_t i = 0; i < rectangles.size(); ++i) {
                Near(rectangles[i].left, referenceRectangles[i].left + options.leadingPadding, "Unwrapped selection X");
                Near(rectangles[i].width, referenceRectangles[i].width, "Unwrapped selection width");
            }
            RendererTarget actualPixels(env), referencePixels(env);
            const auto origin = D2D1::Point2F(12.25f + options.leadingPadding, 9.5f);
            const auto clip = D2D1::RectF(0, 0, 800, 100);
            CheckHR(actualPixels.Paint(env, actual.Layout(), false, origin, {&clip, 1}), "Unwrapped production layout raster");
            CheckHR(referencePixels.Paint(env, reference.Get(), false, origin, {&clip, 1}), "Independent viewport raster");
            Require(DifferentPixels(actualPixels.Pixels(), referencePixels.Pixels()) == 0,
                "Stable unwrapped width preserves independent Windows pixels");
        }
    }
    const auto text = Bytes(u8"abc\x01 אבג\tend  ");
    const ParagraphRepresentation representation{3, 1, 47};
    auto options = Options(false, ParagraphWrap::None, 550);
    options.leadingPadding = 7;
    options.trailingPadding = 46;
    DirectWriteParagraph normal(env.factory.Get(), text, env.format.Get(), {}, {&representation, 1}, options);
    options.width = unwrappedWidth;
    DirectWriteParagraph unwrapped(env.factory.Get(), text, env.format.Get(), {}, {&representation, 1}, options);
    CheckHR(normal.Status(), "Normal-width representation");
    CheckHR(unwrapped.Status(), "Unwrapped representation");
    Near(unwrapped.WidthIncludingPadding(), normal.WidthIncludingPadding(), "Control representation and EOL padding width");
    for (const auto byte : Boundaries(text)) {
        for (const bool trailing : {false, true}) {
            Near(unwrapped.CaretPoint({byte, trailing}).x, normal.CaretPoint({byte, trailing}).x,
                "Control representation, tab and padding coordinates ignore no-wrap width");
        }
    }
    const auto rtlNormal = Reference(env, Bytes(u8"אבג ABC"), Options(true, ParagraphWrap::None, 550));
    const auto rtlWide = Reference(env, Bytes(u8"אבג ABC"), Options(true, ParagraphWrap::None, 2048));
    FLOAT x{}, y{}, otherX{}, otherY{};
    DWRITE_HIT_TEST_METRICS metrics{}, otherMetrics{};
    CheckHR(rtlNormal->HitTestTextPosition(0, FALSE, &x, &y, &metrics), "RTL normal-width control");
    CheckHR(rtlWide->HitTestTextPosition(0, FALSE, &otherX, &otherY, &otherMetrics), "RTL wide control");
    Require(otherX - x > 1400, "RTL negative control still depends on its common alignment canvas");
}
