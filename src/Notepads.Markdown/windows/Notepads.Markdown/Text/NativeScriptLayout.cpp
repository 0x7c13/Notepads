// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Text/NativeScriptLayout.h"
#include <cfloat>
#include <limits>

namespace Notepads::Markdown::Native
{
    namespace
    {
        constexpr uint32_t FontMask = NOTEPADS_MARKDOWN_PREVIEW_SCRIPT_FONT_MASK;

        uint32_t RunFontFlags(uint32_t flags) noexcept
        {
            return (flags & FontMask) | (flags & NOTEPADS_MARKDOWN_PREVIEW_RUN_MONOSPACE ? 8u : 0u);
        }

        void RequireScript(bool condition, int32_t status = NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE)
        {
            if (!condition)
                throw PreviewFailure{status, L"script layout"};
        }

        float Coordinate(double value)
        {
            RequireScript(std::isfinite(value) && value >= -FLT_MAX && value <= FLT_MAX,
                          NOTEPADS_MARKDOWN_CORE_ARITHMETIC_OVERFLOW);
            return static_cast<float>(value);
        }

        bool Scripted(notepads_markdown_preview_run const &run) noexcept
        {
            return run.size_scale != 1 || run.relative_em != 0 || run.absolute_dip != 0;
        }

        bool ValidStyle(double scale, double em, double dip) noexcept
        {
            return std::isfinite(scale) && scale > 0 && scale <= 1 &&
                   std::isfinite(em) && std::isfinite(dip);
        }

        void CheckMetricQuery(HRESULT status)
        {
            if (FAILED(status) && status != HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER))
                winrt::check_hresult(status);
        }

