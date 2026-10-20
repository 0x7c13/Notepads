// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

#include <windows.h>
#include <d2d1.h>
#include <d2d1_3.h>
#include <dwrite.h>
#include <dwrite_3.h>
#include <wincodec.h>
#include <wrl/client.h>
#include <algorithm>
#include <array>
#include <chrono>
#include <cmath>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <limits>
#include <span>
#include <stdexcept>
#include <string>
#include <string_view>
#include <vector>
#include "DirectWriteParagraph.h"
#include "DirectWriteRunLayout.h"
#include "DirectWriteRenderer.h"
#include "SurfaceDrawSession.h"
#include "UniConversion.h"

size_t CheckLineLayoutCache(IDWriteFactory *factory, IDWriteTextFormat *format);
size_t CheckSelectionDisplayCaret();
size_t CheckGutterGeometry();
size_t CheckUndoCaretRestoration();
size_t CheckVerticalScrollGeometry();

namespace {
using Microsoft::WRL::ComPtr;
using namespace Scintilla::Internal;

size_t checks = 0;
struct CapabilityResult {
    const char *name;
    const char *status = "not_run";
    const char *reason = "The capability check was not reached.";
};
std::array<CapabilityResult, 3> capabilityResults{{{"COLRv1"}, {"ChromaticEmoji"}, {"CustomRendererErrors"}}};

void RecordCapability(size_t index, bool verified, const char *unavailableReason) {
    capabilityResults[index].status = verified ? "verified" : "skipped";
    capabilityResults[index].reason = verified ? "" : unavailableReason;
}

void WriteReport(const std::filesystem::path &path, bool passed) {
    if (path.empty()) return;
    std::ofstream output(path, std::ios::binary | std::ios::trunc);
    if (!output) throw std::runtime_error("Cannot create NativeBidi report.");
    // Names and reasons are fixed ASCII strings without JSON metacharacters.
    output << "{\n  \"status\": \"" << (passed ? "passed" : "failed")
        << "\",\n  \"assertions\": " << checks << ",\n  \"capabilities\": [\n";
    for (size_t index = 0; index < capabilityResults.size(); ++index) {
        const auto &capability = capabilityResults[index];
        output << "    {\"name\": \"" << capability.name << "\", \"status\": \"" << capability.status
            << "\", \"reason\": \"" << capability.reason << "\"}"
            << (index + 1 == capabilityResults.size() ? "\n" : ",\n");
    }
    output << "  ]\n}\n";
    output.close();
    if (!output) throw std::runtime_error("Cannot finish NativeBidi report.");
}

void Require(bool condition, std::string_view message) {
    if (!condition) throw std::runtime_error(std::string(message));
    ++checks;
}
void CheckHR(HRESULT result, std::string_view message) {
    Require(SUCCEEDED(result), std::string(message) + " (HRESULT " + std::to_string(result) + ')');
}
void Near(float actual, float expected, std::string_view message) {
    Require(std::isfinite(actual) && std::abs(actual - expected) < 0.05f,
        std::string(message) + ": actual=" + std::to_string(actual) + ", expected=" + std::to_string(expected));
}
std::string Bytes(std::u8string_view value) {
    return {reinterpret_cast<const char *>(value.data()), value.size()};
}

// This oracle uses the Windows UTF-8 converter rather than the production
// paragraph's byte/UTF-16 lookup table.
std::wstring Wide(std::string_view text) {
    if (text.empty()) return {};
    const int count = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, text.data(), static_cast<int>(text.size()), nullptr, 0);
    Require(count > 0, "Reference UTF-8 fixture is valid");
    std::wstring result(static_cast<size_t>(count), L'\0');
    Require(MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, text.data(), static_cast<int>(text.size()), result.data(), count) == count,
        "Reference UTF-8 conversion completes");
    return result;
}
std::vector<size_t> Boundaries(std::string_view text) {
    std::vector<size_t> result;
    for (size_t position = 0; position < text.size(); ++position)
        if ((static_cast<unsigned char>(text[position]) & 0xc0) != 0x80) result.push_back(position);
    result.push_back(text.size());
    return result;
}

struct Environment {
    ComPtr<IDWriteFactory> factory;
    ComPtr<IDWriteTextFormat> format;
    ComPtr<ID2D1Factory> drawing;
    ComPtr<IWICImagingFactory> imaging;
    Environment() {
        CheckHR(DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED, __uuidof(IDWriteFactory),
            reinterpret_cast<IUnknown **>(factory.GetAddressOf())), "Create DirectWrite factory");
        CheckHR(factory->CreateTextFormat(L"Segoe UI", nullptr, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL, 24.0f, L"en-US", &format), "Create reference font");
        CheckHR(D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, drawing.GetAddressOf()), "Create Direct2D factory");
        CheckHR(CoCreateInstance(CLSID_WICImagingFactory, nullptr, CLSCTX_INPROC_SERVER,
            IID_PPV_ARGS(&imaging)), "Create WIC factory");
    }
};

ParagraphOptions Options(bool rtl, ParagraphWrap wrap = ParagraphWrap::None, float width = 550) {
    ParagraphOptions options;
    options.width = width;
    options.lineHeight = 34;
    options.baseline = 26;
    options.tabWidth = 48;
    options.rtl = rtl;
    options.wrap = wrap;
    return options;
}

ComPtr<IDWriteTextLayout> Reference(Environment &env, std::string_view text, ParagraphOptions options) {
    const auto wide = Wide(text);
    ComPtr<IDWriteTextLayout> layout;
    CheckHR(env.factory->CreateTextLayout(wide.data(), static_cast<UINT32>(wide.size()), env.format.Get(),
        std::max(1.0f, options.width - options.leadingPadding - options.trailingPadding), 1000000, &layout),
        "Create independent whole-paragraph layout");
    CheckHR(layout->SetReadingDirection(options.rtl ? DWRITE_READING_DIRECTION_RIGHT_TO_LEFT : DWRITE_READING_DIRECTION_LEFT_TO_RIGHT),
        "Reference base direction");
    CheckHR(layout->SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING), "Reference alignment");
    CheckHR(layout->SetWordWrapping(options.wrap == ParagraphWrap::None ? DWRITE_WORD_WRAPPING_NO_WRAP :
        options.wrap == ParagraphWrap::Character ? DWRITE_WORD_WRAPPING_CHARACTER : DWRITE_WORD_WRAPPING_WRAP),
        "Reference wrapping");
    CheckHR(layout->SetLineSpacing(DWRITE_LINE_SPACING_METHOD_UNIFORM, options.lineHeight, options.baseline),
        "Reference line spacing");
    CheckHR(layout->SetIncrementalTabStop(options.tabWidth), "Reference tab stops");
    return layout;
}

std::vector<DWRITE_HIT_TEST_METRICS> ReferenceRectangles(IDWriteTextLayout *layout, UINT32 start, UINT32 length) {
    UINT32 count = 0;
    const auto queried = layout->HitTestTextRange(start, length, 0, 0, nullptr, 0, &count);
    Require(SUCCEEDED(queried) || queried == E_NOT_SUFFICIENT_BUFFER, "Reference range buffer query");
    std::vector<DWRITE_HIT_TEST_METRICS> rectangles(count);
    if (count) CheckHR(layout->HitTestTextRange(start, length, 0, 0, rectangles.data(), count, &count), "Reference visual intervals");
    return rectangles;
}

struct ReferenceRow { UINT32 start, length; float top, height; };
std::vector<ReferenceRow> ReferenceRows(IDWriteTextLayout *layout) {
    UINT32 count = 0;
    const auto queried = layout->GetLineMetrics(nullptr, 0, &count);
    Require(SUCCEEDED(queried) || queried == E_NOT_SUFFICIENT_BUFFER, "Reference row count query");
    std::vector<DWRITE_LINE_METRICS> metrics(count);
    CheckHR(layout->GetLineMetrics(metrics.data(), count, &count), "Reference uniform row metrics");
    std::vector<ReferenceRow> rows;
    UINT32 start = 0;
    float top = 0;
    for (const auto &metric : metrics) {
        rows.push_back({start, metric.length, top, metric.height});
        start += metric.length;
        top += metric.height;
    }
    return rows;
}
size_t ReferenceRowAt(std::span<const ReferenceRow> rows, UINT32 position, bool trailing) {
    Require(!rows.empty(), "Reference paragraph has a row");
    const auto character = trailing && position ? position - 1 : position;
    for (size_t row = 0; row + 1 < rows.size(); ++row)
        if (character < rows[row].start + rows[row].length) return row;
    return rows.size() - 1;
}

std::vector<unsigned char> Raster(Environment &env, IDWriteTextLayout *layout) {
    constexpr UINT width = 800, height = 400, stride = width * 4;
    ComPtr<IWICBitmap> bitmap;
    CheckHR(env.imaging->CreateBitmap(width, height, GUID_WICPixelFormat32bppPBGRA, WICBitmapCacheOnLoad, &bitmap),
        "Allocate offscreen paragraph bitmap");
    ComPtr<ID2D1RenderTarget> target;
    auto properties = D2D1::RenderTargetProperties(D2D1_RENDER_TARGET_TYPE_SOFTWARE,
        D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_PREMULTIPLIED), 96, 96);
    CheckHR(env.drawing->CreateWicBitmapRenderTarget(bitmap.Get(), properties, &target), "Create offscreen render target");
    target->SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
    ComPtr<ID2D1SolidColorBrush> ink;
    CheckHR(target->CreateSolidColorBrush(D2D1::ColorF(D2D1::ColorF::Black), &ink), "Create raster brush");
    target->BeginDraw();
    target->Clear(D2D1::ColorF(D2D1::ColorF::White));
    target->DrawTextLayout(D2D1::Point2F(4, 4), layout, ink.Get());
    CheckHR(target->EndDraw(), "Draw paragraph to pixels");
    std::vector<unsigned char> pixels(static_cast<size_t>(stride) * height);
    CheckHR(bitmap->CopyPixels(nullptr, stride, static_cast<UINT>(pixels.size()), pixels.data()), "Read paragraph pixels");
    return pixels;
}

