// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Resources/Math/NativeMathText.h"
#include <array>

namespace Notepads::Markdown::Native
{
    namespace
    {
        constexpr float EmSize = 16.0f;

        class ChineseOutline final : public IDWriteGeometrySink
        {
          public:
            ChineseOutline(void *sink, notepads_markdown_math_text_write write, double x, double y) noexcept
                : _sink(sink), _write(write), _x(x), _y(y)
            {
            }

            HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid, void **output) noexcept override
            {
                if (!output)
                    return E_POINTER;
                *output = nullptr;
                if (iid != __uuidof(IUnknown) && iid != __uuidof(IDWriteGeometrySink))
                    return E_NOINTERFACE;
                *output = static_cast<IDWriteGeometrySink *>(this);
                AddRef();
                return S_OK;
            }

            ULONG STDMETHODCALLTYPE AddRef() noexcept override { return ++_references; }
            ULONG STDMETHODCALLTYPE Release() noexcept override { return --_references; }

            void STDMETHODCALLTYPE SetFillMode(D2D1_FILL_MODE mode) noexcept override
            {
                if (mode != D2D1_FILL_MODE_WINDING)
                    Reject();
            }

            void STDMETHODCALLTYPE SetSegmentFlags(D2D1_PATH_SEGMENT flags) noexcept override
            {
                if (flags != D2D1_PATH_SEGMENT_NONE)
                    Reject();
            }

            void STDMETHODCALLTYPE BeginFigure(D2D1_POINT_2F start, D2D1_FIGURE_BEGIN begin) noexcept override
            {
                if (_open || begin != D2D1_FIGURE_BEGIN_FILLED)
                {
                    Reject();
                    return;
                }
                _open = true;
                Emit({NOTEPADS_MARKDOWN_MATH_MOVE, 0, X(start), Y(start)});
            }

            void STDMETHODCALLTYPE AddLines(const D2D1_POINT_2F *points, UINT32 count) noexcept override
            {
                if (!_open || (count && !points))
                {
                    Reject();
                    return;
                }
                for (UINT32 index = 0; index < count && _status == NOTEPADS_MARKDOWN_CORE_OK; ++index)
                {
                    auto point = points[index];
                    Emit({NOTEPADS_MARKDOWN_MATH_LINE, 0, X(point), Y(point)});
                }
            }

            void STDMETHODCALLTYPE AddBeziers(const D2D1_BEZIER_SEGMENT *segments, UINT32 count) noexcept override
            {
                if (!_open || (count && !segments))
                {
                    Reject();
                    return;
                }
                for (UINT32 index = 0; index < count && _status == NOTEPADS_MARKDOWN_CORE_OK; ++index)
                {
                    auto const &segment = segments[index];
                    Emit({NOTEPADS_MARKDOWN_MATH_CUBIC, 0, X(segment.point1), Y(segment.point1),
                          X(segment.point2), Y(segment.point2), X(segment.point3), Y(segment.point3)});
                }
            }

            void STDMETHODCALLTYPE EndFigure(D2D1_FIGURE_END end) noexcept override
            {
                if (!_open || end != D2D1_FIGURE_END_CLOSED)
                {
                    Reject();
                    return;
                }
                _open = false;
                Emit({NOTEPADS_MARKDOWN_MATH_CLOSE});
            }

            HRESULT STDMETHODCALLTYPE Close() noexcept override { return Status() == NOTEPADS_MARKDOWN_CORE_OK ? S_OK : E_FAIL; }

            int32_t Status() const noexcept
            {
                return _status != NOTEPADS_MARKDOWN_CORE_OK ? _status :
                       _open || _references != 1 ? NOTEPADS_MARKDOWN_MATH_INVALID_GEOMETRY : NOTEPADS_MARKDOWN_CORE_OK;
            }

          private:
            void Reject() noexcept
            {
                if (_status == NOTEPADS_MARKDOWN_CORE_OK)
                    _status = NOTEPADS_MARKDOWN_MATH_INVALID_GEOMETRY;
            }

            double X(D2D1_POINT_2F point) const noexcept { return (point.x + _x) / EmSize; }
            double Y(D2D1_POINT_2F point) const noexcept { return (point.y + _y) / EmSize; }

            void Emit(notepads_markdown_math_command const &command) noexcept
            {
                if (_status != NOTEPADS_MARKDOWN_CORE_OK)
                    return;
                if (!std::isfinite(command.x1) || !std::isfinite(command.y1) ||
                    !std::isfinite(command.x2) || !std::isfinite(command.y2) ||
                    !std::isfinite(command.x3) || !std::isfinite(command.y3))
                {
                    Reject();
                    return;
                }
                _status = _write(_sink, &command);
            }

            void *_sink;
            notepads_markdown_math_text_write _write;
            double _x;
            double _y;
            ULONG _references{1};
            int32_t _status{NOTEPADS_MARKDOWN_CORE_OK};
            bool _open{};
        };

