// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Rendering/NativeDocumentRenderer.h"
#include <optional>
#include <span>
#include <string_view>

namespace Notepads::Markdown::Native
{
    struct NativeScriptLine
    {
        uint32_t start{}, length{}, newlineLength{};
        // Baselines are absolute text-local coordinates, not offsets from Y.
        float originalY{}, originalBaseline{}, originalHeight{};
        float finalY{}, finalBaseline{}, finalHeight{};
    };

    struct NativeScriptDrawSpan
    {
        DWRITE_TEXT_RANGE range{};
        uint32_t line{};
        float delta{}, fontSize{};
        uint32_t fontFlags{};
    };

    struct NativeScriptRangeMetric
    {
        DWRITE_HIT_TEST_METRICS metrics{};
        uint32_t line{};
        bool ruby{};
    };

    struct NativeRubyPart
    {
        uint32_t start{}, end{}, line{};
        float x{}, y{}, width{}, height{}, baseline{};
        bool linked{}, authoredUnderline{};
        winrt::com_ptr<IDWriteTextLayout> layout;

        DWRITE_HIT_TEST_METRICS Translate(DWRITE_HIT_TEST_METRICS value) const;
        void Range(uint32_t start, uint32_t end, ReadingVector<DWRITE_HIT_TEST_METRICS> &output) const;
    };

    struct NativeScriptHit
    {
        DWRITE_HIT_TEST_METRICS metrics{};
        float caretX{}, caretY{};
        bool trailing{}, inside{};
        uint32_t line{};
    };

    // Built on the active layout lease, immutable after Resolve. All query-time
    // storage belongs to ReadingMemory, so retained cancelled frames remain usable.
    class NativeScriptLayout final
    {
        struct ConstructionKey
        {
        };

      public:
        static std::shared_ptr<NativeScriptLayout> Create(
            notepads_markdown_preview_layout *owner, IDWriteFactory *factory, float parentFontSize,
            std::wstring_view family, std::wstring_view codeFamily, DWRITE_FONT_WEIGHT baseWeight,
            std::span<notepads_markdown_preview_run const> runs, std::span<notepads_markdown_preview_script_scope const> scopes,
            uint32_t utf16Length);
        NativeScriptLayout(ConstructionKey, notepads_markdown_preview_layout *owner, IDWriteFactory *factory,
                           float parentFontSize, std::wstring_view family, std::wstring_view codeFamily,
                           DWRITE_FONT_WEIGHT baseWeight, std::span<notepads_markdown_preview_run const> runs,
                           std::span<notepads_markdown_preview_script_scope const> scopes, uint32_t utf16Length);

        // DWrite can expose a negative directed advance for leading combining
        // marks. Preserve its logical identity and return the physical interval.
        static DWRITE_HIT_TEST_METRICS CanonicalHitMetric(DWRITE_HIT_TEST_METRICS value);
        float FontSizeAt(uint32_t offset) const;
        uint32_t FontFlagsAt(uint32_t offset) const;
        void ApplyFontSizes(IDWriteTextLayout *layout) const;
        DWRITE_INLINE_OBJECT_METRICS AddRuby(IDWriteFactory *factory, std::wstring_view text,
                                            notepads_markdown_preview_ruby_unit const &unit, DWRITE_FONT_WEIGHT weight);
        std::span<NativeRubyPart const> RubyParts() const noexcept { return _ruby; }
        std::span<uint32_t const> RubyCaretStops() const noexcept { return _rubyCaretStops; }
        // Exactly once, after every inline-object replacement and resulting reflow.
        void Resolve(IDWriteTextLayout *layout, float minimumLineHeight = 0);
        bool Resolved() const noexcept { return _resolved; }
        float FinalHeight() const noexcept { return _height; }
        float VisualTop() const noexcept { return _visualTop; }
        float VisualBottom() const noexcept { return _visualBottom; }
        float LineOverflowAbove() const noexcept { return _lineOverflowAbove; }
        float LineOverflowBelow() const noexcept { return _lineOverflowBelow; }
        std::span<NativeScriptLine const> Lines() const noexcept { return _lines; }
        std::span<DWRITE_LINE_METRICS const> FinalLineMetrics() const noexcept { return _metrics; }
        // A logical, nonoverlapping partition. At most two extra effect slots are
        // required to split it at a contiguous selection's endpoints.
        std::span<NativeScriptDrawSpan const> DrawSpans() const noexcept { return _drawSpans; }
        float BaselineDeltaAt(uint32_t offset, bool trailing = false) const;
        float FinalBaselineAt(uint32_t offset, bool trailing = false) const;
        NativeScriptHit Caret(uint32_t offset, bool trailing = false) const;
        NativeScriptHit HitTestPoint(float x, float y, std::optional<size_t> originalLine = {}) const;
        // Clears output; records retain the original owner line and DWrite bidi/
        // cluster identity. The caller supplies a ReadingMemory-funded allocator.
        void Range(uint32_t start, uint32_t length, ReadingVector<NativeScriptRangeMetric> &output) const;

      private:
        struct FontMetrics
        {
            double ascent{}, descent{}, defaultAscent{}, defaultDescent{};
            bool ready{};
        };
        struct Fragment
        {
            // Native signed raw retains caret provenance; final is canonical.
            DWRITE_HIT_TEST_METRICS raw{}, final{};
            uint32_t line{};
            float leadingX{}, trailingX{};
        };
        struct FragmentLine
        {
            size_t start{}, count{};
            float top{}, bottom{};
        };
        notepads_markdown_preview_run const &StyleAt(uint32_t offset) const;
        float Size(double scale) const;
        double Displacement(notepads_markdown_preview_run const &run) const;
        FontMetrics const &Font(uint32_t flags) const;
        uint32_t LineAt(uint32_t offset) const;
        uint32_t EmptyScopeLine(uint32_t offset) const;
        DWRITE_HIT_TEST_METRICS Transform(DWRITE_HIT_TEST_METRICS value, uint32_t line,
                                         notepads_markdown_preview_run const &style) const;
        NativeScriptHit CaretOnLine(uint32_t offset, bool trailing, uint32_t line) const;
        std::span<NativeRubyPart const> RubyAt(uint32_t offset) const;
        void ResolveRuby();
        void RequireResolved() const;

        notepads_markdown_preview_layout *_owner{};
        float _parentFontSize{}, _height{}, _visualTop{}, _visualBottom{};
        float _lineOverflowAbove{}, _lineOverflowBelow{};
        uint32_t _length{};
        uint32_t _terminalBreak{};
        bool _resolved{};
        notepads_markdown_preview_run _plain{};
        PreviewString _family, _codeFamily;
        PreviewVector<notepads_markdown_preview_run> _runs;
        PreviewVector<notepads_markdown_preview_script_scope> _scopes;
        PreviewVector<NativeScriptLine> _lines;
        PreviewVector<DWRITE_LINE_METRICS> _metrics;
        PreviewVector<NativeScriptDrawSpan> _drawSpans;
        PreviewVector<Fragment> _fragments;
        PreviewVector<FragmentLine> _fragmentLines;
        PreviewVector<NativeRubyPart> _ruby;
        PreviewVector<uint32_t> _rubyCaretStops;
        std::array<FontMetrics, 12> _fonts{};
        winrt::com_ptr<IDWriteTextLayout> _layout;
    };
}