void SameRaster(Environment &env, IDWriteTextLayout *actual, IDWriteTextLayout *expected, std::string_view label) {
    const auto a = Raster(env, actual), b = Raster(env, expected);
    size_t ink = 0, differing = 0;
    for (size_t index = 0; index < a.size(); index += 4) {
        if (b[index] < 250 || b[index + 1] < 250 || b[index + 2] < 250) ++ink;
        if (std::abs(static_cast<int>(a[index]) - b[index]) > 2 ||
            std::abs(static_cast<int>(a[index + 1]) - b[index + 1]) > 2 ||
            std::abs(static_cast<int>(a[index + 2]) - b[index + 2]) > 2) ++differing;
    }
    Require(ink > 20, "Raster oracle contains visible glyphs");
    Require(differing == 0, std::string(label) + ": differing pixels=" + std::to_string(differing));
}

// The optimized renderer is compared with the operating system's original
// DrawTextLayout, including its real color-font and drawing-effect behavior.
// These targets contain no editor/Surface imitation or renderer-derived oracle.
struct RendererTarget {
    static constexpr UINT width = 800, height = 400, stride = width * 4;
    ComPtr<IWICBitmap> bitmap;
    ComPtr<ID2D1RenderTarget> target;
    ComPtr<ID2D1DeviceContext7> modern;
    ComPtr<ID2D1SolidColorBrush> brush;
    RendererTarget(Environment &env, float dpi = 96, D2D1_MATRIX_3X2_F transform = D2D1::Matrix3x2F::Identity()) {
        CheckHR(env.imaging->CreateBitmap(width, height, GUID_WICPixelFormat32bppPBGRA,
            WICBitmapCacheOnLoad, &bitmap), "Create renderer oracle bitmap");
        const auto properties = D2D1::RenderTargetProperties(D2D1_RENDER_TARGET_TYPE_SOFTWARE,
            D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_PREMULTIPLIED), dpi, dpi);
        CheckHR(env.drawing->CreateWicBitmapRenderTarget(bitmap.Get(), properties, &target), "Create renderer oracle target");
        // Context4 supports the legacy custom renderer. Context7 is needed only
        // for modern colour-specific checks, which report unavailable coverage.
        target.As(&modern);
        target->SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
        target->SetTransform(transform);
        CheckHR(target->CreateSolidColorBrush(D2D1::ColorF(D2D1::ColorF::Black), &brush), "Create renderer default brush");
    }
    HRESULT Paint(Environment &env, IDWriteTextLayout *layout, bool optimized, D2D1_POINT_2F origin,
        std::span<const D2D1_RECT_F> clips, D2D1_DRAW_TEXT_OPTIONS options = D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT) {
        target->BeginDraw();
        target->Clear(D2D1::ColorF(D2D1::ColorF::White));
        HRESULT drawn = S_OK;
        for (const auto clip : clips) {
            target->PushAxisAlignedClip(clip, D2D1_ANTIALIAS_MODE_ALIASED);
            if (optimized)
                drawn = DrawParagraphTextClipped(env.factory.Get(), target.Get(), layout, origin, clip, brush.Get(), options);
            else
                target->DrawTextLayout(origin, layout, brush.Get(), options);
            target->PopAxisAlignedClip();
            if (FAILED(drawn)) break;
        }
        const auto flushed = target->EndDraw();
        return FAILED(drawn) ? drawn : flushed;
    }
    std::vector<unsigned char> Pixels() {
        std::vector<unsigned char> pixels(static_cast<size_t>(stride) * height);
        CheckHR(bitmap->CopyPixels(nullptr, stride, static_cast<UINT>(pixels.size()), pixels.data()), "Read renderer oracle pixels");
        return pixels;
    }
    void Effect(IDWriteTextLayout *layout, DWRITE_TEXT_RANGE range, D2D1_COLOR_F colour) {
        ComPtr<ID2D1SolidColorBrush> effect;
        CheckHR(target->CreateSolidColorBrush(colour, &effect), "Create real target-bound drawing effect");
        CheckHR(layout->SetDrawingEffect(effect.Get(), range), "Set independent fixture effect range");
    }
};

size_t DifferentPixels(std::span<const unsigned char> actual, std::span<const unsigned char> expected) {
    Require(actual.size() == expected.size(), "Renderer pictures have equal dimensions");
    size_t differing = 0;
    for (size_t index = 0; index < actual.size(); index += 4) {
        bool different = false;
        for (size_t channel = 0; channel < 4; ++channel)
            different = different || std::abs(static_cast<int>(actual[index + channel]) - expected[index + channel]) > 2;
        differing += different;
    }
    return differing;
}

void CheckRendererPictures(std::span<const unsigned char> actual, std::span<const unsigned char> expected,
    std::string_view label, size_t minimumInk = 20) {
    size_t ink = 0;
    for (size_t index = 0; index < expected.size(); index += 4)
        if (expected[index] < 250 || expected[index + 1] < 250 || expected[index + 2] < 250) ++ink;
    Require(ink >= minimumInk, std::string(label) + ": independent system picture contains visible ink");
    const auto differing = DifferentPixels(actual, expected);
    Require(differing == 0, std::string(label) + ": differing RGBA pixels=" + std::to_string(differing));
}

std::vector<unsigned char> CompareRenderer(Environment &env, RendererTarget &actual, RendererTarget &expected,
    IDWriteTextLayout *actualLayout, IDWriteTextLayout *expectedLayout, D2D1_POINT_2F origin,
    D2D1_RECT_F clip, std::string_view label, size_t minimumInk = 20,
    D2D1_DRAW_TEXT_OPTIONS options = D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT) {
    CheckHR(actual.Paint(env, actualLayout, true, origin, {&clip, 1}, options), std::string(label) + ": production renderer");
    CheckHR(expected.Paint(env, expectedLayout, false, origin, {&clip, 1}, options), std::string(label) + ": system renderer");
    auto pixels = actual.Pixels();
    CheckRendererPictures(pixels, expected.Pixels(), label, minimumInk);
    return pixels;
}

// Capability evidence is collected directly from the fonts selected by
// DirectWrite. Merely displaying an emoji would also pass in monochrome.
struct PaintTreeProbe final : IDWriteTextRenderer {
    DWRITE_PAINT_FEATURE_LEVEL level;
    size_t glyphs = 0;
    size_t versionOneFaces = 0;
    HRESULT failure = S_OK;
    explicit PaintTreeProbe(DWRITE_PAINT_FEATURE_LEVEL level_) : level(level_) {}
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
    HRESULT Inspect(const DWRITE_GLYPH_RUN *run) {
        ComPtr<IDWriteFontFace7> face;
        if (FAILED(run->fontFace->QueryInterface(IID_PPV_ARGS(&face)))) return S_OK;
        const void *table = nullptr;
        UINT32 size = 0;
        void *context = nullptr;
        BOOL exists = FALSE;
        auto result = face->TryGetFontTable(DWRITE_MAKE_OPENTYPE_TAG('C', 'O', 'L', 'R'), &table, &size, &context, &exists);
        if (FAILED(result)) return result;
        const auto bytes = static_cast<const unsigned char *>(table);
        const bool versionOne = exists && size >= 2 && bytes[0] == 0 && bytes[1] == 1;
        if (exists) face->ReleaseFontTable(context);
        if (!versionOne) return S_OK;
        ++versionOneFaces;
        ComPtr<IDWritePaintReader> reader;
        result = face->CreatePaintReader(DWRITE_GLYPH_IMAGE_FORMATS_COLR_PAINT_TREE, level, &reader);
        if (FAILED(result)) return result;
        for (UINT32 index = 0; index < run->glyphCount; ++index) {
            DWRITE_PAINT_ELEMENT element{};
            D2D_RECT_F bounds{};
            result = reader->SetCurrentGlyph(run->glyphIndices[index], &element, &bounds);
            if (FAILED(result)) return result;
            if (element.paintType != DWRITE_PAINT_TYPE_NONE && bounds.right > bounds.left && bounds.bottom > bounds.top) ++glyphs;
        }
        return S_OK;
    }
    HRESULT STDMETHODCALLTYPE DrawGlyphRun(void *, FLOAT, FLOAT, DWRITE_MEASURING_MODE,
        const DWRITE_GLYPH_RUN *run, const DWRITE_GLYPH_RUN_DESCRIPTION *, IUnknown *) override {
        const auto result = Inspect(run);
        if (FAILED(result) && SUCCEEDED(failure)) failure = result;
        return result;
    }
    HRESULT STDMETHODCALLTYPE DrawUnderline(void *, FLOAT, FLOAT, const DWRITE_UNDERLINE *, IUnknown *) override { return S_OK; }
    HRESULT STDMETHODCALLTYPE DrawStrikethrough(void *, FLOAT, FLOAT, const DWRITE_STRIKETHROUGH *, IUnknown *) override { return S_OK; }
    HRESULT STDMETHODCALLTYPE DrawInlineObject(void *, FLOAT, FLOAT, IDWriteInlineObject *, BOOL, BOOL, IUnknown *) override { return S_OK; }
};