        class ChineseRenderer final : public IDWriteTextRenderer
        {
          public:
            ChineseRenderer(IDWriteFontCollection *localFonts, float baseline, void *sink,
                            notepads_markdown_math_text_write write) noexcept
                : _localFonts(localFonts), _baseline(baseline), _sink(sink), _write(write)
            {
            }

            HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid, void **output) noexcept override
            {
                if (!output)
                    return E_POINTER;
                *output = nullptr;
                if (iid != __uuidof(IUnknown) && iid != __uuidof(IDWritePixelSnapping) &&
                    iid != __uuidof(IDWriteTextRenderer))
                    return E_NOINTERFACE;
                *output = static_cast<IDWriteTextRenderer *>(this);
                AddRef();
                return S_OK;
            }

            ULONG STDMETHODCALLTYPE AddRef() noexcept override { return ++_references; }
            ULONG STDMETHODCALLTYPE Release() noexcept override { return --_references; }

            HRESULT STDMETHODCALLTYPE IsPixelSnappingDisabled(void *, BOOL *disabled) noexcept override
            {
                if (!disabled)
                    return E_POINTER;
                *disabled = TRUE;
                return S_OK;
            }

            HRESULT STDMETHODCALLTYPE GetCurrentTransform(void *, DWRITE_MATRIX *matrix) noexcept override
            {
                if (!matrix)
                    return E_POINTER;
                *matrix = {1.0f, 0.0f, 0.0f, 1.0f, 0.0f, 0.0f};
                return S_OK;
            }

            HRESULT STDMETHODCALLTYPE GetPixelsPerDip(void *, FLOAT *pixels) noexcept override
            {
                if (!pixels)
                    return E_POINTER;
                *pixels = 1.0f;
                return S_OK;
            }

            HRESULT STDMETHODCALLTYPE DrawGlyphRun(void *, FLOAT x, FLOAT y, DWRITE_MEASURING_MODE,
                const DWRITE_GLYPH_RUN *run, const DWRITE_GLYPH_RUN_DESCRIPTION *, IUnknown *) noexcept override
            {
                if (_status != NOTEPADS_MARKDOWN_CORE_OK)
                    return E_FAIL;
                _status = _write(_sink, nullptr);
                if (_status != NOTEPADS_MARKDOWN_CORE_OK)
                    return E_FAIL;
                if (!run || !run->fontFace || !std::isfinite(x) || !std::isfinite(y) ||
                    !std::isfinite(run->fontEmSize) || run->fontEmSize <= 0.0f ||
                    run->isSideways || (run->bidiLevel & 1) ||
                    run->glyphCount > NOTEPADS_MARKDOWN_MATH_TEXT_MAX_GLYPHS - _glyphs ||
                    (run->glyphCount && !run->glyphIndices))
                    return Fail(NOTEPADS_MARKDOWN_MATH_INVALID_GEOMETRY);
                _glyphs += run->glyphCount;
                if (!run->glyphCount)
                    return S_OK;

                // Only faces from the installed collection are eligible. No
                // font stream or download request leaves this shaping call.
                winrt::com_ptr<IDWriteFont> installed;
                if (FAILED(_localFonts->GetFontFromFontFace(run->fontFace, installed.put())) || !installed)
                    return Fail(NOTEPADS_MARKDOWN_MATH_MISSING_FONT);
                for (UINT32 index = 0; index < run->glyphCount; ++index)
                {
                    if (run->glyphIndices[index] == 0)
                        return Fail(NOTEPADS_MARKDOWN_MATH_MISSING_FONT);
                }

                ChineseOutline geometry(_sink, _write, x, static_cast<double>(y) - _baseline);
                auto hr = run->fontFace->GetGlyphRunOutline(run->fontEmSize, run->glyphIndices,
                    run->glyphAdvances, run->glyphOffsets, run->glyphCount, FALSE, FALSE, &geometry);
                if (geometry.Status() != NOTEPADS_MARKDOWN_CORE_OK)
                    return Fail(geometry.Status());
                return FAILED(hr) ? Fail(NOTEPADS_MARKDOWN_MATH_MISSING_FONT) : S_OK;
            }

            HRESULT STDMETHODCALLTYPE DrawUnderline(void *, FLOAT, FLOAT,
                const DWRITE_UNDERLINE *, IUnknown *) noexcept override
            {
                return Fail(NOTEPADS_MARKDOWN_MATH_INVALID_GEOMETRY);
            }

            HRESULT STDMETHODCALLTYPE DrawStrikethrough(void *, FLOAT, FLOAT,
                const DWRITE_STRIKETHROUGH *, IUnknown *) noexcept override
            {
                return Fail(NOTEPADS_MARKDOWN_MATH_INVALID_GEOMETRY);
            }

            HRESULT STDMETHODCALLTYPE DrawInlineObject(void *, FLOAT, FLOAT, IDWriteInlineObject *,
                BOOL, BOOL, IUnknown *) noexcept override
            {
                return Fail(NOTEPADS_MARKDOWN_MATH_INVALID_GEOMETRY);
            }

            int32_t Status() const noexcept
            {
                return _status != NOTEPADS_MARKDOWN_CORE_OK ? _status :
                       _references != 1 ? NOTEPADS_MARKDOWN_MATH_INVALID_GEOMETRY : NOTEPADS_MARKDOWN_CORE_OK;
            }