        // A synchronous, stack-only observer. It never escapes TextLayout::Draw,
        // never allocates in a callback, and never invokes a graphics API.
        class FontProbe final : public IDWriteTextRenderer
        {
          public:
            DWRITE_FONT_METRICS metrics{};
            bool found{};
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
            ULONG STDMETHODCALLTYPE AddRef() noexcept override { return 1; }
            ULONG STDMETHODCALLTYPE Release() noexcept override { return 1; }
            HRESULT STDMETHODCALLTYPE IsPixelSnappingDisabled(void *, BOOL *value) noexcept override
            {
                if (!value)
                    return E_POINTER;
                *value = TRUE;
                return S_OK;
            }
            HRESULT STDMETHODCALLTYPE GetCurrentTransform(void *, DWRITE_MATRIX *value) noexcept override
            {
                if (!value)
                    return E_POINTER;
                *value = {1, 0, 0, 1, 0, 0};
                return S_OK;
            }
            HRESULT STDMETHODCALLTYPE GetPixelsPerDip(void *, FLOAT *value) noexcept override
            {
                if (!value)
                    return E_POINTER;
                *value = 1;
                return S_OK;
            }
            HRESULT STDMETHODCALLTYPE DrawGlyphRun(void *, FLOAT, FLOAT, DWRITE_MEASURING_MODE,
                                                   DWRITE_GLYPH_RUN const *run,
                                                   DWRITE_GLYPH_RUN_DESCRIPTION const *, IUnknown *) noexcept override
            {
                if (!run || !run->fontFace)
                    return E_POINTER;
                if (!found)
                {
                    run->fontFace->GetMetrics(&metrics);
                    found = true;
                }
                return S_OK;
            }
            HRESULT STDMETHODCALLTYPE DrawUnderline(void *, FLOAT, FLOAT, DWRITE_UNDERLINE const *,
                                                     IUnknown *) noexcept override { return S_OK; }
            HRESULT STDMETHODCALLTYPE DrawStrikethrough(void *, FLOAT, FLOAT, DWRITE_STRIKETHROUGH const *,
                                                         IUnknown *) noexcept override { return S_OK; }
            HRESULT STDMETHODCALLTYPE DrawInlineObject(void *, FLOAT, FLOAT, IDWriteInlineObject *, BOOL,
                                                        BOOL, IUnknown *) noexcept override { return S_OK; }
        };
    }

    std::shared_ptr<NativeScriptLayout> NativeScriptLayout::Create(
        notepads_markdown_preview_layout *owner, IDWriteFactory *factory, float parentFontSize,
        std::wstring_view family, std::wstring_view codeFamily, DWRITE_FONT_WEIGHT baseWeight,
        std::span<notepads_markdown_preview_run const> runs, std::span<notepads_markdown_preview_script_scope const> scopes,
        uint32_t utf16Length)
    {
        return std::allocate_shared<NativeScriptLayout>(PreviewAllocator<NativeScriptLayout>{owner},
            ConstructionKey{}, owner, factory, parentFontSize, family, codeFamily, baseWeight,
            runs, scopes, utf16Length);
    }

    NativeScriptLayout::NativeScriptLayout(
        ConstructionKey, notepads_markdown_preview_layout *owner, IDWriteFactory *factory, float parentFontSize,
        std::wstring_view family, std::wstring_view codeFamily, DWRITE_FONT_WEIGHT baseWeight,
        std::span<notepads_markdown_preview_run const> runs, std::span<notepads_markdown_preview_script_scope const> scopes,
        uint32_t utf16Length)
        : _owner(owner), _parentFontSize(parentFontSize), _length(utf16Length),
          _family(PreviewAllocator<wchar_t>{owner}), _codeFamily(PreviewAllocator<wchar_t>{owner}),
          _runs(PreviewAllocator<notepads_markdown_preview_run>{owner}),
          _scopes(PreviewAllocator<notepads_markdown_preview_script_scope>{owner}),
          _lines(PreviewAllocator<NativeScriptLine>{owner}),
          _metrics(PreviewAllocator<DWRITE_LINE_METRICS>{owner}),
          _drawSpans(PreviewAllocator<NativeScriptDrawSpan>{owner}),
          _fragments(PreviewAllocator<Fragment>{owner}),
          _fragmentLines(PreviewAllocator<FragmentLine>{owner}),
          _ruby(PreviewAllocator<NativeRubyPart>{owner}),
          _rubyCaretStops(PreviewAllocator<uint32_t>{owner})
    {
        RequireScript(owner && factory && std::isfinite(parentFontSize) && parentFontSize > 0 &&
                      !family.empty() && !codeFamily.empty(), NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT);
        _family.assign(family);
        _codeFamily.assign(codeFamily);
        _runs.assign(runs.begin(), runs.end());
        _scopes.assign(scopes.begin(), scopes.end());
        _plain = {0, _length, 0, NOTEPADS_MARKDOWN_PREVIEW_NONE, 1, 0, 0};
        std::array<bool, 12> needed{};
        needed[0] = true;
        uint32_t position{};
        for (auto const &run : _runs)
        {
            RequireScript(run.start == position && run.start < run.end && run.end <= _length &&
                          ValidStyle(run.size_scale, run.relative_em, run.absolute_dip));
            needed[RunFontFlags(run.flags)] = true;
            position = run.end;
        }
        RequireScript(position == _length);
        for (auto const &scope : _scopes)
        {
            RequireScript(scope.start <= scope.end && scope.end <= _length &&
                          !(scope.font_flags & ~NOTEPADS_MARKDOWN_PREVIEW_SCRIPT_SCOPE_MASK) &&
                          ValidStyle(scope.size_scale, scope.relative_em, scope.absolute_dip));
            needed[scope.font_flags & FontMask] = true;
        }
        for (uint32_t flags = 0; flags < needed.size(); ++flags)
        {
            if (!needed[flags])
                continue;
            winrt::com_ptr<IDWriteTextFormat> format;
            auto const &name = flags & 8 ? _codeFamily : _family;
            winrt::check_hresult(factory->CreateTextFormat(
                name.c_str(), nullptr, flags & 1 ? DWRITE_FONT_WEIGHT_BOLD : baseWeight,
                flags & 2 ? DWRITE_FONT_STYLE_ITALIC : DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL, 1, L"", format.put()));
            winrt::com_ptr<IDWriteTextLayout> layout;
            winrt::check_hresult(factory->CreateTextLayout(L"M", 1, format.get(), 100, 100, layout.put()));
            FontProbe probe;
            winrt::check_hresult(layout->Draw(nullptr, &probe, 0, 0));
            RequireScript(probe.found && probe.metrics.designUnitsPerEm != 0);
            DWRITE_LINE_METRICS line{};
            UINT32 count{};
            winrt::check_hresult(layout->GetLineMetrics(&line, 1, &count));
            RequireScript(count == 1 && std::isfinite(line.height) && line.height > 0 &&
                          std::isfinite(line.baseline) && line.baseline >= 0 && line.baseline <= line.height);
            auto &value = _fonts[flags];
            value.ascent = double(probe.metrics.ascent) / probe.metrics.designUnitsPerEm;
            value.descent = double(probe.metrics.descent) / probe.metrics.designUnitsPerEm;
            value.defaultAscent = line.baseline;
            value.defaultDescent = double(line.height) - line.baseline;
            value.ready = true;
        }
    }

    notepads_markdown_preview_run const &NativeScriptLayout::StyleAt(uint32_t offset) const
    {
        RequireScript(offset <= _length, NOTEPADS_MARKDOWN_CORE_INVALID_RANGE);
        if (_runs.empty())
            return _plain;
        if (offset == _length)
            return _runs.back();
        auto found = std::upper_bound(_runs.begin(), _runs.end(), offset,
            [](uint32_t value, notepads_markdown_preview_run const &run) { return value < run.end; });
        RequireScript(found != _runs.end() && found->start <= offset);
        return *found;
    }

    float NativeScriptLayout::Size(double scale) const
    {
        // Clamp in double before casting. FLT_MIN is normal on all three ABIs,
        // avoiding zero conversion and any dependency on denormal/FTZ state.
        return Coordinate(std::max(double(std::numeric_limits<float>::min()), double(_parentFontSize) * scale));
    }

    double NativeScriptLayout::Displacement(notepads_markdown_preview_run const &run) const
    {
        auto value = double(_parentFontSize) * run.relative_em + run.absolute_dip;
        RequireScript(std::isfinite(value), NOTEPADS_MARKDOWN_CORE_ARITHMETIC_OVERFLOW);
        return value;
    }

    NativeScriptLayout::FontMetrics const &NativeScriptLayout::Font(uint32_t flags) const
    {
        flags &= FontMask;
        RequireScript(flags < _fonts.size() && _fonts[flags].ready);
        return _fonts[flags];
    }

    float NativeScriptLayout::FontSizeAt(uint32_t offset) const { return Size(StyleAt(offset).size_scale); }
    uint32_t NativeScriptLayout::FontFlagsAt(uint32_t offset) const { return RunFontFlags(StyleAt(offset).flags); }

    void NativeScriptLayout::ApplyFontSizes(IDWriteTextLayout *layout) const
    {
        RequireScript(layout != nullptr && !_resolved, NOTEPADS_MARKDOWN_CORE_INVALID_STATE);
        for (auto const &run : _runs)
            winrt::check_hresult(layout->SetFontSize(Size(run.size_scale), {run.start, run.end - run.start}));
    }

    DWRITE_HIT_TEST_METRICS NativeRubyPart::Translate(DWRITE_HIT_TEST_METRICS value) const
    {
        RequireScript(value.textPosition <= end - start && value.length <= end - start - value.textPosition);
        value.textPosition += start;
        value.left = Coordinate(double(value.left) + x);
        value.top = Coordinate(double(value.top) + y);
        return NativeScriptLayout::CanonicalHitMetric(value);
    }

    void NativeRubyPart::Range(uint32_t begin, uint32_t finish,
                               ReadingVector<DWRITE_HIT_TEST_METRICS> &output) const
    {
        begin = std::max(begin, start);
        finish = std::min(finish, end);
        output.clear();
        if (begin >= finish)
            return;
        UINT32 count{};
        CheckMetricQuery(layout->HitTestTextRange(begin - start, finish - begin, 0, 0, nullptr, 0, &count));
        output.resize(count);
        UINT32 actual{};
        if (count)
            winrt::check_hresult(layout->HitTestTextRange(begin - start, finish - begin, 0, 0,
                                                           output.data(), count, &actual));
        RequireScript(actual == count);
        for (auto &metric : output)
        {
            metric = Translate(metric);
            const auto stop = std::min(finish, metric.textPosition + metric.length);
            metric.textPosition = std::max(begin, metric.textPosition);
            metric.length = stop > metric.textPosition ? stop - metric.textPosition : 0;
        }
    }

    DWRITE_INLINE_OBJECT_METRICS NativeScriptLayout::AddRuby(
        IDWriteFactory *factory, std::wstring_view text, notepads_markdown_preview_ruby_unit const &unit,
        DWRITE_FONT_WEIGHT weight)
    {
        RequireScript(!_layout && factory && text.size() == _length && unit.start < unit.base_end &&
                      unit.base_end < unit.end && unit.end <= _length &&
                      (_ruby.empty() || _ruby.back().end <= unit.start));
        const auto &style = StyleAt(unit.start);
        const auto flags = RunFontFlags(style.flags);
        const auto &family = flags & 8 ? _codeFamily : _family;
        const auto fontSize = Size(style.size_scale);
        auto makePart = [&](uint32_t start, uint32_t end, float size)
        {
            NativeRubyPart part;
            part.start = start;
            part.end = end;
            part.linked = style.link != NOTEPADS_MARKDOWN_PREVIEW_NONE;
            part.authoredUnderline = (style.flags & NOTEPADS_MARKDOWN_PREVIEW_RUN_UNDERLINE) != 0;
            winrt::com_ptr<IDWriteTextFormat> format;
            winrt::check_hresult(factory->CreateTextFormat(family.c_str(), nullptr,
                flags & 1 ? DWRITE_FONT_WEIGHT_BOLD : weight,
                flags & 2 ? DWRITE_FONT_STYLE_ITALIC : DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL, size, L"", format.put()));
            winrt::check_hresult(format->SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP));
            winrt::check_hresult(factory->CreateTextLayout(text.data() + start, end - start, format.get(),
                                                           100000.0f, 100000.0f, part.layout.put()));
            const DWRITE_TEXT_RANGE range{0, end - start};
            if (style.flags & 4u)
                winrt::check_hresult(part.layout->SetStrikethrough(TRUE, range));
            if (part.authoredUnderline)
                winrt::check_hresult(part.layout->SetUnderline(TRUE, range));
            DWRITE_TEXT_METRICS metrics{};
            DWRITE_LINE_METRICS line{};
            UINT32 count{};
            winrt::check_hresult(part.layout->GetMetrics(&metrics));
            winrt::check_hresult(part.layout->GetLineMetrics(&line, 1, &count));
            RequireScript(count == 1 && line.length == end - start && line.newlineLength == 0 &&
                          std::isfinite(line.height) && line.height > 0 && std::isfinite(line.baseline) &&
                          line.baseline >= 0 && line.baseline <= line.height &&
                          std::isfinite(metrics.widthIncludingTrailingWhitespace) &&
                          metrics.widthIncludingTrailingWhitespace >= 0);
            part.width = metrics.widthIncludingTrailingWhitespace;
            part.height = line.height;
            part.baseline = line.baseline;
            return part;
        };
        auto base = makePart(unit.start, unit.base_end, fontSize);
        auto annotation = makePart(unit.base_end, unit.end, fontSize * 0.5f);
        const auto width = std::max(base.width, annotation.width);
        const auto gap = std::max(1.0f, fontSize * 0.06f);
        base.x = (width - base.width) * 0.5f;
        annotation.x = (width - annotation.width) * 0.5f;
        base.y = annotation.height + gap;
        const DWRITE_INLINE_OBJECT_METRICS metrics{width, base.y + base.height, base.y + base.baseline, FALSE};
        _ruby.push_back(std::move(base));
        _ruby.push_back(std::move(annotation));
        return metrics;
    }

    std::span<NativeRubyPart const> NativeScriptLayout::RubyAt(uint32_t offset) const
    {
        auto first = std::lower_bound(_ruby.begin(), _ruby.end(), offset,
            [](NativeRubyPart const &part, uint32_t point) { return part.end <= point; });
        if (first == _ruby.end() || offset < first->start)
            return {};
        auto index = static_cast<size_t>(first - _ruby.begin()) & ~size_t{1};
        return std::span<NativeRubyPart const>{_ruby}.subspan(index, 2);
    }

    void NativeScriptLayout::ResolveRuby()
    {
        PreviewVector<DWRITE_CLUSTER_METRICS> clusters{PreviewAllocator<DWRITE_CLUSTER_METRICS>{_owner}};
        for (size_t i = 0; i < _ruby.size(); i += 2)
        {
            auto &base = _ruby[i];
            auto &annotation = _ruby[i + 1];
            auto fragment = std::lower_bound(_fragments.begin(), _fragments.end(), base.start,
                [](Fragment const &value, uint32_t offset) { return value.raw.textPosition < offset; });
            RequireScript(fragment != _fragments.end() && !fragment->raw.isText &&
                          fragment->raw.textPosition == base.start &&
                          fragment->raw.length == annotation.end - base.start);
            const auto &box = fragment->final;
            for (auto *part : {&base, &annotation})
            {
                part->x += box.left;
                part->y += box.top;
                part->line = fragment->line;
                auto &bounds = _fragmentLines[part->line];
                bounds.top = std::min(bounds.top, part->y);
                bounds.bottom = std::max(bounds.bottom, Coordinate(double(part->y) + part->height));
                UINT32 count{};
                CheckMetricQuery(part->layout->GetClusterMetrics(nullptr, 0, &count));
                clusters.resize(count);
                UINT32 actual{};
                if (count)
                    winrt::check_hresult(part->layout->GetClusterMetrics(clusters.data(), count, &actual));
                RequireScript(actual == count);
                auto stop = part->start;
                for (auto const &cluster : clusters)
                {
                    RequireScript(cluster.length > 0 && cluster.length <= part->end - stop);
                    stop += cluster.length;
                    _rubyCaretStops.push_back(stop);
                }
                RequireScript(stop == part->end);
            }
        }
    }

    uint32_t NativeScriptLayout::LineAt(uint32_t offset) const
    {
        RequireScript(offset <= _length && !_lines.empty(), NOTEPADS_MARKDOWN_CORE_INVALID_RANGE);
        auto found = std::upper_bound(_lines.begin(), _lines.end(), offset,
            [](uint32_t value, NativeScriptLine const &line) { return value < line.start; });
        RequireScript(found != _lines.begin());
        return static_cast<uint32_t>(std::distance(_lines.begin(), found) - 1);
    }

    uint32_t NativeScriptLayout::EmptyScopeLine(uint32_t offset) const
    {
        // The browser does not construct the empty row following a terminal
        // hard break, even when empty styled elements follow that break.
        if (_terminalBreak && offset == _length)
            return static_cast<uint32_t>(_lines.size());
        auto line = LineAt(offset);
        // Clean browser probe: a zero-width empty inline at a soft wrap stays
        // on the preceding line. After an explicit break it starts the next.
        if (line && offset == _lines[line].start && !_lines[line - 1].newlineLength)
            --line;
        return line;
    }

    void NativeScriptLayout::Resolve(IDWriteTextLayout *layout, float minimumLineHeight)
    {
        RequireScript(layout && !_resolved && !_layout, NOTEPADS_MARKDOWN_CORE_INVALID_STATE);
        RequireScript(std::isfinite(minimumLineHeight) && minimumLineHeight >= 0, NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT);
        _layout.copy_from(layout);
        UINT32 count{};
        CheckMetricQuery(layout->GetLineMetrics(nullptr, 0, &count));
        RequireScript(count != 0);
        _metrics.resize(count);
        UINT32 actual{};
        winrt::check_hresult(layout->GetLineMetrics(_metrics.data(), count, &actual));
        RequireScript(actual == count);
        float discardedHeight{};
        if (count > 1 && !_metrics.back().length && _metrics[count - 2].newlineLength)
        {
            _terminalBreak = _metrics[count - 2].newlineLength;
            discardedHeight = _metrics.back().height;
            _metrics.pop_back();
            --count;
        }
        DWRITE_TEXT_METRICS text{};
        winrt::check_hresult(layout->GetMetrics(&text));
        RequireScript(std::isfinite(text.top) && std::isfinite(text.height) && text.height >= 0);
        _lines.reserve(count);
        uint32_t position{};
        double top = text.top;
        for (auto const &line : _metrics)
        {
            RequireScript(line.length <= _length - position && line.newlineLength <= line.length &&
                          std::isfinite(line.height) && line.height > 0 && std::isfinite(line.baseline) &&
                          line.baseline >= 0 && line.baseline <= line.height);
            _lines.push_back({position, line.length, line.newlineLength, Coordinate(top),
                              Coordinate(top + line.baseline), line.height, 0, 0, 0});
            top += line.height;
            position += line.length;
        }
        RequireScript(position == _length);

        // Range-max tags avoid scopes-times-lines work for long nested spans.
        // All known scratch capacity remains charged to the layout lease.
        size_t leaves = 1;
        while (leaves < count)
        {
            RequireScript(leaves <= SIZE_MAX / 2, NOTEPADS_MARKDOWN_CORE_ARITHMETIC_OVERFLOW);
            leaves *= 2;
        }
        RequireScript(leaves <= SIZE_MAX / 2, NOTEPADS_MARKDOWN_CORE_ARITHMETIC_OVERFLOW);
        PreviewVector<double> ascent{PreviewAllocator<double>{_owner}};
        PreviewVector<double> descent{PreviewAllocator<double>{_owner}};
        ascent.resize(leaves * 2);
        descent.resize(leaves * 2);
        auto contribute = [&](uint32_t start, uint32_t end, uint32_t flags,
                              double scale, double em, double dip)
        {
            auto const &font = Font(flags);
            double size = Size(scale);
            // Chrome's font metrics are rounded before negative half-leading
            // is floored. A zero-height box is not situated at its baseline.
            double a = std::round(font.ascent * size);
            double d = std::round(font.descent * size);
            double height = flags & NOTEPADS_MARKDOWN_PREVIEW_SCRIPT_CODE_LINE_HEIGHT ? std::round(size * 1.357 * 64) / 64 : 0;
            double leading = std::floor((height - a - d) / 2);
            double dy = double(_parentFontSize) * em + dip;
            RequireScript(std::isfinite(dy) && std::isfinite(height), NOTEPADS_MARKDOWN_CORE_ARITHMETIC_OVERFLOW);
            double above = std::max(0.0, a + leading - dy);
            double below = std::max(0.0, height - a - leading + dy);
            auto first = start == end ? EmptyScopeLine(start) : LineAt(start);
            if (first == count)
                return;
            auto last = start == end ? first : LineAt(end - 1);
            size_t left = leaves + first, right = leaves + last + 1;
            while (left < right)
            {
                if (left & 1)
                {
                    ascent[left] = std::max(ascent[left], above);
                    descent[left] = std::max(descent[left], below);
                    ++left;
                }
                if (right & 1)
                {
                    --right;
                    ascent[right] = std::max(ascent[right], above);
                    descent[right] = std::max(descent[right], below);
                }
                left /= 2;
                right /= 2;
            }
        };
        for (auto const &scope : _scopes)
            if (!(scope.font_flags & NOTEPADS_MARKDOWN_PREVIEW_SCRIPT_SHAPE_ONLY))
                contribute(scope.start, scope.end, scope.font_flags,
                           scope.size_scale, scope.relative_em, scope.absolute_dip);
        for (auto const &run : _runs)
            if (Scripted(run))
                contribute(run.start, run.end, RunFontFlags(run.flags),
                           run.size_scale, run.relative_em, run.absolute_dip);

        UINT32 clusterCount{};
        CheckMetricQuery(layout->GetClusterMetrics(nullptr, 0, &clusterCount));
        PreviewVector<DWRITE_CLUSTER_METRICS> clusters{PreviewAllocator<DWRITE_CLUSTER_METRICS>{_owner}};
        clusters.resize(clusterCount);
        actual = 0;
        if (clusterCount)
            winrt::check_hresult(layout->GetClusterMetrics(clusters.data(), clusterCount, &actual));
        RequireScript(actual == clusterCount);
        _fragments.reserve(clusterCount);
        position = 0;
        for (auto const &cluster : clusters)
        {
            RequireScript(cluster.length && cluster.length <= _length - position);
            FLOAT leading{}, trailing{}, y{};
            DWRITE_HIT_TEST_METRICS raw{}, ignored{};
            winrt::check_hresult(layout->HitTestTextPosition(position, FALSE, &leading, &y, &raw));
            winrt::check_hresult(layout->HitTestTextPosition(position, TRUE, &trailing, &y, &ignored));
            // Validate the physical interval without losing the native directed
            // advance or the independent leading/trailing caret coordinates.
            (void)CanonicalHitMetric(raw);
            RequireScript(raw.textPosition <= position && raw.textPosition <= _length &&
                          raw.length <= _length - raw.textPosition &&
                          std::isfinite(leading) && std::isfinite(trailing));
            auto line = LineAt(position);
            if (!raw.isText)
            {
                winrt::com_ptr<IDWriteInlineObject> object;
                winrt::check_hresult(layout->GetInlineObject(position, object.put(), nullptr));
                if (object)
                {
                    DWRITE_INLINE_OBJECT_METRICS metrics{};
                    winrt::check_hresult(object->GetMetrics(&metrics));
                    RequireScript(std::isfinite(metrics.height) && metrics.height >= 0 &&
                                  std::isfinite(metrics.baseline) && metrics.baseline >= 0 &&
                                  metrics.baseline <= metrics.height);
                    auto dy = Displacement(StyleAt(position));
                    // Replaced objects contribute their full shifted boxes;
                    // non-replaced script text contributes its line-height box.
                    auto leaf = leaves + line;
                    ascent[leaf] = std::max(ascent[leaf], double(metrics.baseline) - dy);
                    descent[leaf] = std::max(descent[leaf], double(metrics.height) - metrics.baseline + dy);
                }
            }
            _fragments.push_back({raw, {}, line, leading, trailing});
            position += cluster.length;
        }
        RequireScript(position == _length);
        for (size_t i = 1; i < leaves; ++i)
        {
            ascent[i * 2] = std::max(ascent[i * 2], ascent[i]);
            ascent[i * 2 + 1] = std::max(ascent[i * 2 + 1], ascent[i]);
            descent[i * 2] = std::max(descent[i * 2], descent[i]);
            descent[i * 2 + 1] = std::max(descent[i * 2 + 1], descent[i]);
        }
        DWRITE_LINE_SPACING_METHOD method{};
        FLOAT spacing{}, baseline{};
        winrt::check_hresult(layout->GetLineSpacing(&method, &spacing, &baseline));
        auto const &parent = Font(0);
        double finalY = text.top, expansion{};
        for (uint32_t i = 0; i < count; ++i)
        {
            auto &line = _lines[i];
            auto const &raw = _metrics[i];
            double a = std::max(double(raw.baseline), ascent[leaves + i]);
            double d = std::max(double(raw.height) - raw.baseline, descent[leaves + i]);
            if (method == DWRITE_LINE_SPACING_METHOD_DEFAULT)
            {
                a = std::max(a, parent.defaultAscent * _parentFontSize);
                d = std::max(d, parent.defaultDescent * _parentFontSize);
            }
            // Content-based object spacing must retain the parent's themed
            // minimum row height. Keep its measured baseline and add tail leading.
            d = std::max(d, double(minimumLineHeight) - a);
            line.finalY = Coordinate(finalY);
            line.finalBaseline = Coordinate(finalY + a);
            line.finalHeight = Coordinate(a + d);
            _metrics[i].baseline = Coordinate(a);
            _metrics[i].height = line.finalHeight;
            expansion += a + d - line.originalHeight;
            finalY += a + d;
        }
        _height = Coordinate(double(text.height) - discardedHeight + expansion);
        _visualTop = _lines.front().finalY;
        _visualBottom = Coordinate(finalY);

        size_t runIndex{};
        for (uint32_t i = 0; i < count; ++i)
        {
            auto const &line = _lines[i];
            auto end = line.start + line.length;
            position = line.start;
            while (position < end)
            {
                while (runIndex < _runs.size() && _runs[runIndex].end <= position)
                    ++runIndex;
                RequireScript(runIndex < _runs.size());
                auto const &run = _runs[runIndex];
                auto finish = std::min(end, run.end);
                auto delta = Coordinate(double(line.finalBaseline) - line.originalBaseline + Displacement(run));
                _drawSpans.push_back({{position, finish - position}, i, delta,
                                      Size(run.size_scale), RunFontFlags(run.flags)});
                position = finish;
            }
        }

        _fragmentLines.resize(count);
        for (uint32_t i = 0; i < count; ++i)
        {
            _fragmentLines[i].top = _lines[i].finalY;
            _fragmentLines[i].bottom = _lines[i].finalY + _lines[i].finalHeight;
        }
        for (auto &fragment : _fragments)
        {
            auto final = Transform(fragment.raw, fragment.line, StyleAt(fragment.raw.textPosition));
            fragment.final = final;
            auto &range = _fragmentLines[fragment.line];
            if (!range.count)
                range.start = static_cast<size_t>(&fragment - _fragments.data());
            ++range.count;
            range.top = std::min(range.top, final.top);
            auto bottom = Coordinate(double(final.top) + final.height);
            range.bottom = std::max(range.bottom, bottom);
            _visualTop = std::min(_visualTop, final.top);
            _visualBottom = std::max(_visualBottom, bottom);
        }
        ResolveRuby();
        // Navigation accumulates FLOAT line heights. Include visual fragments
        // outside those nominal rows on both fresh and cached measurements.
        float nominalY{};
        for (size_t i = 0; i < _lines.size(); ++i)
        {
            auto const &bounds = _fragmentLines[i];
            auto nominalBottom = nominalY + _metrics[i].height;
            _lineOverflowAbove = std::max(_lineOverflowAbove, nominalY - bounds.top);
            _lineOverflowBelow = std::max(_lineOverflowBelow, bounds.bottom - nominalBottom);
            nominalY = nominalBottom;
        }
        if (_lineOverflowAbove > 0)
            _lineOverflowAbove = std::nextafter(_lineOverflowAbove, std::numeric_limits<float>::infinity());
        if (_lineOverflowBelow > 0)
            _lineOverflowBelow = std::nextafter(_lineOverflowBelow, std::numeric_limits<float>::infinity());
        _resolved = true;
    }

    void NativeScriptLayout::RequireResolved() const
    {
        RequireScript(_resolved && _layout && !_lines.empty(), NOTEPADS_MARKDOWN_CORE_INVALID_STATE);
    }

    DWRITE_HIT_TEST_METRICS NativeScriptLayout::CanonicalHitMetric(DWRITE_HIT_TEST_METRICS value)
    {
        RequireScript(std::isfinite(value.left) && std::isfinite(value.top) &&
                      std::isfinite(value.width) && std::isfinite(value.height) && value.height >= 0);
        if (value.width < 0)
        {
            value.left = Coordinate(double(value.left) + value.width);
            value.width = Coordinate(-double(value.width));
        }
        // Individually finite FLOAT fields can still have an overflowing end.
        (void)Coordinate(double(value.left) + value.width);
        (void)Coordinate(double(value.top) + value.height);
        return value;
    }

    DWRITE_HIT_TEST_METRICS NativeScriptLayout::Transform(
        DWRITE_HIT_TEST_METRICS value, uint32_t lineIndex, notepads_markdown_preview_run const &style) const
    {
        value = CanonicalHitMetric(value);
        RequireScript(lineIndex < _lines.size());
        auto const &line = _lines[lineIndex];
        double delta = double(line.finalBaseline) - line.originalBaseline + Displacement(style);
        if (!value.isText && value.length && value.textPosition < _length)
        {
            winrt::com_ptr<IDWriteInlineObject> object;
            winrt::check_hresult(_layout->GetInlineObject(value.textPosition, object.put(), nullptr));
            if (object)
            {
                DWRITE_INLINE_OBJECT_METRICS metrics{};
                winrt::check_hresult(object->GetMetrics(&metrics));
                RequireScript(std::isfinite(metrics.height) && metrics.height >= 0 &&
                              std::isfinite(metrics.baseline) && metrics.baseline >= 0 &&
                              metrics.baseline <= metrics.height);
                value.top = Coordinate(double(line.originalBaseline) - metrics.baseline + delta);
                value.height = metrics.height;
                return CanonicalHitMetric(value);
            }
        }
        // Font sizing has already been applied to the real DWrite layout.
        // Its hit rectangles retain the actual fallback face's ascent/descent,
        // cluster height and bidi fragments. Only the missing baseline shift
        // and expanded-line displacement belong to this transform.
        value.top = Coordinate(double(value.top) + delta);
        return CanonicalHitMetric(value);
    }

    float NativeScriptLayout::BaselineDeltaAt(uint32_t offset, bool trailing) const
    {
        RequireResolved();
        RequireScript(offset <= _length, NOTEPADS_MARKDOWN_CORE_INVALID_RANGE);
        FLOAT x{}, y{};
        DWRITE_HIT_TEST_METRICS raw{};
        winrt::check_hresult(_layout->HitTestTextPosition(offset, trailing, &x, &y, &raw));
        auto const &line = _lines[LineAt(raw.textPosition)];
        return Coordinate(double(line.finalBaseline) - line.originalBaseline + Displacement(StyleAt(raw.textPosition)));
    }

    float NativeScriptLayout::FinalBaselineAt(uint32_t offset, bool trailing) const
    {
        RequireResolved();
        RequireScript(offset <= _length, NOTEPADS_MARKDOWN_CORE_INVALID_RANGE);
        FLOAT x{}, y{};
        DWRITE_HIT_TEST_METRICS raw{};
        winrt::check_hresult(_layout->HitTestTextPosition(offset, trailing, &x, &y, &raw));
        return Coordinate(double(_lines[LineAt(raw.textPosition)].finalBaseline) +
                          Displacement(StyleAt(raw.textPosition)));
    }

    NativeScriptHit NativeScriptLayout::CaretOnLine(uint32_t offset, bool trailing, uint32_t line) const
    {
        RequireScript(line < _lines.size() && offset <= _length, NOTEPADS_MARKDOWN_CORE_INVALID_RANGE);
        FLOAT x{}, y{};
        DWRITE_HIT_TEST_METRICS raw{};
        auto ruby = RubyAt(offset);
        if (!ruby.empty())
        {
            auto const &part = offset < ruby.back().start ? ruby.front() : ruby.back();
            winrt::check_hresult(part.layout->HitTestTextPosition(offset - part.start, trailing, &x, &y, &raw));
            auto box = part.Translate(raw);
            return {box, x + part.x, y + part.y, trailing, false, part.line};
        }
        winrt::check_hresult(_layout->HitTestTextPosition(offset, trailing, &x, &y, &raw));
        if (LineAt(raw.textPosition) != line && offset > _lines[line].start)
        {
            // A leading caret at a shared soft-wrap endpoint belongs to the
            // following row. Its predecessor's real trailing caret stays here.
            winrt::check_hresult(_layout->HitTestTextPosition(offset - 1, TRUE, &x, &y, &raw));
            trailing = true;
        }
        auto transformed = Transform(raw, line, StyleAt(raw.textPosition));
        return {transformed, x, transformed.top, trailing, false, line};
    }

    NativeScriptHit NativeScriptLayout::Caret(uint32_t offset, bool trailing) const
    {
        RequireResolved();
        RequireScript(offset <= _length, NOTEPADS_MARKDOWN_CORE_INVALID_RANGE);
        if (_terminalBreak && offset == _length)
        {
            auto hit = CaretOnLine(offset - _terminalBreak, false, static_cast<uint32_t>(_lines.size() - 1));
            hit.metrics.textPosition = offset;
            hit.metrics.length = 0;
            return hit;
        }
        return CaretOnLine(offset, trailing, LineAt(offset));
    }

    NativeScriptHit NativeScriptLayout::HitTestPoint(float x, float y, std::optional<size_t> originalLine) const
    {
        RequireResolved();
        RequireScript(std::isfinite(x) && std::isfinite(y) &&
                      (!originalLine || *originalLine < _lines.size()), NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT);
        double bestDistance = std::numeric_limits<double>::infinity();
        NativeScriptHit best{};
        bool found{};
        auto firstLine = originalLine ? static_cast<uint32_t>(*originalLine) : 0;
        auto endLine = originalLine ? firstLine + 1 : static_cast<uint32_t>(_lines.size());
        for (uint32_t line = firstLine; line < endLine; ++line)
        {
            auto const &members = _fragmentLines[line];
            auto const &lineRecord = _lines[line];
            bool blank = lineRecord.length == lineRecord.newlineLength;
            double rowDy = std::max({double(members.top) - y, double(y) - members.bottom, 0.0});
            if (rowDy * rowDy > bestDistance)
                continue;
            for (size_t i = members.start; i < members.start + members.count; ++i)
            {
                if (blank)
                    break;
                auto const &fragment = _fragments[i];
                auto ruby = RubyAt(fragment.raw.textPosition);
                if (!ruby.empty())
                {
                    for (auto const &part : ruby)
                    {
                        BOOL trailing{}, inside{};
                        DWRITE_HIT_TEST_METRICS raw{};
                        winrt::check_hresult(part.layout->HitTestPoint(x - part.x, y - part.y, &trailing, &inside, &raw));
                        auto box = part.Translate(raw);
                        const double dx = std::max({double(box.left) - x, double(x) - box.left - box.width, 0.0});
                        const double dy = std::max({double(box.top) - y, double(y) - box.top - box.height, 0.0});
                        const double distance = dx * dx + dy * dy;
                        if (distance >= bestDistance)
                            continue;
                        FLOAT caretX{}, caretY{};
                        if (raw.width < 0)
                        {
                            FLOAT leadingX{}, trailingX{};
                            DWRITE_HIT_TEST_METRICS ignored{};
                            winrt::check_hresult(part.layout->HitTestTextPosition(raw.textPosition, FALSE, &leadingX, &caretY, &ignored));
                            winrt::check_hresult(part.layout->HitTestTextPosition(raw.textPosition, TRUE, &trailingX, &caretY, &ignored));
                            trailing = std::abs(double(x - part.x) - trailingX) < std::abs(double(x - part.x) - leadingX);
                            caretX = trailing ? trailingX : leadingX;
                        }
                        else
                        {
                            winrt::check_hresult(part.layout->HitTestTextPosition(raw.textPosition, trailing, &caretX, &caretY, &raw));
                        }
                        best = {box, caretX + part.x, caretY + part.y, trailing != FALSE,
                                box.width > 0 && distance == 0, line};
                        bestDistance = distance;
                        found = true;
                    }
                    continue;
                }
                auto const &box = fragment.final;
                double dx = std::max({double(box.left) - x, double(x) - box.left - box.width, 0.0});
                double dy = std::max({double(box.top) - y, double(y) - box.top - box.height, 0.0});
                double distance = dx * dx + dy * dy;
                if (distance >= bestDistance)
                    continue;
                BOOL trailing{}, inside{};
                DWRITE_HIT_TEST_METRICS raw{};
                auto epsilon = std::min(0.001f, box.width * 0.25f);
                auto probeX = box.width > 0 ? std::clamp(x, box.left + epsilon, box.left + box.width - epsilon) : box.left;
                auto const &owner = _lines[line];
                winrt::check_hresult(_layout->HitTestPoint(probeX, owner.originalY + owner.originalHeight * 0.5f,
                                                          &trailing, &inside, &raw));
                // DWrite's trailing flag is reversed for a negative directed
                // advance even when its cluster identity matches exactly.
                if (fragment.raw.width < 0 || raw.textPosition != fragment.raw.textPosition || raw.length != fragment.raw.length)
                    trailing = std::abs(double(x) - fragment.trailingX) < std::abs(double(x) - fragment.leadingX);
                bool within = x >= box.left && x <= box.left + box.width && box.width > 0 &&
                              y >= box.top && y <= box.top + box.height && box.height > 0;
                best = {box, trailing ? fragment.trailingX : fragment.leadingX,
                        box.top, trailing != FALSE, within, line};
                bestDistance = distance;
                found = true;
            }
            if (!members.count || blank)
            {
                auto caret = CaretOnLine(_lines[line].start, false, line);
                caret.metrics.left = caret.caretX;
                caret.metrics.width = 0;
                caret.metrics.textPosition = _lines[line].start;
                caret.metrics.length = 0;
                caret.trailing = false;
                double dx = double(caret.caretX) - x;
                double dy = std::max({double(caret.metrics.top) - y,
                                     double(y) - caret.metrics.top - caret.metrics.height, 0.0});
                double distance = dx * dx + dy * dy;
                if (distance < bestDistance)
                {
                    bestDistance = distance;
                    best = caret;
                    found = true;
                }
            }
        }
        RequireScript(found, NOTEPADS_MARKDOWN_CORE_INVALID_STATE);
        return best;
    }

    void NativeScriptLayout::Range(uint32_t start, uint32_t length,
                                   ReadingVector<NativeScriptRangeMetric> &output) const
    {
        RequireResolved();
        RequireScript(start <= _length && length <= _length - start, NOTEPADS_MARKDOWN_CORE_INVALID_RANGE);
        output.clear();
        if (!length)
        {
            auto caret = Caret(start);
            caret.metrics.left = caret.caretX;
            caret.metrics.width = 0;
            caret.metrics.textPosition = start;
            caret.metrics.length = 0;
            output.push_back({caret.metrics, caret.line});
            return;
        }
        ReadingVector<DWRITE_HIT_TEST_METRICS> raw{
            ReadingAllocator<DWRITE_HIT_TEST_METRICS>{output.get_allocator()}};
        ReadingVector<DWRITE_HIT_TEST_METRICS> rubyRanges{raw.get_allocator()};
        uint32_t end = start + length;
        auto first = std::lower_bound(_drawSpans.begin(), _drawSpans.end(), start,
            [](NativeScriptDrawSpan const &span, uint32_t value)
            { return uint64_t(span.range.startPosition) + span.range.length <= value; });
        for (auto span = first; span != _drawSpans.end() && span->range.startPosition < end; ++span)
        {
            auto begin = std::max(start, span->range.startPosition);
            auto finish = std::min(end, span->range.startPosition + span->range.length);
            UINT32 count{};
            CheckMetricQuery(_layout->HitTestTextRange(begin, finish - begin, 0, 0, nullptr, 0, &count));
            raw.resize(count);
            UINT32 actual{};
            if (count)
                winrt::check_hresult(_layout->HitTestTextRange(begin, finish - begin, 0, 0,
                                                              raw.data(), count, &actual));
            RequireScript(actual == count);
            for (auto value : raw)
            {
                value = CanonicalHitMetric(value);
                RequireScript(value.textPosition <= _length && value.length <= _length - value.textPosition);
                auto firstFragment = [&](uint32_t position)
                {
                    return std::lower_bound(_fragments.begin(), _fragments.end(), position,
                        [](Fragment const &fragment, uint32_t point)
                        { return uint64_t(fragment.raw.textPosition) + fragment.raw.length <= point; });
                };
                // DWrite may report a zero-width, zero-length result for the
                // leading part of a composed cluster. Keep that exact identity
                // and horizontal decision, including at a terminal break.
                if (!value.length)
                {
                    auto point = std::clamp(value.textPosition, begin, finish - 1);
                    auto fragment = firstFragment(point);
                    RequireScript(fragment != _fragments.end() && fragment->line == span->line &&
                                  fragment->raw.textPosition < finish &&
                                  uint64_t(fragment->raw.textPosition) + fragment->raw.length > begin);
                    if (RubyAt(fragment->raw.textPosition).empty())
                    {
                        value.top = fragment->final.top;
                        value.height = fragment->final.height;
                        output.push_back({value, span->line});
                    }
                    continue;
                }
                auto logicalStart = std::max(begin, value.textPosition);
                auto logicalEnd = std::min(finish, value.textPosition + value.length);
                auto mergeStart = output.size();
                for (auto fragment = firstFragment(logicalStart);
                     fragment != _fragments.end() && fragment->raw.textPosition < logicalEnd; ++fragment)
                {
                    auto clippedStart = std::max(logicalStart, fragment->raw.textPosition);
                    auto clippedEnd = std::min(logicalEnd, fragment->raw.textPosition + fragment->raw.length);
                    if (clippedStart >= clippedEnd)
                        continue;
                    RequireScript(fragment->line == span->line);
                    if (!RubyAt(fragment->raw.textPosition).empty())
                        continue;
                    auto box = value;
                    box.textPosition = clippedStart;
                    box.length = clippedEnd - clippedStart;
                    // Range preserves native bidi/partial-cluster advances.
                    // Only its uniform line-height box is replaced by the
                    // already resolved font or replaced-object cluster box.
                    box.left = std::max(value.left, fragment->final.left);
                    auto right = std::min(double(value.left) + value.width,
                                          double(fragment->final.left) + fragment->final.width);
                    box.width = Coordinate(std::max(0.0, right - box.left));
                    box.top = fragment->final.top;
                    box.height = fragment->final.height;
                    if (output.size() > mergeStart && box.isText && box.length && box.width > 0)
                    {
                        auto &last = output.back();
                        auto &previous = last.metrics;
                        auto previousRight = double(previous.left) + previous.width;
                        auto boxRight = double(box.left) + box.width;
                        if (last.line == span->line && previous.isText && previous.length && previous.width > 0 &&
                            previous.bidiLevel == box.bidiLevel && previous.isTrimmed == box.isTrimmed &&
                            previous.top == box.top && previous.height == box.height &&
                            uint64_t(previous.textPosition) + previous.length == box.textPosition &&
                            std::max(double(previous.left), double(box.left)) <=
                                std::min(previousRight, boxRight) + 0.001)
                        {
                            auto left = std::min(previous.left, box.left);
                            previous.width = Coordinate(std::max(previousRight, boxRight) - left);
                            previous.left = left;
                            previous.length += box.length;
                            continue;
                        }
                    }
                    // Objects and zero-advance results remain independent:
                    // root background/object handling uses their exact range.
                    output.push_back({box, span->line});
                }
            }
        }
        auto ruby = std::lower_bound(_ruby.begin(), _ruby.end(), start,
            [](NativeRubyPart const &part, uint32_t point) { return part.end <= point; });
        for (; ruby != _ruby.end() && ruby->start < end; ++ruby)
        {
            ruby->Range(start, end, rubyRanges);
            for (auto const &metric : rubyRanges)
                output.push_back({metric, ruby->line, true});
        }
    }
}