struct FailingInline final : IDWriteInlineObject {
    int calls = 0;
    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID id, void **value) override {
        if (!value) return E_POINTER;
        *value = nullptr;
        if (id != __uuidof(IUnknown) && id != __uuidof(IDWriteInlineObject)) return E_NOINTERFACE;
        *value = static_cast<IDWriteInlineObject *>(this);
        AddRef();
        return S_OK;
    }
    ULONG STDMETHODCALLTYPE AddRef() override { return 1; }
    ULONG STDMETHODCALLTYPE Release() override { return 1; }
    HRESULT STDMETHODCALLTYPE Draw(void *, IDWriteTextRenderer *, FLOAT, FLOAT, BOOL, BOOL, IUnknown *) override {
        ++calls;
        return E_FAIL;
    }
    HRESULT STDMETHODCALLTYPE GetMetrics(DWRITE_INLINE_OBJECT_METRICS *value) override { *value = {20, 24, 18, FALSE}; return S_OK; }
    HRESULT STDMETHODCALLTYPE GetOverhangMetrics(DWRITE_OVERHANG_METRICS *value) override { *value = {}; return S_OK; }
    HRESULT STDMETHODCALLTYPE GetBreakConditions(DWRITE_BREAK_CONDITION *before, DWRITE_BREAK_CONDITION *after) override {
        *before = *after = DWRITE_BREAK_CONDITION_NEUTRAL;
        return S_OK;
    }
};

void CheckRenderer(Environment &env) {
    ComPtr<IDWriteFactory4> rendererFactory;
    env.factory.As(&rendererFactory);
    constexpr DWRITE_TEXT_RANGE all{0, UINT32_MAX};
    const auto origin = D2D1::Point2F(24, 18);
    const auto full = D2D1::RectF(0, 0, 760, 380);
    RendererTarget actual(env), expected(env);
    ComPtr<ID2D1DeviceContext4> rendererContext;
    actual.target.As(&rendererContext);
    const auto mixed = Bytes(u8"abc אבג السَّلَامُ عَلَيْكُمْ 123 e\u0301\u0323 😀 XYZ ");
    for (const bool rtl : {false, true}) {
        const auto options = Options(rtl, ParagraphWrap::Word, 220);
        auto a = Reference(env, mixed + mixed + mixed, options);
        auto b = Reference(env, mixed + mixed + mixed, options);
        const auto undecorated = CompareRenderer(env, actual, expected, a.Get(), b.Get(), origin, full, "Mixed complete paragraph");
        const auto rows = ReferenceRows(b.Get());
        Require(rows.size() >= 3, "Renderer fixture contains middle wrapped rows");
        const auto middle = D2D1::RectF(15, origin.y + rows[1].top, 250, origin.y + rows[2].top);
        CompareRenderer(env, actual, expected, a.Get(), b.Get(), origin, middle, "Only a middle wrapped row");

        // The editor draws decoration separately. A layout that violates that
        // contract must fail, including a decoration that begins after zero.
        CheckHR(a->SetUnderline(TRUE, {4, 3}), "Set late underline on actual layout");
        CheckHR(b->SetUnderline(TRUE, {4, 3}), "Set late underline on reference layout");
        CheckHR(a->SetStrikethrough(TRUE, {8, 5}), "Set late strike on actual layout");
        CheckHR(b->SetStrikethrough(TRUE, {8, 5}), "Set late strike on reference layout");
        CheckHR(expected.Paint(env, b.Get(), false, origin, {&full, 1}), "Draw independent late decoration fixture");
        Require(DifferentPixels(expected.Pixels(), undecorated) > 20, "Late decorations produce independently observable ink");
        if (rendererContext && rendererFactory)
            Require(actual.Paint(env, a.Get(), true, origin, {&full, 1}) == E_NOTIMPL,
                "Unexpected layout decoration violates the paragraph renderer contract explicitly");
        else
            CompareRenderer(env, actual, expected, a.Get(), b.Get(), origin, full, "Unavailable custom renderer uses system decoration fallback");
    }

    const auto effectText = Bytes(u8"abc אבג لا السَّلَامُ 123");
    auto effectsActual = Reference(env, effectText, Options(false));
    auto effectsExpected = Reference(env, effectText, Options(false));
    const auto plain = CompareRenderer(env, actual, expected, effectsActual.Get(), effectsExpected.Get(), origin, full, "Plain effect baseline");
    for (const auto colour : {D2D1::ColorF(D2D1::ColorF::Red), D2D1::ColorF(D2D1::ColorF::Blue)}) {
        // UTF-16 offsets are fixture constants, independent of the paragraph's
        // UTF-8 mapping. The effect crosses an Arabic ligature boundary too.
        actual.Effect(effectsActual.Get(), {1, 8}, colour);
        expected.Effect(effectsExpected.Get(), {1, 8}, colour);
        const auto changed = CompareRenderer(env, actual, expected, effectsActual.Get(), effectsExpected.Get(), origin, full, "Mixed selection/effect foreground");
        Require(DifferentPixels(changed, plain) > 20, "Effect fixture actually changes the foreground pixels");
    }
    CheckHR(effectsActual->SetDrawingEffect(nullptr, all), "Caller clears actual layout effects");
    CheckHR(effectsExpected->SetDrawingEffect(nullptr, all), "Caller clears reference layout effects");
    const auto restored = CompareRenderer(env, actual, expected, effectsActual.Get(), effectsExpected.Get(), origin, full, "Same-layout return to default foreground");
    CheckRendererPictures(restored, plain, "Default foreground has no retained effect");
    RendererTarget replacement(env);
    CompareRenderer(env, replacement, expected, effectsActual.Get(), effectsExpected.Get(), origin, full, "Same layout on a replacement render target");

    auto italicActual = Reference(env, Bytes(u8"jf e\u0301\u0323 السَّلَامُ"), Options(false));
    auto italicExpected = Reference(env, Bytes(u8"jf e\u0301\u0323 السَّلَامُ"), Options(false));
    for (auto *layout : {italicActual.Get(), italicExpected.Get()}) {
        CheckHR(layout->SetFontFamilyName(L"Times New Roman", all), "Select known italic overhang font");
        CheckHR(layout->SetFontStyle(DWRITE_FONT_STYLE_ITALIC, all), "Select italic overhang");
        CheckHR(layout->SetFontSize(36, all), "Set overhang fixture font size");
    }
    CompareRenderer(env, actual, expected, italicActual.Get(), italicExpected.Get(), origin, full, "Italic and combining ink extents");
    DWRITE_OVERHANG_METRICS overhang{};
    CheckHR(italicExpected->GetOverhangMetrics(&overhang), "Measure independent italic overhang");
    Require(overhang.left > 0.5f, "Italic fixture has real ink before its logical start");
    const auto sliver = D2D1::RectF(origin.x - overhang.left - 1, 0, origin.x, 120);
    CompareRenderer(env, actual, expected, italicActual.Get(), italicExpected.Get(), origin, sliver, "Clip intersects only italic overhang", 1);

    const auto emoji = Bytes(u8"A 😀 👩🏽‍💻 🏳️‍🌈 B");
    auto emojiActual = Reference(env, emoji, Options(false));
    auto emojiExpected = Reference(env, emoji, Options(false));
    bool paintTreeAvailable = false;
    if (actual.modern && rendererFactory) {
        PaintTreeProbe probe(actual.modern->GetPaintFeatureLevel());
        CheckHR(emojiExpected->Draw(nullptr, &probe, 0, 0), "Inspect independently resolved COLRv1 glyphs");
        CheckHR(probe.failure, "Independent font inspection has no swallowed callback error");
        if (probe.versionOneFaces) {
            Require(probe.glyphs > 0, "Installed COLRv1 fixture resolves actual paint-tree glyphs");
            paintTreeAvailable = true;
        }
    }
    if (!paintTreeAvailable)
        std::cout << "SKIP: COLRv1-specific coverage requires modern DirectWrite/Direct2D and an installed COLRv1 emoji font.\n";
    const auto colored = CompareRenderer(env, actual, expected, emojiActual.Get(), emojiExpected.Get(), origin, full, "Emoji, ZWJ and skin tone");
    size_t chromatic = 0;
    for (size_t index = 0; index < colored.size(); index += 4) {
        const auto [low, high] = std::minmax({colored[index], colored[index + 1], colored[index + 2]});
        if (high - low > 32) ++chromatic;
    }
    if (paintTreeAvailable) Require(chromatic > 100, "COLRv1 oracle contains actual colored pixels");
    RecordCapability(0, paintTreeAvailable,
        "Modern DirectWrite/Direct2D with an installed COLRv1 emoji font is unavailable.");
    CheckHR(expected.Paint(env, emojiExpected.Get(), false, origin, {&full, 1}, D2D1_DRAW_TEXT_OPTIONS_NONE), "Render monochrome negative control");
    if (chromatic > 100)
        Require(DifferentPixels(colored, expected.Pixels()) > 100, "Disabling color-font support is rejected by the oracle");
    else
        std::cout << "SKIP: installed emoji fallback has no chromatic pixels; monochrome equality is not color coverage.\n";
    RecordCapability(1, chromatic > 100, "Installed emoji fallback has no chromatic pixels.");
    if (paintTreeAvailable) std::cout << "PASS: real COLRv1 paint-tree glyphs match system color pixels and reject monochrome output.\n";
    actual.brush->SetColor(D2D1::ColorF(D2D1::ColorF::Red));
    expected.brush->SetColor(D2D1::ColorF(D2D1::ColorF::Red));
    CompareRenderer(env, actual, expected, emojiActual.Get(), emojiExpected.Get(), origin, full, "Color fonts with a changed foreground brush");
    CheckHR(emojiActual->SetFontStyle(DWRITE_FONT_STYLE_ITALIC, all), "Italic actual emoji fixture");
    CheckHR(emojiExpected->SetFontStyle(DWRITE_FONT_STYLE_ITALIC, all), "Italic reference emoji fixture");
    CompareRenderer(env, actual, expected, emojiActual.Get(), emojiExpected.Get(), origin, full, "Simulated italic color fonts retain overhang");
    CheckHR(emojiActual->SetFontStyle(DWRITE_FONT_STYLE_NORMAL, all), "Restore actual emoji font");
    CheckHR(emojiExpected->SetFontStyle(DWRITE_FONT_STYLE_NORMAL, all), "Restore reference emoji font");
    actual.brush->SetColor(D2D1::ColorF(D2D1::ColorF::Black));
    expected.brush->SetColor(D2D1::ColorF(D2D1::ColorF::Black));

    for (const float dpi : {120.0f, 144.0f, 192.0f}) {
        const bool tileOffset = dpi == 192;
        const auto transform = tileOffset ? D2D1::Matrix3x2F::Translation(113.375f, 47.625f) :
            D2D1::Matrix3x2F::Translation(0.375f, 0.625f);
        RendererTarget scaledActual(env, dpi, transform), scaledExpected(env, dpi, transform);
        // Reuse the layout created at 96 DPI instead of rebuilding geometry.
        CompareRenderer(env, scaledActual, scaledExpected, emojiActual.Get(), emojiExpected.Get(),
            D2D1::Point2F(8.25f, 10.5f), D2D1::RectF(7, 0, 350, 160), "DPI and fractional transform preserve color glyphs");
        // A real tile translation must not turn the pre-transform clip into
        // screen coordinates. This narrow negative clip keeps only a few glyphs.
        const auto clippedOrigin = tileOffset ? D2D1::Point2F(-80.25f, -20.5f) : D2D1::Point2F(-18.25f, 10.5f);
        const auto clippedBounds = tileOffset ? D2D1::RectF(-65, -25, -20, 35) : D2D1::RectF(0, 0, 250, 150);
        if (tileOffset)
            CompareRenderer(env, scaledActual, scaledExpected, emojiActual.Get(), emojiExpected.Get(),
                clippedOrigin, clippedBounds, "Narrow translated clip preserves color paint-tree glyphs");
        CompareRenderer(env, scaledActual, scaledExpected, effectsActual.Get(), effectsExpected.Get(),
            clippedOrigin, clippedBounds, "Transformed clip preserves mixed glyphs", 20,
            static_cast<D2D1_DRAW_TEXT_OPTIONS>(D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT | D2D1_DRAW_TEXT_OPTIONS_NO_SNAP));
    }
    // Change DPI on the same targets and layouts: fresh targets alone cannot
    // catch a renderer retaining an earlier pixels-per-DIP value. Compare to
    // Windows at each step, and require an exact return to the initial pixels.
    const auto beforeDpiChange = CompareRenderer(env, actual, expected,
        effectsActual.Get(), effectsExpected.Get(), origin, full, "Before live render-target DPI changes");
    for (const float dpi : {120.0f, 144.0f, 192.0f, 96.0f}) {
        actual.target->SetDpi(dpi, dpi);
        expected.target->SetDpi(dpi, dpi);
        const auto picture = CompareRenderer(env, actual, expected,
            effectsActual.Get(), effectsExpected.Get(), origin, full, "Live render-target DPI change matches Windows");
        if (dpi == 96)
            Require(picture == beforeDpiChange, "Returning to 96 DPI restores the exact original pixels");
        else
            Require(DifferentPixels(picture, beforeDpiChange) > 100,
                "A changed target DPI has an observable raster effect");
    }
    const auto complexTransform = D2D1::Matrix3x2F::Scale(1.125f, 0.875f) * D2D1::Matrix3x2F::Translation(0.375f, 0.625f);
    RendererTarget transformedActual(env, 96, complexTransform), transformedExpected(env, 96, complexTransform);
    CompareRenderer(env, transformedActual, transformedExpected, emojiActual.Get(), emojiExpected.Get(), origin, full,
        "Nonuniform scale retains exact system fallback pixels");

    const auto tiledText = mixed + mixed + mixed;
    auto tiledActual = Reference(env, tiledText, Options(true, ParagraphWrap::Word, 220));
    auto tiledExpected = Reference(env, tiledText, Options(true, ParagraphWrap::Word, 220));
    actual.brush->SetOpacity(0.5f);
    expected.brush->SetOpacity(0.5f);
    const auto viewport = D2D1::RectF(12, 18, 260, 188);
    const std::array<D2D1_RECT_F, 2> tiles{{{12, 18, 260, 86}, {12, 86, 260, 188}}};
    CheckHR(actual.Paint(env, tiledActual.Get(), true, origin, tiles), "Draw adjacent clipped tiles");
    CheckHR(expected.Paint(env, tiledExpected.Get(), false, origin, {&viewport, 1}), "Draw one independent reference viewport");
    CheckRendererPictures(actual.Pixels(), expected.Pixels(), "Adjacent clips neither omit nor double-blend glyphs");

    if (rendererContext && rendererFactory) {
        FailingInline failure;
        auto inlineLayout = Reference(env, "aXb", Options(false));
        CheckHR(inlineLayout->SetInlineObject(&failure, {1, 1}), "Install a deliberately failing inline object");
        const auto failed = actual.Paint(env, inlineLayout.Get(), true, origin, {&full, 1});
        Require(failed == E_FAIL && failure.calls == 1,
            "Renderer propagates a swallowed callback error without falling back and drawing a second time: HRESULT=" +
            std::to_string(failed) + ", calls=" + std::to_string(failure.calls));
        CheckHR(inlineLayout->SetInlineObject(nullptr, all), "Release borrowed inline object before leaving scope");
        std::cout << "PASS: renderer preserves callback failure without retrying a partial draw.\n";
    } else {
        std::cout << "SKIP: custom renderer capabilities are unavailable; system fallback was compared above.\n";
    }
    RecordCapability(2, rendererContext && rendererFactory,
        "The Direct2D Context4 or DirectWrite Factory4 interface is unavailable.");
    std::cout << "PASS: production clipped renderer matches system pixels for effects, overhang, transforms and tiles.\n";
}

