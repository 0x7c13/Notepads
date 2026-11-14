// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

void CheckTextRendering(Environment &env) {
    using Scintilla::FontQuality;
    ComPtr<IDWriteTextFormat> format;
    CheckHR(env.factory->CreateTextFormat(L"Consolas", nullptr, DWRITE_FONT_WEIGHT_NORMAL,
        DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, 14, L"en-US", &format), "Create font-quality fixture");
    constexpr std::wstring_view text = L"Small text Il1| mnw 0123456789 العربية אבג 中文 e\u0301";
    ComPtr<IDWriteTextLayout> layout;
    CheckHR(env.factory->CreateTextLayout(text.data(), static_cast<UINT32>(text.size()), format.Get(),
        600, 300, &layout), "Create independent font-quality layout");
    const std::array<std::pair<FontQuality, D2D1_TEXT_ANTIALIAS_MODE>, 5> modes{{
        {FontQuality::QualityNonAntialiased, D2D1_TEXT_ANTIALIAS_MODE_ALIASED},
        {FontQuality::QualityAntialiased, D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE},
        {FontQuality::QualityLcdOptimized, D2D1_TEXT_ANTIALIAS_MODE_CLEARTYPE},
        {FontQuality::QualityDefault, D2D1_TEXT_ANTIALIAS_MODE_DEFAULT},
        {FontQuality::QualityAntialiased, D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE}
    }};
    for (const float dpi : {96.f, 120.f, 144.f, 288.f}) {
        for (int targetIndex = 0; targetIndex < 2; targetIndex++) {
            RendererTarget image(env, dpi);
            std::vector<unsigned char> aliased;
            for (const auto &[quality, expected] : modes) {
                image.target->SetTextAntialiasMode(TextAntialiasMode(quality));
                Require(image.target->GetTextAntialiasMode() == expected, "Each requested quality reaches the real D2D target");
                // ClearType's diagnostic path has actual opaque native pixels.
                image.target->BeginDraw();
                image.target->Clear(D2D1::ColorF(D2D1::ColorF::White));
                image.target->DrawTextLayout(D2D1::Point2F(4, 4), layout.Get(), image.brush.Get());
                CheckHR(image.target->EndDraw(), "Font quality transition renders without a deferred D2D error");
                const auto pixels = image.Pixels();
                if (quality == FontQuality::QualityNonAntialiased) aliased = pixels;
                if (quality == FontQuality::QualityAntialiased)
                    Require(DifferentPixels(pixels, aliased) > 20, "Real gray glyph pixels differ from aliased negative control");
                RendererTarget paragraph(env, dpi);
                paragraph.target->SetTextAntialiasMode(TextAntialiasMode(quality));
                const D2D1_RECT_F clip = D2D1::RectF(0, 0, 800, 400);
                const DWRITE_TEXT_RANGE row{0, static_cast<UINT32>(text.size())};
                paragraph.target->BeginDraw();
                paragraph.target->Clear(D2D1::ColorF(D2D1::ColorF::White));
                CheckHR(DrawParagraphTextClipped(env.factory.Get(), paragraph.target.Get(), layout.Get(),
                    D2D1::Point2F(4, 4), clip, paragraph.brush.Get(), D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT, &row),
                    "The custom paragraph renderer supports the same requested antialiasing mode");
                CheckHR(paragraph.target->EndDraw(), "Custom text quality flushes successfully");
                CheckRendererPictures(paragraph.Pixels(), pixels,
                    "Custom paragraph glyphs match the independent system renderer for each quality");
            }
        }
    }
    std::cout << "PASS: text-quality transitions and independent D2D targets at four DPIs.\n";
}
