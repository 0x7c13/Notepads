// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Reading/NativeReadingSnapshot.h"
#include "Rendering/NativeMeasurementCache.h"
#include "Control/NativeContinuousFollow.h"
#include "notepads/markdown/preview.h"
#include <dwrite.h>
#include <algorithm>
#include <winrt/Windows.UI.Xaml.Automation.Text.h>
#include <optional>
#include <span>
#include <array>

namespace Notepads::Markdown::Native
{
    struct ReadingSpan
    {
        uint32_t start{};
        uint32_t end{};
        bool Contains(uint32_t first, uint32_t last) const noexcept
        {
            return start <= first && first <= last && last <= end;
        }
    };

    struct ReadingWordSegments
    {
        explicit ReadingWordSegments(std::shared_ptr<ReadingMemory> const &memory)
            : spans(ReadingAllocator<ReadingSpan>{memory})
        {
        }

        ReadingVector<ReadingSpan> spans;
        uint64_t textGeneration{};
        ReadingSpan scope;
        winrt::hstring language;
    };

    struct ReadingSection
    {
        uint32_t heading;
        ReadingSpan text;
        size_t firstHeading;
        size_t endHeading;
        bool ContainsHeading(size_t index) const noexcept
        {
            return firstHeading <= index && index < endHeading;
        }
    };
    struct ReadingVisibility
    {
        ReadingVector<notepads_markdown_preview_disclosure> disclosures;
        ReadingVector<ReadingSpan> hidden;
        explicit ReadingVisibility(std::shared_ptr<ReadingMemory> const &memory)
            : disclosures(ReadingAllocator<notepads_markdown_preview_disclosure>{memory}),
              hidden(ReadingAllocator<ReadingSpan>{memory})
        {
        }
        ReadingSpan const *HiddenAt(uint32_t position) const noexcept;
        ReadingSpan const *FirstHidden(ReadingSpan span) const noexcept;
        notepads_markdown_preview_disclosure const *Disclosure(uint32_t node) const noexcept;
        std::optional<size_t> SummaryIndex(uint32_t summary) const noexcept;
    };
    struct ReadingRect
    {
        float x{};
        float y{};
        float width{};
        float height{};
        uint32_t start{};
        uint32_t end{};
    };
    struct ReadingHit
    {
        uint32_t position{};
        bool inside{};
        float x{};
        float y{};
        ReadingSpan cluster;
        bool trailingCaret{};
    };
    struct ReadingLine
    {
        ReadingSpan text;
        uint32_t block{};
        float x{};
        float localY{};
        float width{};
        float height{};
        double y{};
        bool tableCell{};
        // A current-layout envelope in Main text order. Only viewport follow
        // uses this coordinate; caret/reveal and visual navigation keep y.
        double followY{};
        uint32_t textIndex{NOTEPADS_MARKDOWN_PREVIEW_NONE};
        bool wrappedEnd{};
    };
    struct ReadingBlock
    {
        ReadingSpan text;
        uint32_t node{};
        double y{};
        double height{};
    };
    enum class ReadingTableAliasKind : uint8_t
    {
        ColumnSpan,
        EmptyCaption,
        RowSpan,
    };
    struct ReadingTableCaretAlias
    {
        uint32_t first{};
        uint32_t last{}; // Inclusive boundary points, never visible characters.
        uint32_t target{};
        ReadingTableAliasKind kind{};
        void Validate(MainTextSnapshot const &main) const;
    };
    enum class ReadingObjectKind : uint32_t
    {
        Image = 1,
        Task = 2,
        Math = 3,
        Diagram = 4,
        NoteEmbed = 5
    };
    enum class ReadingActionKind : uint32_t
    {
        None,
        Link,
        Task,
        Disclosure,
        Diagram
    };
    enum class ReadingChildKind
    {
        Object,
        Link,
        Disclosure,
        Table,
        Caption
    };
    struct ReadingActionTarget
    {
        ReadingActionKind kind{ReadingActionKind::None};
        uint32_t index{NOTEPADS_MARKDOWN_PREVIEW_NONE};
        bool operator==(ReadingActionTarget const &) const = default;
    };
    struct ReadingObjectDescriptor
    {
        uint32_t object{};
        uint32_t node{};
        ReadingSpan text;
        ReadingObjectKind kind{};
        uint32_t syntax{};
        bool checked{};
        bool staticHtmlCheckbox{};
        bool imageHasFixedSize{};
        bool imageReadinessAffectsText{};
        ReadingSpan labelText;
        bool hasTaskAction{};
        uint64_t taskActionStart{};
        uint64_t taskActionEnd{};
        std::array<uint8_t, 3> taskExpected{};
        ReadingString alt;
        ReadingString destination;
        ReadingString title;
        ReadingString input;
        ReadingString source;
        ReadingString label;
        ReadingString lightSource;
        ReadingString darkSource;
        uint32_t PictureSource(bool dark) const noexcept
        {
            if (dark)
                return darkSource.empty() ? 0u : 2u;
            return lightSource.empty() ? 0u : 1u;
        }
        std::wstring_view ImageDestination(uint32_t sourceIndex) const noexcept
        {
            if (sourceIndex == 1)
                return lightSource;
            if (sourceIndex == 2)
                return darkSource;
            return destination;
        }
        std::wstring_view ExportText() const noexcept
        {
            return kind == ReadingObjectKind::Math || kind == ReadingObjectKind::Diagram || kind == ReadingObjectKind::NoteEmbed
                       ? std::wstring_view{source.data(), source.size()}
                       : std::wstring_view{alt.data(), alt.size()};
        }
        explicit ReadingObjectDescriptor(std::shared_ptr<ReadingMemory> const &memory)
            : alt(ReadingAllocator<wchar_t>{memory}), destination(ReadingAllocator<wchar_t>{memory}),
              title(ReadingAllocator<wchar_t>{memory}), input(ReadingAllocator<wchar_t>{memory}),
              source(ReadingAllocator<wchar_t>{memory}), label(ReadingAllocator<wchar_t>{memory}),
              lightSource(ReadingAllocator<wchar_t>{memory}), darkSource(ReadingAllocator<wchar_t>{memory})
        {
        }
    };
    struct ReadingLinkDescriptor
    {
        uint32_t node{};
        ReadingSpan text;
        std::optional<uint32_t> localPosition;
        uint32_t localHeadingNode{};
        bool wiki{};
        ReadingString destination;
        ReadingString title;
        explicit ReadingLinkDescriptor(std::shared_ptr<ReadingMemory> const &memory)
            : destination(ReadingAllocator<wchar_t>{memory}), title(ReadingAllocator<wchar_t>{memory})
        {
        }
    };
    struct ReadingDisclosureDescriptor
    {
        uint32_t node{};
        uint32_t summary{};
        ReadingSpan text;
    };
    struct ReadingTableDescriptor
    {
        uint32_t node{};
        ReadingSpan text;
        ReadingSpan caption;
    };
    class ReadingOutline;
    struct SemanticDescriptors
    {
        std::shared_ptr<ReadingMemory> memory;
        uint64_t textGeneration{};
        ReadingVector<ReadingObjectDescriptor> objects;
        ReadingVector<ReadingLinkDescriptor> links;
        ReadingVector<ReadingDisclosureDescriptor> disclosures;
        ReadingVector<ReadingTableDescriptor> tables;
        std::shared_ptr<ReadingOutline const> outline;
        explicit SemanticDescriptors(std::shared_ptr<ReadingMemory> owner, uint64_t generation);
        void Finish(MainTextSnapshot const &main);
        ReadingString Copy(MainTextSnapshot const &main, uint32_t start, uint32_t end) const;
        std::optional<size_t> LinkAt(uint32_t position) const;
        ReadingSpan Child(ReadingChildKind kind, size_t index) const;
        ReadingTableDescriptor const *TableContaining(ReadingSpan span) const noexcept;
        template <typename Visitor> void VisitChildren(Visitor &&visitor, uint32_t from = 0) const
        {
            auto first = [from](auto const &items)
            {
                return static_cast<size_t>(std::lower_bound(items.begin(), items.end(), from,
                    [](auto const &item, auto value) { return item.text.start < value; }) - items.begin());
            };
            auto object = first(objects);
            auto link = first(links);
            auto disclosure = first(disclosures);
            while (object < objects.size() || link < links.size() || disclosure < disclosures.size())
            {
                auto objectStart = object < objects.size() ? objects[object].text.start : UINT32_MAX;
                auto linkStart = link < links.size() ? links[link].text.start : UINT32_MAX;
                auto disclosureStart = disclosure < disclosures.size() ? disclosures[disclosure].text.start : UINT32_MAX;
                if (disclosure < disclosures.size() && disclosureStart <= std::min(objectStart, linkStart))
                {
                    if (!visitor(ReadingChildKind::Disclosure, disclosure++))
                        return;
                }
                else if (link < links.size() && linkStart <= objectStart)
                {
                    if (!visitor(ReadingChildKind::Link, link++))
                        return;
                }
                else if (!visitor(ReadingChildKind::Object, object++))
                    return;
            }
        }
        template <typename Visitor> void VisitRootChildren(Visitor &&visitor) const
        {
            size_t table{};
            bool proceed = true;
            VisitChildren([&](ReadingChildKind kind, size_t index)
            {
                auto span = Child(kind, index);
                while (table < tables.size() && tables[table].text.start <= span.start)
                {
                    proceed = visitor(ReadingChildKind::Table, table++);
                    if (!proceed)
                        return false;
                }
                if (!TableContaining(span))
                    proceed = visitor(kind, index);
                return proceed;
            });
            while (proceed && table < tables.size())
                proceed = visitor(ReadingChildKind::Table, table++);
        }