// XAML owns the outer surface transaction. Only that COM boundary is faked;
// all Direct2D draw/clip state below belongs to a real WIC render target.
struct BegunSurface final : ISurfaceImageSourceNativeWithD2D {
    ULONG references = 1;
    int ends = 0;
    HRESULT endResult = S_OK;
    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID id, void **value) override {
        if (!value) return E_POINTER;
        *value = nullptr;
        if (id != __uuidof(IUnknown) && id != __uuidof(ISurfaceImageSourceNativeWithD2D)) return E_NOINTERFACE;
        *value = static_cast<ISurfaceImageSourceNativeWithD2D *>(this);
        AddRef();
        return S_OK;
    }
    // The fixture owns the initial reference until the tested session dies.
    ULONG STDMETHODCALLTYPE AddRef() override { return ++references; }
    ULONG STDMETHODCALLTYPE Release() override { return --references; }
    HRESULT STDMETHODCALLTYPE SetDevice(IUnknown *) override { return E_NOTIMPL; }
    HRESULT STDMETHODCALLTYPE BeginDraw(REFRECT, REFIID, void **, POINT *) override { return E_NOTIMPL; }
    HRESULT STDMETHODCALLTYPE EndDraw() override { return ++ends == 1 ? endResult : E_UNEXPECTED; }
    HRESULT STDMETHODCALLTYPE SuspendDraw() override { return E_NOTIMPL; }
    HRESULT STDMETHODCALLTYPE ResumeDraw() override { return E_NOTIMPL; }
};

struct ScopedTestClip {
    ID2D1RenderTarget *target;
    explicit ScopedTestClip(ID2D1RenderTarget *target_) : target(target_) {
        target->PushAxisAlignedClip(D2D1::RectF(20, 20, 40, 40), D2D1_ANTIALIAS_MODE_ALIASED);
    }
    ~ScopedTestClip() { target->PopAxisAlignedClip(); }
};