          private:
            HRESULT Fail(int32_t status) noexcept
            {
                if (_status == NOTEPADS_MARKDOWN_CORE_OK)
                    _status = status;
                return E_FAIL;
            }

            IDWriteFontCollection *_localFonts;
            float _baseline;
            void *_sink;
            notepads_markdown_math_text_write _write;
            ULONG _references{1};
            UINT32 _glyphs{};
            int32_t _status{NOTEPADS_MARKDOWN_CORE_OK};
        };
    }

    int32_t NOTEPADS_MARKDOWN_FFI_CALL ShapeChineseMathText(const uint16_t *text, uint32_t length, uint32_t style,
                                             void *sink, notepads_markdown_math_text_write write, notepads_markdown_math_text_metrics *metrics) noexcept
    {
        constexpr uint32_t FamilyMask = 0xff;
        constexpr uint32_t KnownStyle = FamilyMask | NOTEPADS_MARKDOWN_MATH_TEXT_BOLD | NOTEPADS_MARKDOWN_MATH_TEXT_ITALIC;
        static_assert(sizeof(notepads_markdown_math_text_metrics) == 24);
        if (!text || !length || length > NOTEPADS_MARKDOWN_MATH_TEXT_MAX_UNITS || !sink || !write || !metrics ||
            (style & ~KnownStyle) || (style & FamilyMask) > NOTEPADS_MARKDOWN_MATH_TEXT_MONOSPACE)
            return NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT;
        if (auto status = write(sink, nullptr); status != NOTEPADS_MARKDOWN_CORE_OK)
            return status;

        std::array<wchar_t, NOTEPADS_MARKDOWN_MATH_TEXT_MAX_UNITS> units;
        for (uint32_t index = 0; index < length; ++index)
            units[index] = static_cast<wchar_t>(text[index]);
        constexpr wchar_t const *Families[] = {L"Times New Roman", L"Segoe UI", L"Consolas"};
        winrt::com_ptr<IDWriteFactory> factory;
        winrt::com_ptr<IDWriteFontCollection> collection;
        winrt::com_ptr<IDWriteTextFormat> format;
        winrt::com_ptr<IDWriteTextLayout> layout;
        if (FAILED(DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED, __uuidof(IDWriteFactory),
                reinterpret_cast<IUnknown **>(factory.put()))) ||
            FAILED(factory->GetSystemFontCollection(collection.put(), FALSE)) ||
            FAILED(factory->CreateTextFormat(Families[style & FamilyMask], collection.get(),
                style & NOTEPADS_MARKDOWN_MATH_TEXT_BOLD ? DWRITE_FONT_WEIGHT_BOLD : DWRITE_FONT_WEIGHT_NORMAL,
                style & NOTEPADS_MARKDOWN_MATH_TEXT_ITALIC ? DWRITE_FONT_STYLE_ITALIC : DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL, EmSize, L"zh-CN", format.put())) ||
            FAILED(format->SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)) ||
            FAILED(factory->CreateTextLayout(units.data(), length, format.get(),
                EmSize * static_cast<float>(NOTEPADS_MARKDOWN_MATH_TEXT_MAX_UNITS), EmSize * static_cast<float>(NOTEPADS_MARKDOWN_MATH_TEXT_MAX_UNITS), layout.put())))
            return NOTEPADS_MARKDOWN_MATH_MISSING_FONT;
        if (auto status = write(sink, nullptr); status != NOTEPADS_MARKDOWN_CORE_OK)
            return status;

        DWRITE_TEXT_METRICS box{};
        DWRITE_LINE_METRICS line{};
        UINT32 lineCount{};
        if (FAILED(layout->GetMetrics(&box)) || FAILED(layout->GetLineMetrics(&line, 1, &lineCount)) ||
            box.lineCount != 1 || lineCount != 1 || !std::isfinite(box.widthIncludingTrailingWhitespace) ||
            !std::isfinite(line.baseline) || !std::isfinite(line.height) ||
            box.widthIncludingTrailingWhitespace < 0.0f || line.baseline < 0.0f || line.height < line.baseline)
            return NOTEPADS_MARKDOWN_MATH_INVALID_GEOMETRY;

        ChineseRenderer renderer(collection.get(), line.baseline, sink, write);
        auto hr = layout->Draw(nullptr, &renderer, 0.0f, 0.0f);
        if (renderer.Status() != NOTEPADS_MARKDOWN_CORE_OK)
            return renderer.Status();
        if (FAILED(hr))
            return NOTEPADS_MARKDOWN_MATH_MISSING_FONT;
        if (auto status = write(sink, nullptr); status != NOTEPADS_MARKDOWN_CORE_OK)
            return status;
        *metrics = {box.widthIncludingTrailingWhitespace / static_cast<double>(EmSize),
                    line.baseline / static_cast<double>(EmSize),
                    (static_cast<double>(line.height) - line.baseline) / EmSize};
        return NOTEPADS_MARKDOWN_CORE_OK;
    }
}