        template <typename Visitor> void VisitTableChildren(size_t index, bool captionOnly, Visitor &&visitor) const
        {
            auto const &table = tables.at(index);
            auto scope = captionOnly ? table.caption : table.text;
            if (!captionOnly && !visitor(ReadingChildKind::Caption, index))
                return;
            VisitChildren([&](ReadingChildKind kind, size_t child)
            {
                auto span = Child(kind, child);
                if (span.start >= scope.end)
                    return false;
                if (scope.Contains(span.start, span.end) &&
                    (captionOnly || !table.caption.Contains(span.start, span.end)))
                    return visitor(kind, child);
                return true;
            }, scope.start);
        }
    };

    class ReadingNavigationIndex;
    class ReadingMetricToken
    {
      public:
        explicit operator bool() const noexcept { return _owner != nullptr; }

      private:
        friend class ReadingNavigationIndex;
        ReadingNavigationIndex const *_owner{};
        size_t _firstLine{}, _lineCount{};
        uint32_t _start{}, _end{};
    };

    class ReadingNavigationIndex
    {
      public:
        ReadingNavigationIndex(std::shared_ptr<ReadingMemory> memory, notepads_markdown_preview_key const &key, uint32_t length,
                               std::shared_ptr<ReadingVisibility const> visibility = {}, std::optional<ReadingSpan> scope = {});
        void AddText(uint32_t block, uint32_t start, uint32_t end, IDWriteTextLayout *layout, float x, float y,
                     uint32_t prefixStart = NOTEPADS_MARKDOWN_PREVIEW_NONE, bool tableCell = false, uint32_t terminalBreak = 0,
                     std::span<DWRITE_LINE_METRICS const> adjustedLines = {},
                     std::span<uint32_t const> inlineCaretStops = {},
                     uint32_t textIndex = NOTEPADS_MARKDOWN_PREVIEW_NONE,
                     float overflowAbove = 0, float overflowBelow = 0);
        ReadingMetricToken AppendTextMetrics(uint32_t start, uint32_t end, TextMetricView const &metrics,
                                            uint32_t terminalBreak = 0);
        void BindTextMetrics(ReadingMetricToken const &token, uint32_t block, uint32_t start, uint32_t end,
                             float x, float y, uint32_t prefixStart = NOTEPADS_MARKDOWN_PREVIEW_NONE,
                             bool tableCell = false, uint32_t textIndex = NOTEPADS_MARKDOWN_PREVIEW_NONE);
        void AddObjectLine(uint32_t block, uint32_t start, uint32_t end, float x, float y, float height);
        void AddParagraph(uint32_t start, uint32_t end);
        void AddBlock(notepads_markdown_preview_block const &block);
        void AddTableCaretAlias(ReadingTableCaretAlias alias);
        void Finish(MainTextSnapshot const &main, ReadingWordSegments *wordSegments = nullptr);
        uint32_t CanonicalCaret(uint32_t position) const;
        uint32_t SnapTextBoundary(uint32_t position, bool forward) const;
        uint32_t Snap(uint32_t position, bool forward) const;
        void SnapSelection(uint32_t &anchor, uint32_t &active) const;
        uint32_t Move(uint32_t position, winrt::Windows::UI::Xaml::Automation::Text::TextUnit unit, int32_t count,
                      int32_t &moved) const;
        ReadingSpan Unit(uint32_t position, winrt::Windows::UI::Xaml::Automation::Text::TextUnit unit) const;
        size_t LineAt(uint32_t position, bool trailing = false) const;
        size_t VisualLine(size_t current, bool forward, double pageDistance, float preferredX) const;
        std::span<uint32_t const> VisibleLines(double top, double bottom) const noexcept;
        // Conservative visual bounds retain the nominal order used for caret navigation.
        double VisibleLineTop(ReadingLine const &line) const noexcept { return line.y - _visualOverflowAbove; }
        double VisibleLineBottom(ReadingLine const &line) const noexcept { return line.y + line.height + _visualOverflowBelow; }
        ReadingLine const &Line(size_t index) const;
        size_t LineCount() const noexcept
        {
            return _lines.size();
        }
        double RevealY(uint32_t position) const;
        double RevealBottomY(ReadingSpan span) const;
        double FollowY(uint32_t position) const;
        double ContinuousFollowY(uint32_t position) const;
        FollowCoordinates::Span ContinuousFollowSpan(double y) const;
        double RevealBlockY(uint32_t node) const;
        notepads_markdown_preview_key const &Key() const noexcept
        {
            return _key;
        }
        std::shared_ptr<ReadingMemory> Memory() const noexcept
        {
            return _memory;
        }

      private:
        std::shared_ptr<ReadingMemory> _memory;
        notepads_markdown_preview_key _key;
        uint32_t _length;
        ReadingSpan _scope;
        std::shared_ptr<ReadingVisibility const> _visibility;
        ReadingVector<uint32_t> _carets;
        ReadingVector<uint32_t> _words;
        ReadingVector<uint32_t> _lineStops;
        ReadingVector<uint32_t> _paragraphStops;
        std::array<uint32_t, 2> _document;
        ReadingVector<ReadingSpan> _coverage;
        ReadingVector<ReadingSpan> _paragraphs;
        ReadingVector<ReadingLine> _lines;
        ReadingVector<uint32_t> _visualLines;
        // Prefix maximum of line bottoms in visual order, including overlapping table rows.
        ReadingVector<double> _visualLineBottoms;
        float _visualOverflowAbove{}, _visualOverflowBelow{};
        ReadingVector<ReadingBlock> _blocks;
        ReadingVector<ReadingTableCaretAlias> _tableAliases;
        std::span<uint32_t const> Boundaries(winrt::Windows::UI::Xaml::Automation::Text::TextUnit unit) const;
    };
}