void CheckDrawSession(Environment &env) {
    RendererTarget image(env);
    const size_t outsidePreviousClips = 200 * RendererTarget::stride + 300 * 4;
    const auto seedDraw = [&]() {
        image.target->BeginDraw();
        image.target->Clear(D2D1::ColorF(D2D1::ColorF::White));
        CheckHR(image.target->EndDraw(), "Seed opaque contrasting pixels before guarded drawing");
        const auto pixels = image.Pixels();
        Require(pixels[outsidePreviousClips] == 255 && pixels[outsidePreviousClips + 1] == 255 &&
            pixels[outsidePreviousClips + 2] == 255 && pixels[outsidePreviousClips + 3] == 255,
            "Recovery sample starts opaque white outside the later clips");
    };
    const auto freshDraw = [&]() {
        image.target->BeginDraw();
        image.target->Clear(D2D1::ColorF(D2D1::ColorF::White));
        image.target->FillRectangle(D2D1::RectF(0, 0, 800, 400), image.brush.Get());
        CheckHR(image.target->EndDraw(), "A subsequent real draw succeeds after session cleanup");
        const auto pixels = image.Pixels();
        Require(pixels[outsidePreviousClips] == 0 && pixels[outsidePreviousClips + 1] == 0 &&
            pixels[outsidePreviousClips + 2] == 0 && pixels[outsidePreviousClips + 3] == 255,
            "A subsequent draw reaches pixels outside both prior clip rectangles");
    };
    {
        seedDraw();
        BegunSurface surface;
        {
            SurfaceDrawSession session(&surface);
            Require(surface.references == 2, "Draw session retains the begun surface");
            session.BeginD2D(image.target.Get());
            session.PushClip(D2D1::RectF(10, 10, 100, 100));
            CheckHR(session.Finish(), "Explicit draw session finish succeeds");
            CheckHR(session.Finish(), "Repeated finish preserves success");
            Require(surface.ends == 1 && surface.references == 1, "Explicit finish closes and releases the surface once");
        }
        Require(surface.ends == 1, "Destructor does not repeat an explicit finish");
        freshDraw();
    }
    struct InjectedDrawFailure {};
    {
        BegunSurface surface;
        try {
            SurfaceDrawSession session(&surface);
            throw InjectedDrawFailure{};
        } catch (const InjectedDrawFailure &) {
        }
        Require(surface.ends == 1 && surface.references == 1, "Failure before Direct2D still closes the begun XAML surface");
    }
    {
        seedDraw();
        BegunSurface surface;
        try {
            SurfaceDrawSession session(&surface);
            session.BeginD2D(image.target.Get());
            session.PushClip(D2D1::RectF(10, 10, 100, 100));
            ScopedTestClip inner(image.target.Get());
            throw InjectedDrawFailure{};
        } catch (const InjectedDrawFailure &) {
        }
        Require(surface.ends == 1 && surface.references == 1, "Failure unwinds inner and outer clips before closing the surface");
        freshDraw();
    }
    {
        BegunSurface surface;
        surface.endResult = E_ACCESSDENIED;
        SurfaceDrawSession session(&surface);
        session.BeginD2D(image.target.Get());
        Require(session.Finish() == E_ACCESSDENIED && session.Finish() == E_ACCESSDENIED && surface.ends == 1,
            "XAML EndDraw failure propagates and remains idempotent after successful Direct2D EndDraw");
    }
    {
        RendererTarget invalid(env);
        BegunSurface surface;
        surface.endResult = E_ACCESSDENIED;
        SurfaceDrawSession session(&surface);
        session.BeginD2D(invalid.target.Get());
        invalid.target->PopAxisAlignedClip(); // Deliberate real Direct2D error.
        const auto result = session.Finish();
        Require(result == D2DERR_PUSH_POP_UNBALANCED && session.Finish() == result && surface.ends == 1,
            "Direct2D EndDraw failure closes XAML and preserves the first failure: HRESULT=" + std::to_string(result));
    }
    std::cout << "PASS: draw sessions close XAML and real Direct2D state once on success, exceptions and EndDraw failures.\n";
}

void CheckVisualTraversal(DirectWriteParagraph &paragraph, IDWriteTextLayout *reference, std::string_view text, float originX = 0) {
    struct Edge { float x, y; UINT32 position; bool trailing; };
    UINT32 count = 0;
    const auto queried = reference->GetClusterMetrics(nullptr, 0, &count);
    Require(SUCCEEDED(queried) || queried == E_NOT_SUFFICIENT_BUFFER, "Reference cluster count");
    if (!count) return;
    std::vector<DWRITE_CLUSTER_METRICS> clusters(count);
    CheckHR(reference->GetClusterMetrics(clusters.data(), count, &count), "Reference shaping clusters");
    std::vector<Edge> edges;
    const auto rows = ReferenceRows(reference);
    UINT32 position = 0;
    for (const auto &cluster : clusters) {
        Edge first {0, 0, position, false}, last {0, 0, position + cluster.length, true};
        DWRITE_HIT_TEST_METRICS metrics {};
        CheckHR(reference->HitTestTextPosition(position, FALSE, &first.x, &first.y, &metrics), "Reference cluster start");
        CheckHR(reference->HitTestTextPosition(position + cluster.length - 1, TRUE, &last.x, &last.y, &metrics),
            "Reference cluster end");
        first.x += originX;
        last.x += originX;
        first.y = rows[ReferenceRowAt(rows, first.position, first.trailing)].top;
        last.y = rows[ReferenceRowAt(rows, last.position, last.trailing)].top;
        if (std::abs(first.x - last.x) > 0.001f && std::abs(first.y - last.y) < 0.01f) {
            edges.push_back(first);
            edges.push_back(last);
        }
        position += cluster.length;
    }
    for (size_t row = 0; row < paragraph.Lines().size(); ++row) {
        std::vector<float> stops;
        for (const auto &edge : edges)
            if (std::abs(edge.y - paragraph.Lines()[row].top) < 0.01f) stops.push_back(edge.x);
        std::sort(stops.begin(), stops.end());
        stops.erase(std::unique(stops.begin(), stops.end(), [](float a, float b) { return std::abs(a - b) < 0.001f; }), stops.end());
        if (stops.empty()) continue;
        for (const int direction : {-1, 1}) {
            auto caret = paragraph.VisualEdge(row, direction < 0);
            auto previous = paragraph.CaretPoint(caret);
            Near(previous.x, direction < 0 ? stops.back() : stops.front(), "Visual row edge matches independent clusters");
            for (size_t index = 1; index < stops.size(); ++index) {
                const auto next = paragraph.VisualNeighbor(caret, direction);
                Require(next.has_value(), "Visual traversal never ends before the last independently shaped cluster");
                const auto point = paragraph.CaretPoint(*next);
                const auto expectedX = stops[direction < 0 ? stops.size() - index - 1 : index];
                Near(point.x, expectedX, "Each visual arrow visits the next independent cluster edge without loops");
                Near(point.y, paragraph.Lines()[row].top, "Visual traversal stays on its row until its edge");
                Require((point.x - previous.x) * direction > 0.0001f, "Visual traversal makes strict directional progress");
                const auto expected16 = Wide(text.substr(0, next->position)).size();
                Require(std::any_of(edges.begin(), edges.end(), [&](const Edge &edge) {
                    return std::abs(edge.x - point.x) < 0.05f && std::abs(edge.y - point.y) < 0.05f &&
                        edge.position == expected16 && edge.trailing == next->trailing;
                }), "Visual caret logical offset and affinity identify a real independent cluster edge");
                caret = *next;
                previous = point;
            }
        }
    }
}

