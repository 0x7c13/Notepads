// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Text/NativeScriptTextRenderer.h"
#include "Text/NativeScriptLayout.h"
#include <bit>
#include <d2d1_3.h>
#include <dwrite_3.h>

namespace Notepads::Markdown::Native
{
    class __declspec(uuid("8d7f6a27-64ca-49be-92f9-946ecda3bd42")) ScriptDrawingEffect final : public IUnknown
    {
        std::atomic<ULONG> _references{1};

      public:
        DWRITE_TEXT_RANGE const range;
        bool const link;
        bool const authoredUnderline;
        bool const suppressUnderline;
        ScriptDrawingEffect(DWRITE_TEXT_RANGE value, bool linked, bool authored, bool suppress) noexcept
            : range(value), link(linked), authoredUnderline(authored), suppressUnderline(suppress)
        {
        }
        static void *operator new(size_t bytes, notepads_markdown_preview_layout *owner) { return AllocatePreview(owner, bytes); }
        static void operator delete(void *memory) noexcept { FreePreview(memory); }
        static void operator delete(void *memory, notepads_markdown_preview_layout *) noexcept { FreePreview(memory); }
        HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid, void **output) noexcept override
        {
            if (!output)
                return E_POINTER;
            *output = nullptr;
            if (iid != __uuidof(IUnknown) && iid != __uuidof(ScriptDrawingEffect))
                return E_NOINTERFACE;
            *output = static_cast<IUnknown *>(this);
            AddRef();
            return S_OK;
        }
        ULONG STDMETHODCALLTYPE AddRef() noexcept override { return ++_references; }
        ULONG STDMETHODCALLTYPE Release() noexcept override
        {
            auto count = --_references;
            if (!count)
                delete this;
            return count;
        }
    };

    namespace
    {
        uint64_t End(DWRITE_TEXT_RANGE value) noexcept
        {
            return uint64_t{value.startPosition} + value.length;
        }
        bool Intersects(DWRITE_TEXT_RANGE a, DWRITE_TEXT_RANGE b) noexcept
        {
            return a.startPosition < End(b) && b.startPosition < End(a);
        }
        bool Contains(DWRITE_TEXT_RANGE a, DWRITE_TEXT_RANGE b) noexcept
        {
            return a.startPosition <= b.startPosition && End(a) >= End(b);
        }
        HRESULT ExceptionStatus() noexcept
        {
            try { throw; }
            catch (std::bad_alloc const &) { return E_OUTOFMEMORY; }
            catch (winrt::hresult_error const &error) { return error.code(); }
            catch (...) { return E_UNEXPECTED; }
        }
        winrt::com_ptr<ScriptDrawingEffect> ReadEffect(IUnknown *value)
        {
            if (!value)
                throw winrt::hresult_error(E_INVALIDARG);
            winrt::com_ptr<ScriptDrawingEffect> result;
            winrt::check_hresult(value->QueryInterface(__uuidof(ScriptDrawingEffect), result.put_void()));
            return result;
        }

        // Used only to close layout/font discovery before publication. No
        // captured coordinates are replayed: real drawing asks DWrite to snap
        // baselines using the current target's transform and DPI.
        class WarmRenderer final : public IDWriteTextRenderer
        {
          public:
            HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid, void **output) noexcept override
            {
                if (!output)
                    return E_POINTER;
                *output = nullptr;
                if (iid != __uuidof(IUnknown) && iid != __uuidof(IDWritePixelSnapping) && iid != __uuidof(IDWriteTextRenderer))
                    return E_NOINTERFACE;
                *output = static_cast<IDWriteTextRenderer *>(this);
                return S_OK;
            }
            ULONG STDMETHODCALLTYPE AddRef() noexcept override { return 1; }
            ULONG STDMETHODCALLTYPE Release() noexcept override { return 1; }
            HRESULT STDMETHODCALLTYPE IsPixelSnappingDisabled(void *, BOOL *value) noexcept override
            {
                if (!value) return E_POINTER;
                *value = TRUE;
                return S_OK;
            }
            HRESULT STDMETHODCALLTYPE GetCurrentTransform(void *, DWRITE_MATRIX *value) noexcept override
            {
                if (!value) return E_POINTER;
                *value = {1, 0, 0, 1, 0, 0};
                return S_OK;
            }
            HRESULT STDMETHODCALLTYPE GetPixelsPerDip(void *, FLOAT *value) noexcept override
            {
                if (!value) return E_POINTER;
                *value = 1;
                return S_OK;
            }
            HRESULT STDMETHODCALLTYPE DrawGlyphRun(void *, FLOAT, FLOAT, DWRITE_MEASURING_MODE,
                DWRITE_GLYPH_RUN const *run, DWRITE_GLYPH_RUN_DESCRIPTION const *, IUnknown *effect) noexcept override
            {
                if (!run || !run->fontFace) return E_POINTER;
                try { (void)ReadEffect(effect); return S_OK; }
                catch (...) { return ExceptionStatus(); }
            }
            HRESULT STDMETHODCALLTYPE DrawUnderline(void *, FLOAT, FLOAT, DWRITE_UNDERLINE const *, IUnknown *) noexcept override { return S_OK; }
            HRESULT STDMETHODCALLTYPE DrawStrikethrough(void *, FLOAT, FLOAT, DWRITE_STRIKETHROUGH const *, IUnknown *) noexcept override { return S_OK; }
            HRESULT STDMETHODCALLTYPE DrawInlineObject(void *client, FLOAT x, FLOAT y, IDWriteInlineObject *object,
                BOOL sideways, BOOL rtl, IUnknown *effect) noexcept override
            {
                return object ? object->Draw(client, this, x, y, sideways, rtl, effect) : E_POINTER;
            }
        };

        struct SelectionClip
        {
            uint32_t line{};
            float left{}, right{};
        };
        struct Decoration
        {
            D2D1_RECT_F rectangle{};
            ID2D1Brush *brush{};
            uint32_t line{};
            DWRITE_TEXT_RANGE range{};
            DWRITE_READING_DIRECTION direction{};
            bool ready{};
        };

        class ScriptTextRenderer final : public IDWriteTextRenderer
        {
            ID2D1DeviceContext *_context;
            NativeScriptLayout const &_geometry;
            ScriptPaint const &_paint;
            D2D1_POINT_2F _origin;
            D2D1_RECT_F _target;
            winrt::com_ptr<ID2D1DeviceContext7> _modern;
            winrt::com_ptr<ID2D1DeviceContext4> _colorLayers;
            winrt::com_ptr<IDWriteFactory4> _factory;
            winrt::com_ptr<ID2D1SolidColorBrush> _palette;
            ReadingVector<SelectionClip> _selection;
            Decoration _underline, _strike;

            ID2D1Brush *Brush(ScriptDrawingEffect const &effect) const noexcept
            {
                return effect.link && _paint.link ? _paint.link : _paint.foreground;
            }
            uint32_t Row(uint32_t position) const
            {
                auto lines = _geometry.Lines();
                auto found = std::upper_bound(lines.begin(), lines.end(), position,
                    [](uint32_t value, NativeScriptLine const &line) { return value < line.start; });
                return found == lines.begin() ? 0 : static_cast<uint32_t>(found - lines.begin() - 1);
            }
            uint32_t DecorationRow(ScriptDrawingEffect const &effect, float baseline) const
            {
                auto lines = _geometry.Lines();
                if (!effect.range.length || lines.empty())
                    throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
                auto first = Row(effect.range.startPosition);
                auto last = Row(static_cast<uint32_t>(End(effect.range) - 1));
                auto y = baseline - _origin.y;
                auto found = std::lower_bound(lines.begin() + first, lines.begin() + last + 1, y,
                    [](NativeScriptLine const &line, float value) { return line.originalBaseline < value; });
                auto best = found == lines.begin() + last + 1 ? last : static_cast<uint32_t>(found - lines.begin());
                if (best > first && std::abs(y - lines[best - 1].originalBaseline) < std::abs(y - lines[best].originalBaseline))
                    --best;
                return best;
            }
            template <typename Paint> HRESULT Split(uint32_t line, ID2D1Brush *normal, Paint &&draw)
            {
                auto selected = _paint.selectionForeground;
                if (!selected || _selection.empty())
                    return draw(normal);
                auto first = std::lower_bound(_selection.begin(), _selection.end(), line,
                    [](SelectionClip const &clip, uint32_t value) { return clip.line < value; });
                if (first == _selection.end() || first->line != line)
                    return draw(normal);
                float cursor = _target.left;
                for (auto current = first; current != _selection.end() && current->line == line; ++current)
                {
                    auto const &clip = *current;
                    auto left = std::clamp(_origin.x + clip.left, _target.left, _target.right);
                    auto right = std::clamp(_origin.x + clip.right, _target.left, _target.right);
                    if (left > cursor)
                    {
                        auto status = Clipped(cursor, left, normal, draw);
                        if (FAILED(status)) return status;
                    }
                    if (right > left)
                    {
                        auto status = Clipped(left, right, selected, draw);
                        if (FAILED(status)) return status;
                    }
                    cursor = std::max(cursor, right);
                }
                return cursor < _target.right ? Clipped(cursor, _target.right, normal, draw) : S_OK;
            }
            template <typename Paint> HRESULT Clipped(float left, float right, ID2D1Brush *brush, Paint &draw)
            {
                // Adjacent normal/selected intervals share one aliased edge;
                // no glyph coverage is composited twice at that edge.
                _context->PushAxisAlignedClip(D2D1::RectF(left, _target.top, right, _target.bottom), D2D1_ANTIALIAS_MODE_ALIASED);
                auto status = draw(brush);
                _context->PopAxisAlignedClip();
                return status;
            }
            HRESULT Glyphs(D2D1_POINT_2F origin, DWRITE_MEASURING_MODE mode,
                           DWRITE_GLYPH_RUN const *run, DWRITE_GLYPH_RUN_DESCRIPTION const *description,
                           ID2D1Brush *brush) noexcept
            {
                if (_modern)
                {
                    _modern->DrawGlyphRunWithColorSupport(origin, run, description, brush, nullptr, 0, mode);
                    return S_OK;
                }
                D2D1_MATRIX_3X2_F world{};
                _context->GetTransform(&world);
                float dpiX{}, dpiY{};
                _context->GetDpi(&dpiX, &dpiY);
                auto pixels = D2D1::Matrix3x2F(world._11, world._12, world._21, world._22, world._31, world._32) *
                    D2D1::Matrix3x2F::Scale(dpiX / 96.0f, dpiY / 96.0f);
                auto transform = std::bit_cast<DWRITE_MATRIX>(pixels);
                auto formats = static_cast<DWRITE_GLYPH_IMAGE_FORMATS>(
                    DWRITE_GLYPH_IMAGE_FORMATS_TRUETYPE | DWRITE_GLYPH_IMAGE_FORMATS_CFF |
                    DWRITE_GLYPH_IMAGE_FORMATS_COLR | DWRITE_GLYPH_IMAGE_FORMATS_SVG |
                    DWRITE_GLYPH_IMAGE_FORMATS_PNG | DWRITE_GLYPH_IMAGE_FORMATS_JPEG |
                    DWRITE_GLYPH_IMAGE_FORMATS_TIFF | DWRITE_GLYPH_IMAGE_FORMATS_PREMULTIPLIED_B8G8R8A8);
                winrt::com_ptr<IDWriteColorGlyphRunEnumerator1> layers;
                auto status = _factory->TranslateColorGlyphRun(origin, run, description, formats, mode, &transform, 0, layers.put());
                if (status == DWRITE_E_NOCOLOR)
                {
                    _context->DrawGlyphRun(origin, run, description, brush, mode);
                    return S_OK;
                }
                if (FAILED(status)) return status;
                for (;;)
                {
                    BOOL hasRun{};
                    status = layers->MoveNext(&hasRun);
                    if (FAILED(status) || !hasRun) return status;
                    DWRITE_COLOR_GLYPH_RUN1 const *layer{};
                    status = layers->GetCurrentRun(&layer);
                    if (FAILED(status)) return status;
                    if (!layer) return E_UNEXPECTED;
                    auto point = D2D1::Point2F(layer->baselineOriginX, layer->baselineOriginY);
                    ID2D1Brush *foreground = brush;
                    if (layer->paletteIndex != 0xffff)
                    {
                        auto color = layer->runColor;
                        _palette->SetColor(D2D1::ColorF(color.r, color.g, color.b, color.a));
                        foreground = _palette.get();
                    }
                    switch (layer->glyphImageFormat)
                    {
                    case DWRITE_GLYPH_IMAGE_FORMATS_TRUETYPE:
                    case DWRITE_GLYPH_IMAGE_FORMATS_CFF:
                    case DWRITE_GLYPH_IMAGE_FORMATS_COLR:
                        _context->DrawGlyphRun(point, &layer->glyphRun, layer->glyphRunDescription, foreground, layer->measuringMode);
                        break;

                    case DWRITE_GLYPH_IMAGE_FORMATS_SVG:
                        _colorLayers->DrawSvgGlyphRun(point, &layer->glyphRun, brush, nullptr, 0, layer->measuringMode);
                        break;
                    case DWRITE_GLYPH_IMAGE_FORMATS_PNG:
                    case DWRITE_GLYPH_IMAGE_FORMATS_JPEG:
                    case DWRITE_GLYPH_IMAGE_FORMATS_TIFF:
                    case DWRITE_GLYPH_IMAGE_FORMATS_PREMULTIPLIED_B8G8R8A8:
                        _colorLayers->DrawColorBitmapGlyphRun(layer->glyphImageFormat, point, &layer->glyphRun, layer->measuringMode);
                        break;
                    default:
                        return E_NOTIMPL;
                    }
                }
            }
            HRESULT Flush(Decoration &pending)
            {
                if (!pending.ready) return S_OK;
                auto draw = [&](ID2D1Brush *brush)
                {
                    _context->FillRectangle(pending.rectangle, brush);
                    return S_OK;
                };
                auto status = _paint.selection && _paint.selectionForeground && Contains(*_paint.selection, pending.range)
                    ? draw(_paint.selectionForeground) : Split(pending.line, pending.brush, draw);
                pending.ready = false;
                return status;
            }
            HRESULT Line(Decoration &pending, ScriptDrawingEffect const &effect, float x, float y,
                         float width, float offset, float thickness, DWRITE_READING_DIRECTION direction)
            {
                auto row = DecorationRow(effect, y);
                auto position = std::max(effect.range.startPosition, _geometry.Lines()[row].start);
                auto top = y + _geometry.BaselineDeltaAt(position) + offset;
                auto rectangle = D2D1::RectF(x, top, x + width, top + thickness);
                auto brush = Brush(effect);
                auto const &owner = _geometry.Lines()[row];
                auto finish = std::min(End(effect.range), uint64_t{owner.start} + owner.length);
                DWRITE_TEXT_RANGE range{position, static_cast<uint32_t>(finish - position)};
                // DirectWrite can split a continuous underline at a strike
                // property boundary. Merge touching pieces before rasterizing
                // so fractional edges do not leave an AA seam.
                constexpr float adjacency = 0.0001f;
                if (pending.ready && pending.line == row && pending.brush == brush && pending.direction == direction &&
                    pending.rectangle.top == rectangle.top && pending.rectangle.bottom == rectangle.bottom &&
                    rectangle.left <= pending.rectangle.right + adjacency &&
                    rectangle.right + adjacency >= pending.rectangle.left)
                {
                    pending.rectangle.left = std::min(pending.rectangle.left, rectangle.left);
                    pending.rectangle.right = std::max(pending.rectangle.right, rectangle.right);
                    auto begin = std::min(pending.range.startPosition, range.startPosition);
                    auto end = std::max(End(pending.range), End(range));
                    pending.range = {begin, static_cast<uint32_t>(end - begin)};
                    return S_OK;
                }
                auto status = Flush(pending);
                if (FAILED(status)) return status;
                pending = {rectangle, brush, row, range, direction, true};
                return S_OK;
            }

          public:
            ScriptTextRenderer(ID2D1DeviceContext *context, IDWriteFactory *factory,
                               NativeScriptLayout const &geometry, D2D1_POINT_2F origin,
                               D2D1_RECT_F target, ScriptPaint const &paint)
                : _context(context), _geometry(geometry), _paint(paint), _origin(origin), _target(target),
                  _selection(ReadingAllocator<SelectionClip>{paint.memory})
            {
                auto status = context->QueryInterface(__uuidof(ID2D1DeviceContext7), _modern.put_void());
                if (FAILED(status) && status != E_NOINTERFACE) winrt::check_hresult(status);
                if (!_modern)
                {
                    winrt::check_hresult(context->QueryInterface(__uuidof(ID2D1DeviceContext4), _colorLayers.put_void()));
                    winrt::check_hresult(factory->QueryInterface(__uuidof(IDWriteFactory4), _factory.put_void()));

                    winrt::check_hresult(context->CreateSolidColorBrush(D2D1::ColorF(D2D1::ColorF::Black), _palette.put()));
                }
                if (paint.selection && paint.selection->length && paint.selectionForeground)
                {
                    ReadingVector<NativeScriptRangeMetric> ranges{ReadingAllocator<NativeScriptRangeMetric>{paint.memory}};
                    geometry.Range(paint.selection->startPosition, paint.selection->length, ranges);
                    for (auto const &range : ranges)
                    {
                        auto const &box = range.metrics;
                        if (!range.ruby && box.width > 0)
                            _selection.push_back({range.line, box.left, box.left + box.width});
                    }
                    std::sort(_selection.begin(), _selection.end(), [](auto const &a, auto const &b)
                    { return a.line != b.line ? a.line < b.line : a.left < b.left; });
                    size_t count{};
                    for (auto const clip : _selection)
                    {
                        if (count && _selection[count - 1].line == clip.line && _selection[count - 1].right >= clip.left)
                            _selection[count - 1].right = std::max(_selection[count - 1].right, clip.right);
                        else
                            _selection[count++] = clip;
                    }
                    _selection.resize(count);
                }
            }
            HRESULT Finish() { auto status = Flush(_underline); return FAILED(status) ? status : Flush(_strike); }
            HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid, void **output) noexcept override
            {
                if (!output) return E_POINTER;
                *output = nullptr;
                if (iid != __uuidof(IUnknown) && iid != __uuidof(IDWritePixelSnapping) && iid != __uuidof(IDWriteTextRenderer))
                    return E_NOINTERFACE;
                *output = static_cast<IDWriteTextRenderer *>(this);
                return S_OK;
            }
            ULONG STDMETHODCALLTYPE AddRef() noexcept override { return 1; }
            ULONG STDMETHODCALLTYPE Release() noexcept override { return 1; }
            HRESULT STDMETHODCALLTYPE IsPixelSnappingDisabled(void *, BOOL *disabled) noexcept override
            {
                if (!disabled) return E_POINTER;
                *disabled = FALSE;
                return S_OK;
            }
            HRESULT STDMETHODCALLTYPE GetCurrentTransform(void *, DWRITE_MATRIX *transform) noexcept override
            {
                if (!transform) return E_POINTER;
                D2D1_MATRIX_3X2_F value{};
                _context->GetTransform(&value);
                *transform = std::bit_cast<DWRITE_MATRIX>(value);
                return S_OK;
            }
            HRESULT STDMETHODCALLTYPE GetPixelsPerDip(void *, FLOAT *value) noexcept override
            {
                if (!value) return E_POINTER;
                float x{}, y{};
                _context->GetDpi(&x, &y);
                *value = x / 96.0f;
                return std::isfinite(*value) && *value > 0 ? S_OK : E_INVALIDARG;
            }
            HRESULT STDMETHODCALLTYPE DrawGlyphRun(void *, FLOAT x, FLOAT y, DWRITE_MEASURING_MODE mode,
                DWRITE_GLYPH_RUN const *run, DWRITE_GLYPH_RUN_DESCRIPTION const *description, IUnknown *value) noexcept override
            {
                if (!run || !description) return E_POINTER;
                try
                {
                    auto effect = ReadEffect(value);
                    DWRITE_TEXT_RANGE range{description->textPosition, description->stringLength};
                    if (!Contains(effect->range, range)) return E_UNEXPECTED;
                    auto origin = D2D1::Point2F(x, y + _geometry.BaselineDeltaAt(range.startPosition));
                    auto draw = [&](ID2D1Brush *brush) { return Glyphs(origin, mode, run, description, brush); };
                    // This also preserves all ink of a fully selected independent
                    // zero-advance mark, whose hit rectangle has no width.
                    if (_paint.selection && _paint.selectionForeground && Contains(*_paint.selection, range))
                        return draw(_paint.selectionForeground);
                    if (!_paint.selection || !Intersects(*_paint.selection, range))
                        return draw(Brush(*effect));
                    return Split(Row(range.startPosition), Brush(*effect), draw);
                }
                catch (...) { return ExceptionStatus(); }
            }
            HRESULT STDMETHODCALLTYPE DrawUnderline(void *, FLOAT x, FLOAT y, DWRITE_UNDERLINE const *line, IUnknown *value) noexcept override
            {
                if (!line) return E_POINTER;
                try
                {
                    auto effect = ReadEffect(value);
                    if (effect->suppressUnderline || (effect->link && !effect->authoredUnderline &&
                        (!_paint.hover || !Intersects(*_paint.hover, effect->range)))) return S_OK;
                    return Line(_underline, *effect, x, y, line->width, line->offset, line->thickness, line->readingDirection);
                }
                catch (...) { return ExceptionStatus(); }
            }
            HRESULT STDMETHODCALLTYPE DrawStrikethrough(void *, FLOAT x, FLOAT y, DWRITE_STRIKETHROUGH const *line, IUnknown *value) noexcept override
            {
                if (!line) return E_POINTER;
                try { auto effect = ReadEffect(value); return Line(_strike, *effect, x, y, line->width, line->offset, line->thickness, line->readingDirection); }
                catch (...) { return ExceptionStatus(); }
            }
            HRESULT STDMETHODCALLTYPE DrawInlineObject(void *client, FLOAT x, FLOAT y, IDWriteInlineObject *object,
                BOOL sideways, BOOL rtl, IUnknown *effect) noexcept override
            {
                return object ? object->Draw(client, this, x, y, sideways, rtl, effect) : E_POINTER;
            }
        };
    }

    std::shared_ptr<NativeScriptDrawing> NativeScriptDrawing::Create(
        notepads_markdown_preview_layout *owner, std::shared_ptr<NativeScriptLayout const> geometry,
        IDWriteFactory *factory, std::span<notepads_markdown_preview_run const> runs)
    {
        return std::allocate_shared<NativeScriptDrawing>(PreviewAllocator<NativeScriptDrawing>{owner},
            ConstructionKey{}, owner, std::move(geometry), factory, runs);
    }
    NativeScriptDrawing::NativeScriptDrawing(ConstructionKey, notepads_markdown_preview_layout *owner,
        std::shared_ptr<NativeScriptLayout const> geometry, IDWriteFactory *factory,
        std::span<notepads_markdown_preview_run const> runs)
        : _owner(owner), _geometry(std::move(geometry)), _runs(PreviewAllocator<notepads_markdown_preview_run>{owner}),
          _effects(PreviewAllocator<winrt::com_ptr<ScriptDrawingEffect>>{owner})
    {
        _factory.copy_from(factory);
        if (!_owner || !_geometry || !_factory || _geometry->Resolved())
            throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
        uint32_t position{};
        for (auto const &run : runs)
        {
            if (run.start != position || run.end <= run.start)
                throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
            position = run.end;
        }
        _length = position;
        _runs.assign(runs.begin(), runs.end());
    }
    NativeScriptDrawing::~NativeScriptDrawing() = default;

    void NativeScriptDrawing::Prepare(IDWriteTextLayout *layout, std::span<PreviewLinkVisual const> links,
                                      std::span<PreviewImageVisual const> images)
    {
        if (!layout || _layout || _geometry->Resolved()) throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
        PreviewVector<uint32_t> boundaries{PreviewAllocator<uint32_t>{_owner}};
        boundaries.push_back(0);
        boundaries.push_back(_length);
        for (size_t index = 1; index < _runs.size(); ++index)
        {
            auto const &before = _runs[index - 1];
            auto const &after = _runs[index];
            // IR atoms (entities, escapes and source fragments) are not shaping
            // boundaries. Split effects when the geometry or the persistent
            // underline inside a hoverable link changes.
            if (before.size_scale != after.size_scale || before.relative_em != after.relative_em ||
                before.absolute_dip != after.absolute_dip ||
                ((before.flags ^ after.flags) & NOTEPADS_MARKDOWN_PREVIEW_RUN_UNDERLINE))
                boundaries.push_back(after.start);
        }
        uint32_t linkEnd{};
        for (auto const &link : links)
        {
            if (link.start < linkEnd || link.start > _length || link.length > _length - link.start)
                throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_RANGE};
            linkEnd = link.start + link.length;
            boundaries.push_back(link.start);
            boundaries.push_back(link.start + link.length);
            winrt::check_hresult(layout->SetUnderline(TRUE, {link.start, link.length}));
        }
        uint32_t imageEnd{};
        for (auto const &image : images)
        {
            if (image.offset < imageEnd || image.offset >= _length) throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_RANGE};
            imageEnd = image.offset + 1;
            if (!image.hasPixels)
            {
                boundaries.push_back(image.offset);
                boundaries.push_back(image.offset + 1);
                winrt::check_hresult(layout->SetUnderline(FALSE, {image.offset, 1}));
            }
        }
        auto ruby = _geometry->RubyParts();
        for (auto const &part : ruby)
        {
            boundaries.push_back(part.start);
            boundaries.push_back(part.end);
            winrt::check_hresult(layout->SetUnderline(FALSE, {part.start, part.end - part.start}));
        }
        std::sort(boundaries.begin(), boundaries.end());
        boundaries.erase(std::unique(boundaries.begin(), boundaries.end()), boundaries.end());
        _effects.reserve(boundaries.size());
        size_t runIndex{}, linkIndex{}, imageIndex{}, rubyIndex{};
        for (size_t index = 1; index < boundaries.size(); ++index)
        {
            auto start = boundaries[index - 1];
            auto length = boundaries[index] - start;
            if (!length) continue;
            while (_runs[runIndex].end <= start)
                ++runIndex;
            while (linkIndex < links.size() && uint64_t{links[linkIndex].start} + links[linkIndex].length <= start)
                ++linkIndex;
            while (imageIndex < images.size() && images[imageIndex].offset < start)
                ++imageIndex;
            while (rubyIndex < ruby.size() && ruby[rubyIndex].end <= start)
                ++rubyIndex;
            bool linked = linkIndex < links.size() && links[linkIndex].start <= start;
            bool suppress = (imageIndex < images.size() && images[imageIndex].offset == start && !images[imageIndex].hasPixels) ||
                (rubyIndex < ruby.size() && ruby[rubyIndex].start <= start);
            winrt::com_ptr<ScriptDrawingEffect> effect;
            bool authored = (_runs[runIndex].flags & NOTEPADS_MARKDOWN_PREVIEW_RUN_UNDERLINE) != 0;
            effect.attach(new (_owner) ScriptDrawingEffect({start, length}, linked, authored, suppress));
            winrt::check_hresult(layout->SetDrawingEffect(effect.get(), {start, length}));
            _effects.push_back(std::move(effect));
        }
        _layout.copy_from(layout);
    }
    void NativeScriptDrawing::Seal(IDWriteTextLayout *layout)
    {
        if (_sealed || !_layout || layout != _layout.get() || !_geometry->Resolved()) throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
        WarmRenderer renderer;
        winrt::check_hresult(layout->Draw(nullptr, &renderer, 0, 0));
        _sealed = true;
    }
    void NativeScriptDrawing::Draw(ID2D1DeviceContext *context, IDWriteTextLayout *layout, D2D1_POINT_2F origin,
                                   ScriptPaint const &paint) const
    {
        if (!context || !layout || layout != _layout.get() || !_sealed || !paint.foreground)
            throw winrt::hresult_error(E_INVALIDARG);
        // Only selection splits need a finite clip. Static drawings record every
        // line and keep horizontal ink that overhangs the DirectWrite layout box.
        D2D1_RECT_F target{};
        if (paint.selection && paint.selectionForeground)
        {
            D2D1_MATRIX_3X2_F world{};
            context->GetTransform(&world);
            auto inverse = D2D1::Matrix3x2F(world._11, world._12, world._21, world._22, world._31, world._32);
            if (!inverse.Invert()) throw winrt::hresult_error(E_INVALIDARG);
            auto size = context->GetSize();
            auto a = inverse.TransformPoint(D2D1::Point2F(0, 0));
            auto b = inverse.TransformPoint(D2D1::Point2F(size.width, 0));
            auto c = inverse.TransformPoint(D2D1::Point2F(0, size.height));
            auto d = inverse.TransformPoint(D2D1::Point2F(size.width, size.height));
            target = D2D1::RectF(std::min({a.x, b.x, c.x, d.x}), std::min({a.y, b.y, c.y, d.y}),
                                 std::max({a.x, b.x, c.x, d.x}), std::max({a.y, b.y, c.y, d.y}));
            if (!std::isfinite(target.left) || !std::isfinite(target.top) ||
                !std::isfinite(target.right) || !std::isfinite(target.bottom))
                throw winrt::hresult_error(E_INVALIDARG);
        }
        ScriptTextRenderer renderer{context, _factory.get(), *_geometry, origin, target, paint};
        winrt::check_hresult(layout->Draw(nullptr, &renderer, origin.x, origin.y));
        winrt::check_hresult(renderer.Finish());

        ReadingVector<DWRITE_HIT_TEST_METRICS> selection{ReadingAllocator<DWRITE_HIT_TEST_METRICS>{paint.memory}};
        for (auto const &part : _geometry->RubyParts())
        {
            DWRITE_TEXT_RANGE range{part.start, part.end - part.start};
            auto normal = part.linked && paint.link ? paint.link : paint.foreground;
            bool underline = part.linked && !part.authoredUnderline && paint.hover && Intersects(*paint.hover, range);
            auto point = D2D1::Point2F(origin.x + part.x, origin.y + part.y);
            auto draw = [&](ID2D1Brush *brush)
            {
                context->DrawTextLayout(point, part.layout.get(), brush, D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT);
                if (underline)
                {
                    auto top = point.y + part.baseline + 1;
                    context->FillRectangle(D2D1::RectF(point.x, top, point.x + part.width, top + 1), brush);
                }
            };
            if (!paint.selectionForeground || !paint.selection || !Intersects(*paint.selection, range))
            {
                draw(normal);
                continue;
            }
            if (Contains(*paint.selection, range))
            {
                draw(paint.selectionForeground);
                continue;
            }
            // Clip independent child layouts against their own selected advances.
            // Disjoint clips keep antialiased glyph coverage from drawing twice.
            part.Range(paint.selection->startPosition, static_cast<uint32_t>(End(*paint.selection)), selection);
            std::sort(selection.begin(), selection.end(), [](auto const &a, auto const &b) { return a.left < b.left; });
            auto clipped = [&](float left, float right, ID2D1Brush *brush)
            {
                if (right <= left)
                    return;
                context->PushAxisAlignedClip(D2D1::RectF(left, target.top, right, target.bottom), D2D1_ANTIALIAS_MODE_ALIASED);
                draw(brush);
                context->PopAxisAlignedClip();
            };
            auto cursor = target.left;
            for (auto const &box : selection)
            {
                auto left = std::max(cursor, origin.x + box.left);
                auto right = std::min(target.right, origin.x + box.left + box.width);
                if (left >= right)
                    continue;
                clipped(cursor, left, normal);
                clipped(left, right, paint.selectionForeground);
                cursor = right;
            }
            clipped(cursor, target.right, normal);
        }
    }
    void NativeScriptDrawing::Draw(ID2D1DeviceContext *context, IDWriteTextLayout *layout, D2D1_POINT_2F origin,
                                   ID2D1Brush *foreground) const
    {
        ScriptPaint paint;
        paint.foreground = foreground;
        Draw(context, layout, origin, paint);
    }
}