void CheckReference(Environment &env, std::string_view text, ParagraphOptions options, bool raster) {
    DirectWriteParagraph paragraph(env.factory.Get(), text, env.format.Get(), {}, {}, options);
    Require(paragraph.IsValid(), "Production paragraph is valid");
    const auto reference = Reference(env, text, options);
    const auto originX = options.rtl ? options.trailingPadding : options.leadingPadding;
    Near(paragraph.DrawOriginX(), originX, "Draw origin follows logical leading and trailing gutters");
    Near(paragraph.LayoutWidth(), options.width, "Layout publishes outer width including gutters");
    DWRITE_TEXT_METRICS textMetrics {};
    CheckHR(reference->GetMetrics(&textMetrics), "Independent paragraph extent");
    Near(paragraph.WidthIncludingTrailingWhitespace(), textMetrics.widthIncludingTrailingWhitespace,
        "Raw text extent excludes marker and caret gutters");
    Near(paragraph.WidthIncludingPadding(), textMetrics.widthIncludingTrailingWhitespace + options.leadingPadding + options.trailingPadding,
        "Scroll extent includes both fixed gutters once");
    const auto referenceRows = ReferenceRows(reference.Get());
    const auto offsets = Boundaries(text);
    for (const size_t offset : offsets) {
        const auto utf16 = static_cast<UINT32>(Wide(text.substr(0, offset)).size());
        Require(paragraph.Utf16Position(offset) == utf16 && paragraph.BytePosition(utf16) == offset,
            "Independent UTF-8/UTF-16 mapping");
        FLOAT x = 0, y = 0;
        DWRITE_HIT_TEST_METRICS metrics {};
        CheckHR(reference->HitTestTextPosition(utf16, FALSE, &x, &y, &metrics), "Reference leading caret");
        for (const bool trailing : {false, true}) {
            CheckHR(reference->HitTestTextPosition(trailing && utf16 ? utf16 - 1 : utf16,
                trailing && utf16, &x, &y, &metrics), "Reference caret affinity");
            const auto point = paragraph.CaretPoint({offset, trailing});
            const auto row = ReferenceRowAt(referenceRows, utf16, trailing);
            Near(point.x, x + originX, "Caret X matches whole-paragraph oracle");
            Near(point.y, referenceRows[row].top, "Caret Y uses the uniform display row, not glyph ascent");
            Near(point.height, referenceRows[row].height, "Caret height uses the uniform display row");
            Require(paragraph.CaretLine({offset, trailing}) == row, "Caret row follows logical boundary and wrap affinity");
        }
    }
    UINT32 lineCount = 0;
    const auto queried = reference->GetLineMetrics(nullptr, 0, &lineCount);
    Require(SUCCEEDED(queried) || queried == E_NOT_SUFFICIENT_BUFFER, "Reference line count query");
    std::vector<DWRITE_LINE_METRICS> lines(lineCount);
    CheckHR(reference->GetLineMetrics(lines.data(), lineCount, &lineCount), "Reference line metrics");
    const auto actual = paragraph.Lines();
    Require(actual.size() == lineCount, "Paragraph wrap count matches independent layout");
    size_t start = 0;
    UINT32 utf16Start = 0;
    for (size_t index = 0; index < lines.size(); ++index) {
        const auto next16 = utf16Start + lines[index].length;
        size_t end = text.size();
        for (const auto byte : offsets)
            if (Wide(text.substr(0, byte)).size() == next16) { end = byte; break; }
        Require(actual[index].start == start && actual[index].length == end - start,
            "Wrapped rows retain full paragraph context and exact logical coverage");
        Near(actual[index].height, lines[index].height, "Line height matches oracle");
        start = end;
        utf16Start = next16;
    }
    Require(start == text.size(), "Wrapped rows cover each byte exactly once");
    CheckVisualTraversal(paragraph, reference.Get(), text, originX);

    const auto expectedRectangles = text.empty() ? std::vector<DWRITE_HIT_TEST_METRICS>{} :
        ReferenceRectangles(reference.Get(), 0, static_cast<UINT32>(Wide(text).size()));
    const auto actualRectangles = paragraph.RangeRectangles(0, text.size());
    Require(actualRectangles.size() == expectedRectangles.size(), "Selection has the independent number of visual fragments");
    for (size_t index = 0; index < actualRectangles.size(); ++index) {
        const auto &expected = expectedRectangles[index];
        const auto &rectangle = actualRectangles[index];
        Near(rectangle.left, expected.left + originX, "Selection X includes the fixed drawing origin");
        Near(rectangle.width, expected.width, "Selection width excludes gutters");
        const auto row = ReferenceRowAt(referenceRows, expected.textPosition, false);
        Near(rectangle.top, referenceRows[row].top, "Selection top follows uniform display row");
        Near(rectangle.height, referenceRows[row].height, "Selection height follows uniform display row");
    }

    // Sample independently chosen points, not points returned by production.
    for (float y = options.lineHeight / 2; y < options.lineHeight * static_cast<float>(lineCount); y += options.lineHeight)
        for (float x = 1; x < options.width; x += 17) {
            BOOL trailing = FALSE, inside = FALSE;
            DWRITE_HIT_TEST_METRICS hit {};
            CheckHR(reference->HitTestPoint(x - originX, y, &trailing, &inside, &hit), "Reference mouse hit");
            const UINT32 expected16 = hit.textPosition + (trailing ? hit.length : 0);
            const auto position = paragraph.HitTest(x, y);
            Require(Wide(text.substr(0, position.position)).size() == expected16,
                "Hit testing maps independent screen samples to correct logical boundary");
            Require(position.trailing == (trailing != FALSE), "Mouse hit retains the independent visual edge affinity");
            Require(Wide(text.substr(0, paragraph.HitTest(x, y, true).position)).size() == hit.textPosition,
                "Character hit returns the containing cluster's logical start");
        }
    if (raster && !text.empty()) SameRaster(env, paragraph.Layout(), reference.Get(), "Production shaping raster agrees with reference");
}

void CheckUniformRows(Environment &env) {
    const auto originalFont = env.format;
    const auto text = Bytes(u8"אבג ABC مرحبا דהו אבג XYZ דהו 123 אבג");
    for (const float fontSize : {14.0f, 24.0f, 31.0f}) {
        ComPtr<IDWriteTextFormat> font;
        CheckHR(env.factory->CreateTextFormat(L"Consolas", nullptr, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL, fontSize, L"en-US", &font), "Create fallback-font row regression fixture");
        env.format = font;
        for (const bool rtl : {false, true}) {
            auto options = Options(rtl, ParagraphWrap::Character, 110);
            options.lineHeight = fontSize * 1.5f;
            options.baseline = 1;
            const auto reference = Reference(env, text, options);
            const auto rows = ReferenceRows(reference.Get());
            Require(rows.size() >= 3, "Negative glyph-Y fixture reaches multiple wrapped rows");
            FLOAT x = 0, y = 0;
            DWRITE_HIT_TEST_METRICS glyph {};
            CheckHR(reference->HitTestTextPosition(0, FALSE, &x, &y, &glyph), "Reference fallback glyph bounds");
            Require(y < 0, "Independent DWrite glyph metrics extend above the first uniform row");
            CheckReference(env, text, options, false);
            DirectWriteParagraph paragraph(env.factory.Get(), text, env.format.Get(), {}, {}, options);
            const auto actual = paragraph.RangeRectangles(0, text.size());
            const auto expected = ReferenceRectangles(reference.Get(), 0, static_cast<UINT32>(Wide(text).size()));
            Require(actual.size() == expected.size(), "Uniform-row normalization retains every visual text fragment");
            for (size_t index = 0; index < actual.size(); ++index) {
                const auto row = ReferenceRowAt(rows, expected[index].textPosition, false);
                Near(actual[index].left, expected[index].left, "Normalized selection preserves DWrite X");
                Near(actual[index].width, expected[index].width, "Normalized selection preserves DWrite width");
                Near(actual[index].top, rows[row].top, "Selection row uses logical position rather than negative glyph Y");
                Near(actual[index].height, rows[row].height, "Selection row uses uniform line height");
                Require(actual[index].top >= 0, "No selection can map above display row zero");
            }
        }
    }
    env.format = originalFont;
}

void CheckVisualNavigation(Environment &env) {
    const auto text = Bytes(u8"אבג");
    DirectWriteParagraph paragraph(env.factory.Get(), text, env.format.Get(), {}, {}, Options(true));
    Require(paragraph.IsValid(), "Hebrew navigation paragraph valid");
    ParagraphCaret caret {0, false};
    auto point = paragraph.CaretPoint(caret);
    for (const size_t expected : {2U, 4U, 6U}) {
        const auto next = paragraph.VisualNeighbor(caret, -1);
        Require(next && next->position == expected, "Left traverses exact Hebrew byte offsets");
        const auto nextPoint = paragraph.CaretPoint(*next);
        Require(nextPoint.x < point.x, "Visual Left moves to a strictly smaller X");
        caret = *next;
        point = nextPoint;
    }
    Require(!paragraph.VisualNeighbor(caret, -1), "Left edge reports paragraph boundary");
    for (const size_t expected : {4U, 2U, 0U}) {
        const auto next = paragraph.VisualNeighbor(caret, 1);
        Require(next && next->position == expected, "Right reverses exact Hebrew byte offsets");
        const auto nextPoint = paragraph.CaretPoint(*next);
        Require(nextPoint.x > point.x, "Visual Right moves to a strictly larger X");
        caret = *next;
        point = nextPoint;
    }
    Require(!paragraph.VisualNeighbor(caret, 1), "Right edge reports paragraph boundary");
    Require(paragraph.VisualEdge(0, true).position == 0 && paragraph.VisualEdge(0, false).position == 6,
        "RTL visual row edges map to logical start/end");

    const auto clustered = Bytes(u8"א\u05b7ב😀ג");
    DirectWriteParagraph clusters(env.factory.Get(), clustered, env.format.Get(), {}, {}, Options(true));
    caret = {0, false};
    for (const size_t expected : {4U, 6U, 10U, 12U}) {
        const auto next = clusters.VisualNeighbor(caret, -1);
        Require(next && next->position == expected, "Visual arrows do not split Hebrew combining marks or emoji");
        caret = *next;
    }
}

void CheckSelections(Environment &env) {
    const auto text = Bytes(u8"abc אבג def");
    DirectWriteParagraph paragraph(env.factory.Get(), text, env.format.Get(), {}, {}, Options(false));
    const auto reference = Reference(env, text, Options(false));
    // Select logical "bc אב": the unselected gimel separates its two visual
    // islands. A single endpoint-to-endpoint rectangle would color wrong text.
    auto actual = paragraph.RangeRectangles(1, 8);
    auto expected = ReferenceRectangles(reference.Get(), 1, 5);
    auto byLeft = [](const auto &a, const auto &b) { return a.left < b.left; };
    std::sort(actual.begin(), actual.end(), byLeft);
    std::sort(expected.begin(), expected.end(), byLeft);
    Require(actual.size() == expected.size() && actual.size() >= 2, "Mixed logical selection has multiple visual intervals");
    float width = 0;
    for (size_t index = 0; index < actual.size(); ++index) {
        Near(actual[index].left, expected[index].left, "Selection interval left");
        Near(actual[index].width, expected[index].width, "Selection interval width");
        Near(actual[index].top, 0, "Selection interval uses the first uniform row");
        Require(actual[index].width > 0 && actual[index].height > 0, "Selection rectangles have positive extents");
        width += actual[index].width;
    }
    Require(actual.back().left + actual.back().width - actual.front().left > width + 1,
        "Mixed selection does not cover its unselected visual gap");
    Require(paragraph.RangeRectangles(4, 4).empty(), "Empty selection has no highlight");

    // The inverse problem is different: a single visual rectangle covering
    // "bc <gimel>" maps to two nonadjacent logical source ranges.
    const auto latin = ReferenceRectangles(reference.Get(), 1, 3);
    const auto gimel = ReferenceRectangles(reference.Get(), 6, 1);
    Require(latin.size() == 1 && gimel.size() == 1, "Fixed rectangle oracle has simple constituent runs");
    const float left = latin.front().left;
    const float right = gimel.front().left + gimel.front().width;
    for (const auto sides : {std::pair{left, right}, std::pair{right, left}}) {
        const auto ranges = paragraph.VisualRanges(0, sides.first, sides.second);
        Require(ranges.size() == 2 && ranges[0].start == 1 && ranges[0].length == 3 &&
            ranges[1].start == 8 && ranges[1].length == 2,
            "Visual rectangle maps to exact disjoint logical intervals without selecting aleph/bet");
    }
    Require(paragraph.VisualRanges(0, left, left).empty(), "Zero-width rectangle selects no bytes");

    const auto combinedText = Bytes(u8"א\u05b7בגדהוזח");
    DirectWriteParagraph wrapped(env.factory.Get(), combinedText, env.format.Get(), {}, {}, Options(true, ParagraphWrap::Character, 40));
    Require(wrapped.Lines().size() > 1, "Rectangle fixture wraps to multiple rows");
    for (size_t row = 0; row < wrapped.Lines().size(); ++row) {
        const auto ranges = wrapped.VisualRanges(row, -1000, 1000);
        Require(ranges.size() == 1 && ranges[0].start == wrapped.Lines()[row].start &&
            ranges[0].length == wrapped.Lines()[row].length,
            "A full visual row selects exactly that row's logical bytes and preserves combining clusters");
    }
}

void CheckOversizedCaretQueries(Environment &env) {
    std::vector<size_t> positions{SIZE_MAX};
    if constexpr (sizeof(size_t) > sizeof(uint32_t)) {
        // These keys would alias real stops at bytes 0 and 1 if narrowed.
        positions.push_back(static_cast<size_t>(uint64_t{UINT32_MAX} + 1));
        positions.push_back(static_cast<size_t>(uint64_t{UINT32_MAX} + 2));
    }
    for (const bool rtl : {false, true}) {
        const auto text = rtl ? Bytes(u8"אבג") : std::string("ABC");
        DirectWriteParagraph paragraph(env.factory.Get(), text, env.format.Get(), {}, {}, Options(rtl));
        Require(paragraph.IsValid(), "Oversized caret fixture is valid");
        const auto reference = Reference(env, text, Options(rtl));
        FLOAT endX = 0, endY = 0;
        DWRITE_HIT_TEST_METRICS metrics{};
        CheckHR(reference->HitTestTextPosition(3, FALSE, &endX, &endY, &metrics),
            "Independent three-letter paragraph end");
        for (const size_t position : positions) {
            for (const bool trailing : {false, true}) {
                const ParagraphCaret caret{position, trailing};
                Near(paragraph.CaretPoint(caret).x, endX, "Oversized caret clamps to the independent end position");
                Require(paragraph.CaretLine(caret) == 0, "Oversized caret remains on the only display row");
                const auto inward = paragraph.VisualNeighbor(caret, rtl ? 1 : -1);
                Require(inward && inward->position == (rtl ? 4U : 2U),
                    "Moving inward from an oversized caret reaches the last letter's fixed UTF-8 start");
                Require(!paragraph.VisualNeighbor(caret, rtl ? -1 : 1),
                    "Moving outward from an oversized caret reports the paragraph edge");
            }
        }
    }
}

void CheckStyledShaping(Environment &env) {
    const auto text = Bytes(u8"ABC السَّلَامُ عَلَيْكُمْ אבג 123");
    const auto offsets = Boundaries(text);
    ComPtr<IDWriteTextFormat> equivalentFont;
    CheckHR(env.factory->CreateTextFormat(L"Segoe UI", nullptr, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_STYLE_NORMAL,
        DWRITE_FONT_STRETCH_NORMAL, 24.0f, L"en-US", &equivalentFont), "Create independent equivalent style font");
    std::vector<ParagraphFontRun> styles;
    for (size_t index = 1; index < offsets.size(); ++index)
        styles.push_back({offsets[index - 1], offsets[index] - offsets[index - 1],
            index % 2 ? env.format.Get() : equivalentFont.Get()});
    for (const bool rtl : {false, true}) {
        const auto options = Options(rtl, ParagraphWrap::Word, 240);
        DirectWriteParagraph segmented(env.factory.Get(), text, env.format.Get(), styles, {}, options);
        Require(segmented.IsValid(), "Per-character styled paragraph is valid");
        const auto reference = Reference(env, text, options);
        const auto rows = ReferenceRows(reference.Get());
        SameRaster(env, segmented.Layout(), reference.Get(), "Style boundaries preserve Arabic joining and wrapped context");
        for (const auto byte : offsets) {
            FLOAT x = 0, y = 0;
            DWRITE_HIT_TEST_METRICS metrics {};
            CheckHR(reference->HitTestTextPosition(static_cast<UINT32>(Wide(text.substr(0, byte)).size()), FALSE,
                &x, &y, &metrics), "Styled reference caret");
            Near(segmented.CaretPoint({byte, false}).x, x, "Styling preserves visual caret X");
            const auto utf16 = static_cast<UINT32>(Wide(text.substr(0, byte)).size());
            Near(segmented.CaretPoint({byte, false}).y, rows[ReferenceRowAt(rows, utf16, false)].top,
                "Styling preserves the wrapped display row");
        }
    }
    Require(env.format->GetReadingDirection() == DWRITE_READING_DIRECTION_LEFT_TO_RIGHT,
        "RTL layouts never mutate a shared font format's reading direction");
}

void CheckRepresentations(Environment &env) {
    const std::string text = "a\x01" "b";
    const std::array<ParagraphRepresentation, 1> representations {{{1, 1, 37}}};
    for (const bool rtl : {false, true}) {
        DirectWriteParagraph paragraph(env.factory.Get(), text, env.format.Get(), {}, representations, Options(rtl));
        Require(paragraph.IsValid(), "Paragraph with a control-character representation is valid");
        const auto ranges = paragraph.RangeRectangles(1, 2);
        Require(ranges.size() == 1, "Control-character representation is one visual span");
        Near(ranges.front().width, 37, "Representation has the explicitly requested pixel width");
        const auto hit = paragraph.HitTest(ranges.front().left + ranges.front().width / 4, ranges.front().height / 2, true);
        Require(hit.position == 1, "Clicking a representation selects its original control byte");
    }
}

void CheckAffinity(Environment &env) {
    const auto text = Bytes(u8"ABC אבג DEF");
    for (const bool rtl : {false, true}) {
        DirectWriteParagraph paragraph(env.factory.Get(), text, env.format.Get(), {}, {}, Options(rtl));
        const auto reference = Reference(env, text, Options(rtl));
        // The space resolves with the paragraph direction. The bidi boundary
        // therefore precedes it in RTL and follows it in LTR.
        const size_t boundary = rtl ? 3 : 4;
        const auto leading = paragraph.CaretPoint({boundary, false});
        const auto trailing = paragraph.CaretPoint({boundary, true});
        Require(std::abs(leading.x - trailing.x) > 10,
            "One logical bidi boundary has two distinct visual caret positions");
        FLOAT x = 0, y = 0;
        DWRITE_HIT_TEST_METRICS metrics {};
        CheckHR(reference->HitTestTextPosition(static_cast<UINT32>(boundary - 1), TRUE, &x, &y, &metrics), "Reference preceding cluster trailing edge");
        Near(trailing.x, x, "Trailing affinity uses the preceding cluster instead of following RTL run");
        CheckHR(reference->HitTestTextPosition(static_cast<UINT32>(boundary), FALSE, &x, &y, &metrics), "Reference following cluster leading edge");
        Near(leading.x, x, "Leading affinity uses the following cluster");
        const auto ranges = ReferenceRectangles(reference.Get(), 0, static_cast<UINT32>(Wide(text).size()));
        float leftEdge = std::numeric_limits<float>::infinity();
        float rightEdge = -std::numeric_limits<float>::infinity();
        for (const auto &range : ranges) {
            leftEdge = std::min(leftEdge, range.left);
            rightEdge = std::max(rightEdge, range.left + range.width);
        }
        for (const auto start : {ParagraphCaret{boundary, false}, ParagraphCaret{boundary, true}})
            for (const int direction : {-1, 1}) {
                const auto next = paragraph.VisualNeighbor(start, direction);
                if (next)
                    Require((paragraph.CaretPoint(*next).x - paragraph.CaretPoint(start).x) * direction > 0,
                        "Directional boundary movement starts from the chosen visual edge");
                else
                    Near(paragraph.CaretPoint(start).x, direction > 0 ? rightEdge : leftEdge,
                        "No visual neighbor is valid only at the independent paragraph edge");
            }
    }
}

void CheckDetectionAndValidation(Environment &env) {
    for (const auto text : {u8"plain ASCII 123", u8"中文日本語", u8"😀 e\u0301"})
        Require(!DirectWriteParagraph::NeedsLayout(Bytes(text)), "LTR-only text retains its ordinary fast path");
    for (const auto text : {u8"אבג", u8"مرحبا", u8"١٢٣", u8"x\u200fy", u8"\u2066abc\u2069", u8"\U0001e900", u8"\U0001ee00"})
        Require(DirectWriteParagraph::NeedsLayout(Bytes(text)), "RTL text and direction controls require paragraph layout: " + Bytes(text));
    Require(DirectWriteParagraph::NeedsLayout(std::string(513, 'x') + Bytes(u8"א")), "RTL detection scans beyond the first chunk");

    for (const float width : {0.0f, -1.0f, std::numeric_limits<float>::infinity(), std::numeric_limits<float>::quiet_NaN()}) {
        DirectWriteParagraph paragraph(env.factory.Get(), "abc", env.format.Get(), {}, {}, Options(false, ParagraphWrap::None, width));
        Require(!paragraph.IsValid() && paragraph.Layout() == nullptr && paragraph.Lines().empty(),
            "Invalid layout dimensions fail without publishing partial geometry");
    }
    for (const float padding : {-1.0f, std::numeric_limits<float>::infinity(), std::numeric_limits<float>::quiet_NaN()}) {
        for (const bool leading : {false, true}) {
            auto options = Options(true);
            (leading ? options.leadingPadding : options.trailingPadding) = padding;
            DirectWriteParagraph paragraph(env.factory.Get(), "abc", env.format.Get(), {}, {}, options);
            Require(!paragraph.IsValid(), "Invalid caret or marker padding is rejected");
        }
    }
    const auto text = Bytes(u8"א😀ב");
    const std::array<ParagraphFontRun, 1> invalidFont {{{1, 1, env.format.Get()}}};
    DirectWriteParagraph invalid(env.factory.Get(), text, env.format.Get(), invalidFont, {}, Options(true));
    Require(!invalid.IsValid(), "A font span inside a UTF-8 character is rejected");

    DirectWriteParagraph valid(env.factory.Get(), text, env.format.Get(), {}, {}, Options(true));
    for (const auto pair : {std::pair<size_t, uint32_t>{0, 0}, {2, 1}, {6, 3}, {8, 4}}) {
        Require(valid.Utf16Position(pair.first) == pair.second && valid.BytePosition(pair.second) == pair.first,
            "Supplementary characters have fixed independent byte/UTF-16 boundary pairs");
    }
    Require(valid.Utf16Position(3) == 1 && valid.BytePosition(2) == 2,
        "An interior UTF-8 byte or low surrogate clamps to the character start");
    Require(valid.BytePosition(UINT32_MAX) == 8 && valid.Utf16Position(SIZE_MAX) == 4,
        "Mapping oversized positions clamps at document end");
    const std::string malformed = "\xe0\x80\x80" "A";
    DirectWriteParagraph replacements(env.factory.Get(), malformed, env.format.Get(), {}, {}, Options(false));
    Require(replacements.IsValid() && replacements.Length() == 4, "Malformed UTF-8 remains independently addressable bytes");
    for (uint32_t offset = 0; offset <= 4; ++offset)
        Require(replacements.Utf16Position(offset) == offset && replacements.BytePosition(offset) == offset,
            "Malformed bytes are represented without dropping subsequent text");
}

void ReportScaling(Environment &env) {
    using Clock = std::chrono::steady_clock;
    const auto elapsed = [](auto start, auto end) {
        return std::chrono::duration<double, std::milli>(end - start).count();
    };
    const auto letter = Bytes(u8"א");
    for (const size_t characters : {2048U, 8192U, 32768U}) {
        std::string text;
        text.reserve(characters * letter.size());
        for (size_t index = 0; index < characters; ++index) text += letter;
        const auto start = Clock::now();
        DirectWriteParagraph paragraph(env.factory.Get(), text, env.format.Get(), {}, {}, Options(true));
        CheckHR(paragraph.Status(), "Long RTL paragraph layout");
        const auto shaped = Clock::now();
        auto caret = paragraph.VisualNeighbor({0, false}, -1);
        const auto firstArrow = Clock::now();
        Require(caret && caret->position == 2, "First long-line arrow reaches the adjacent character");
        for (size_t index = 1; index < 1024; ++index) {
            caret = paragraph.VisualNeighbor(*caret, -1);
            Require(caret && caret->position == (index + 1) * 2, "Repeated long-line arrows retain exact positions");
        }
        const auto repeated = Clock::now();
        std::cout << "TIMING: " << characters << " RTL characters; layout=" << elapsed(start, shaped)
            << " ms; first arrow=" << elapsed(shaped, firstArrow)
            << " ms; next 1023 arrows=" << elapsed(firstArrow, repeated) << " ms.\n";
    }
}

#include "RowRenderingTests.h"
#include "LegacyColorTests.h"
#include "UnwrappedWidthTests.h"
#include "ForegroundRenderingTests.h"
#include "InputBoundaryTests.h"
#include "RunLayoutTests.h"
}

int wmain(int argc, wchar_t **argv) {
    const auto initialized = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
    if (FAILED(initialized)) { std::cerr << "FAIL: COM initialization\n"; return 1; }
    int result = 0;
    std::filesystem::path report;
    try {
        const bool performance = argc == 2 && std::wstring_view(argv[1]) == L"--performance";
        if (argc == 3 && std::wstring_view(argv[1]) == L"--report" && *argv[2]) report = argv[2];
        else if (argc != 1 && !performance)
            throw std::runtime_error("Usage: NativeBidi.Tests [--performance | --report <path>]");
        Environment env;
        if (performance) {
            CheckAnalyzedStress();
            ReportScaling(env);
            std::cout << "PASS: RTL scaling observations (no machine-dependent timing threshold).\n";
        } else {
        CheckRunLayout(env);
        std::cout << "PASS: analyzed paragraph geometry and glyphs match independent whole-paragraph DirectWrite.\n";
        for (const auto fixture : {u8"אבג", u8"ABC אבג 123 DEF", u8"مرحبا بالعالم 123 ABC",
            u8"السَّلَامُ عَلَيْكُمْ", u8"אב😀ג e\u0301", u8"(אבג) [123] ABC", u8"אבג  ", u8"אבג\tABC\t12"}) {
            const auto text = Bytes(fixture);
            for (const bool rtl : {false, true}) CheckReference(env, text, Options(rtl), true);
        }
        std::cout << "PASS: mixed-script coordinates, hit tests and offscreen raster match independent DirectWrite.\n";
        const auto wrapped = Bytes(u8"אבג \u2066ABC 123 DEF GHI\u2069 דהו مرحبا 😀 XYZ אבג דהו 12345");
        for (const bool rtl : {false, true})
            for (const auto wrap : {ParagraphWrap::Word, ParagraphWrap::Character})
                for (const float width : {120.0f, 240.0f}) CheckReference(env, wrapped, Options(rtl, wrap, width), true);
        std::cout << "PASS: wrapping retains Unicode isolate context and exact logical row coverage.\n";
        for (const bool rtl : {false, true}) CheckReference(env, "", Options(rtl), false);
        for (const bool rtl : {false, true})
            for (const auto wrap : {ParagraphWrap::None, ParagraphWrap::Word, ParagraphWrap::Character})
                for (const float width : {70.0f, 240.0f}) {
                    auto padded = Options(rtl, wrap, width);
                    padded.leadingPadding = 3;
                    padded.trailingPadding = 27;
                    CheckReference(env, wrapped, padded, true);
                    CheckReference(env, "", padded, false);
                }
        std::cout << "PASS: fixed caret and marker gutters share paint, hit, selection, navigation and wrap coordinates.\n";
        CheckUniformRows(env);
        std::cout << "PASS: fallback-font negative glyph bounds retain uniform caret/selection rows and wrap affinity.\n";
        CheckVisualNavigation(env);
        CheckAffinity(env);
        CheckOversizedCaretQueries(env);
        std::cout << "PASS: exact visual Hebrew navigation, combining/emoji clusters and paragraph edges.\n";
        CheckSelections(env);
        CheckStyledShaping(env);
        CheckRepresentations(env);
        CheckDetectionAndValidation(env);
        CheckDrawingByteBoundaries(env);
        std::cout << "PASS: disjoint selection, style-independent shaping and control representations.\n";
        checks += CheckLineLayoutCache(env.factory.Get(), env.format.Get());
        checks += CheckSelectionDisplayCaret();
        checks += CheckGutterGeometry();
        std::cout << "PASS: bounded page cache, oversized active layout ownership and paragraph revalidation.\n";
        checks += CheckUndoCaretRestoration();
        checks += CheckVerticalScrollGeometry();
        std::cout << "PASS: undo/redo caret edges survive grouped replacements, coalesced deletion and container actions.\n";
        CheckDrawSession(env);
        CheckRenderer(env);
        CheckRowRendering();
        CheckLegacyColorFonts();
        CheckUnwrappedWidth(env);
        std::cout << "PASS: stable unwrapped paragraph widths preserve independent geometry and pixels.\n";
        CheckImmutableForeground();
        std::cout << "PASS: NativeBidi " << checks << " checks.\n";
        }
    } catch (const std::exception &error) {
        std::cerr << "FAIL: " << error.what() << '\n';
        result = 1;
    }
    try { WriteReport(report, result == 0); }
    catch (const std::exception &error) {
        std::cerr << "FAIL: " << error.what() << '\n';
        result = 1;
    }
    CoUninitialize();
    return result;
}
