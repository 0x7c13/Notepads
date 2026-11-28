// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include <winrt/Windows.Foundation.Collections.h>
#include "Reading/NativeReadingNavigation.h"
#include "Reading/NativeReadingOutline.h"
#include <winrt/Windows.Data.Text.h>
#include <winrt/Windows.System.UserProfile.h>
#include <algorithm>
#include <limits>

namespace Notepads::Markdown::Native
{
    namespace
    {
        using TextUnit = winrt::Windows::UI::Xaml::Automation::Text::TextUnit;
        void RequireReading(bool valid)
        {
            if (!valid)
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
        }
        template <typename T> void SortUnique(T &items)
        {
            std::sort(items.begin(), items.end());
            items.erase(std::unique(items.begin(), items.end()), items.end());
        }
    }
    SemanticDescriptors::SemanticDescriptors(std::shared_ptr<ReadingMemory> owner, uint64_t generation)
        : memory(std::move(owner)), textGeneration(generation),
          objects(ReadingAllocator<ReadingObjectDescriptor>{memory}),
          links(ReadingAllocator<ReadingLinkDescriptor>{memory}),
          disclosures(ReadingAllocator<ReadingDisclosureDescriptor>{memory}),
          tables(ReadingAllocator<ReadingTableDescriptor>{memory})
    {
    }

    ReadingSpan SemanticDescriptors::Child(ReadingChildKind kind, size_t index) const
    {
        if (kind == ReadingChildKind::Object && index < objects.size())
            return objects[index].text;
        if (kind == ReadingChildKind::Link && index < links.size())
            return links[index].text;
        if (kind == ReadingChildKind::Disclosure && index < disclosures.size())
            return disclosures[index].text;
        if (index < tables.size())
        {
            if (kind == ReadingChildKind::Table)
                return tables[index].text;
            if (kind == ReadingChildKind::Caption)
                return tables[index].caption;
        }
        throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_RANGE};
    }

    ReadingTableDescriptor const *SemanticDescriptors::TableContaining(ReadingSpan span) const noexcept
    {
        auto at = std::upper_bound(tables.begin(), tables.end(), span.start,
            [](uint32_t value, ReadingTableDescriptor const &table) { return value < table.text.start; });
        if (at != tables.begin())
        {
            --at;
            if (span.start < at->text.end && at->text.Contains(span.start, span.end))
                return &*at;
        }
        return nullptr;
    }

    void SemanticDescriptors::Finish(MainTextSnapshot const &main)
    {
        RequireReading(main.TextGeneration() == textGeneration);
        std::sort(tables.begin(), tables.end(),
            [](auto const &a, auto const &b) { return a.text.start < b.text.start; });
        uint32_t previousTableEnd{};
        for (auto const &table : tables)
        {
            RequireReading(table.node && table.text.start >= previousTableEnd &&
                table.caption.start == table.text.start && table.caption.end > table.caption.start &&
                table.caption.end <= table.text.end);
            main.ValidateRange(table.text.start, table.text.end - table.text.start);
            main.ValidateRange(table.caption.start, table.caption.end - table.caption.start);
            previousTableEnd = table.text.end;
        }
        std::sort(objects.begin(), objects.end(),
                  [](auto const &a, auto const &b) { return a.text.start < b.text.start; });
        uint32_t priorEnd{};
        for (auto const &object : objects)
        {
            main.ValidateRange(object.text.start, object.text.end - object.text.start);
            RequireReading(object.text.start >= priorEnd && object.text.end == object.text.start + 1 &&
                           main.Text()[object.text.start] == L'\xfffc');
            if (object.staticHtmlCheckbox)
            {
                RequireReading(object.kind == ReadingObjectKind::Task && !object.hasTaskAction &&
                               object.taskActionStart == 0 && object.taskActionEnd == 0 &&
                               object.taskExpected == std::array<uint8_t, 3>{} &&
                               object.labelText.start >= object.text.end &&
                               object.labelText.end >= object.labelText.start);
                main.ValidateRange(object.labelText.start, object.labelText.end - object.labelText.start);
            }
            if (object.hasTaskAction)
            {
                auto const &expected = object.taskExpected;
                RequireReading(object.kind == ReadingObjectKind::Task && object.taskActionStart <= UINT64_MAX - 3 &&
                               object.taskActionEnd == object.taskActionStart + 3 && expected[0] == '[' &&
                               expected[2] == ']' && (expected[1] == ' ' || expected[1] == 'x' || expected[1] == 'X') &&
                               object.checked == (expected[1] != ' '));
            }
            priorEnd = object.text.end;
        }
        // Bind labels only after every object is sorted and fully validated. Native copy
        // expands safe inline image alt text instead of exposing U+FFFC in the
        // checkbox's accessible name. Both temporary and retained text are funded.
        for (auto &object : objects)
            if (object.staticHtmlCheckbox)
                object.label = Copy(main, object.labelText.start, object.labelText.end);
        std::sort(links.begin(), links.end(), [](auto const &a, auto const &b)
                  { return a.node < b.node || (a.node == b.node && a.text.start < b.text.start); });
        size_t output{};
        for (size_t i = 0; i < links.size(); ++i)
        {
            auto &link = links[i];
            main.ValidateRange(link.text.start, link.text.end - link.text.start);
            if (link.localPosition)
            {
                main.ValidateRange(*link.localPosition, 0);
                RequireReading(!link.localHeadingNode && link.destination.empty() && !link.title.empty());
            }
            if (link.localHeadingNode)
                RequireReading(outline && outline->FindNode(link.localHeadingNode) &&
                               link.destination.empty() && !link.title.empty());
            if (link.wiki)
                RequireReading(!link.localPosition && !link.localHeadingNode &&
                               !link.destination.empty() && link.title.empty());
            if (output && links[output - 1].node == link.node)
            {
                auto &old = links[output - 1];
                RequireReading(old.destination == link.destination && old.title == link.title &&
                               old.localPosition == link.localPosition &&
                               old.localHeadingNode == link.localHeadingNode &&
                               old.wiki == link.wiki &&
                               link.text.start <= old.text.end);
                old.text.end = std::max(old.text.end, link.text.end);
            }
            else
            {
                if (output != i)
                    links[output] = std::move(link);
                ++output;
            }
        }
        links.erase(links.begin() + output, links.end());
        std::sort(links.begin(), links.end(), [](auto const &a, auto const &b) { return a.text.start < b.text.start; });
    }
    ReadingString SemanticDescriptors::Copy(MainTextSnapshot const &main, uint32_t start, uint32_t end) const
    {
        RequireReading(main.TextGeneration() == textGeneration && end >= start);
        main.ValidateRange(start, end - start);
        // Finish establishes sorted, disjoint one-unit object spans. Exclude
        // objects before start and at/after end by binary search, so binding
        // each short HTML label does not rescan every object in the document.
        auto before = [](auto const &object, uint32_t position) { return object.text.start < position; };
        auto first = std::lower_bound(objects.begin(), objects.end(), start, before);
        auto last = std::lower_bound(first, objects.end(), end, before);
        size_t length = end - start;
        for (auto at = first; at != last; ++at)
        {
            auto const &object = *at;
            if (object.text.end > end)
                continue;
            auto replacement = object.kind == ReadingObjectKind::Task ? size_t{3} : object.ExportText().size();
            if (replacement > SIZE_MAX - (length - 1))
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_ARITHMETIC_OVERFLOW};
            length = length - 1 + replacement;
        }
        ReadingString result{ReadingAllocator<wchar_t>{memory}};
        result.reserve(length);
        auto text = main.Text();
        uint32_t cursor = start;
        for (auto at = first; at != last; ++at)
        {
            auto const &object = *at;
            if (object.text.end > end)
                continue;
            result.append(text.data() + cursor, object.text.start - cursor);
            if (object.kind == ReadingObjectKind::Task)
                result.append(object.checked ? L"[x]" : L"[ ]");
            else
                result.append(object.ExportText());
            cursor = object.text.end;
        }
        result.append(text.data() + cursor, end - cursor);
        return result;
    }
    std::optional<size_t> SemanticDescriptors::LinkAt(uint32_t position) const
    {
        auto it = std::upper_bound(links.begin(), links.end(), position,
                                   [](uint32_t value, auto const &link) { return value < link.text.start; });
        if (it == links.begin())
            return {};
        --it;
        if (position >= it->text.start && position < it->text.end)
            return static_cast<size_t>(it - links.begin());
        return {};
    }

    ReadingSpan const *ReadingVisibility::HiddenAt(uint32_t position) const noexcept
    {
        auto at = std::lower_bound(hidden.begin(), hidden.end(), position,
                                  [](auto range, uint32_t point) { return range.end <= point; });
        return at != hidden.end() && at->start <= position ? &*at : nullptr;
    }

    ReadingSpan const *ReadingVisibility::FirstHidden(ReadingSpan span) const noexcept
    {
        auto at = std::lower_bound(hidden.begin(), hidden.end(), span.start,
                                  [](auto range, uint32_t point) { return range.end <= point; });
        return span.start < span.end && at != hidden.end() && at->start < span.end ? &*at : nullptr;
    }

    notepads_markdown_preview_disclosure const *ReadingVisibility::Disclosure(uint32_t node) const noexcept
    {
        auto at = std::lower_bound(disclosures.begin(), disclosures.end(), node,
                                  [](auto const &row, uint32_t id) { return row.node < id; });
        return at != disclosures.end() && at->node == node ? &*at : nullptr;
    }

    std::optional<size_t> ReadingVisibility::SummaryIndex(uint32_t summary) const noexcept
    {
        auto at = std::lower_bound(disclosures.begin(), disclosures.end(), summary,
                                  [](auto const &row, uint32_t id) { return row.summary < id; });
        if (at == disclosures.end() || at->summary != summary)
            return {};
        return static_cast<size_t>(at - disclosures.begin());
    }

    ReadingNavigationIndex::ReadingNavigationIndex(std::shared_ptr<ReadingMemory> memory, notepads_markdown_preview_key const &key,
                                                   uint32_t length, std::shared_ptr<ReadingVisibility const> visibility,
                                                   std::optional<ReadingSpan> scope)
        : _memory(std::move(memory)), _key(key), _length(length), _scope(scope.value_or(ReadingSpan{0, length})),
          _visibility(std::move(visibility)),
          _carets(ReadingAllocator<uint32_t>{_memory}),
          _words(ReadingAllocator<uint32_t>{_memory}), _lineStops(ReadingAllocator<uint32_t>{_memory}),
          _paragraphStops(ReadingAllocator<uint32_t>{_memory}), _document{0, length},
          _coverage(ReadingAllocator<ReadingSpan>{_memory}), _paragraphs(ReadingAllocator<ReadingSpan>{_memory}),
          _lines(ReadingAllocator<ReadingLine>{_memory}), _visualLines(ReadingAllocator<uint32_t>{_memory}),
          _visualLineBottoms(ReadingAllocator<double>{_memory}),
          _blocks(ReadingAllocator<ReadingBlock>{_memory}),
          _tableAliases(ReadingAllocator<ReadingTableCaretAlias>{_memory})
    {
        RequireReading(_scope.start <= _scope.end && _scope.end <= length);
    }

    void ReadingNavigationIndex::AddText(uint32_t block, uint32_t start, uint32_t end, IDWriteTextLayout *layout,
                                         float x, float y, uint32_t prefixStart, bool tableCell,
                                         uint32_t terminalBreak, std::span<DWRITE_LINE_METRICS const> adjustedLines,
                                         std::span<uint32_t const> inlineCaretStops, uint32_t textIndex,
                                         float overflowAbove, float overflowBelow)
    {
        RequireReading(std::isfinite(overflowAbove) && overflowAbove >= 0 &&
                       std::isfinite(overflowBelow) && overflowBelow >= 0);
        _visualOverflowAbove = std::max(_visualOverflowAbove, overflowAbove);
        _visualOverflowBelow = std::max(_visualOverflowBelow, overflowBelow);
        RequireReading(start <= end && end <= _length && (prefixStart == NOTEPADS_MARKDOWN_PREVIEW_NONE || prefixStart <= start));
        _coverage.push_back({start, end});
        _carets.push_back(start);
        UINT32 count{};
        auto hr = layout->GetClusterMetrics(nullptr, 0, &count);
        if (FAILED(hr) && hr != HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER))
            winrt::check_hresult(hr);
        ReadingVector<DWRITE_CLUSTER_METRICS> clusters{ReadingAllocator<DWRITE_CLUSTER_METRICS>{_memory}};
        clusters.resize(count);
        if (count)
            winrt::check_hresult(layout->GetClusterMetrics(clusters.data(), count, &count));
        clusters.resize(count);
        uint32_t position = start;
        for (auto const &cluster : clusters)
        {
            RequireReading(cluster.length <= end - position);
            position += cluster.length;
            _carets.push_back(position);
        }
        RequireReading(position == end);
        for (auto stop : inlineCaretStops)
        {
            RequireReading(stop <= end - start);
            _carets.push_back(start + stop);
        }
        count = 0;
        hr = layout->GetLineMetrics(nullptr, 0, &count);
        if (FAILED(hr) && hr != HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER))
            winrt::check_hresult(hr);
        ReadingVector<DWRITE_LINE_METRICS> lines{ReadingAllocator<DWRITE_LINE_METRICS>{_memory}};
        lines.resize(count);
        if (count)
            winrt::check_hresult(layout->GetLineMetrics(lines.data(), count, &count));
        lines.resize(count);
        if (!adjustedLines.empty())
        {
            RequireReading(adjustedLines.size() == lines.size() ||
                           (adjustedLines.size() + 1 == lines.size() && !lines.back().length &&
                            lines[lines.size() - 2].newlineLength));
            lines.resize(adjustedLines.size());
            for (size_t i = 0; i < lines.size(); ++i)
            {
                auto const &replacement = adjustedLines[i];
                RequireReading(replacement.length == lines[i].length &&
                               replacement.newlineLength == lines[i].newlineLength &&
                               std::isfinite(replacement.height) && replacement.height > 0);
                lines[i].height = replacement.height;
            }
        }
        position = start;
        float localY = y;
        for (auto const &line : lines)
        {
            RequireReading(line.length <= end - position && std::isfinite(line.height));
            if (terminalBreak && &line == &lines.back() && lines.size() > 1 && !line.length)
                break;
            RequireReading(line.newlineLength <= line.length);
            auto lineStart = position == start && prefixStart != NOTEPADS_MARKDOWN_PREVIEW_NONE ? prefixStart : position;
            _lines.push_back({{lineStart, position + line.length - line.newlineLength},
                              block,
                              x,
                              localY,
                              layout->GetMaxWidth(),
                              line.height,
                              0,
                              tableCell});
            _lines.back().textIndex = textIndex;
            _lines.back().wrappedEnd = !line.newlineLength && &line != &lines.back();
            position += line.length;
            localY += line.height;
        }
        RequireReading(position == end);
    }
    ReadingMetricToken ReadingNavigationIndex::AppendTextMetrics(uint32_t start, uint32_t end,
                                                                 TextMetricView const &metrics,
                                                                 uint32_t terminalBreak)
    {
        RequireReading(start <= end && end <= _length && metrics.Valid(end - start));
        _visualOverflowAbove = std::max(_visualOverflowAbove, metrics.OverflowAbove());
        _visualOverflowBelow = std::max(_visualOverflowBelow, metrics.OverflowBelow());
        ReadingMetricToken token;
        token._owner = this;
        token._firstLine = _lines.size();
        token._lineCount = metrics.LineCount();
        if (terminalBreak && token._lineCount > 1 && !metrics.Line(metrics.LineCount() - 1).length)
            --token._lineCount;
        token._start = start;
        token._end = end;
        _coverage.push_back({start, end});
        _carets.push_back(start);
        uint32_t position = start;
        for (uint32_t i = 0; i < metrics.ClusterCount(); ++i)
        {
            position += metrics.Cluster(i);
            _carets.push_back(position);
        }
        position = start;
        float y{};
        for (uint32_t i = 0; i < token._lineCount; ++i)
        {
            auto line = metrics.Line(i);
            _lines.push_back({{position, position + line.length - line.newlineLength},
                              NOTEPADS_MARKDOWN_PREVIEW_NONE,
                              0,
                              y,
                              metrics.Width(),
                              line.height,
                              0,
                              false});
            _lines.back().wrappedEnd = !line.newlineLength && i + 1 < token._lineCount;
            position += line.length;
            y += line.height;
        }
        return token;
    }
    void ReadingNavigationIndex::BindTextMetrics(ReadingMetricToken const &token, uint32_t block, uint32_t start,
                                                 uint32_t end, float x, float y, uint32_t prefixStart, bool tableCell,
                                                 uint32_t textIndex)
    {
        RequireReading(token._owner == this && token._start == start && token._end == end && block != NOTEPADS_MARKDOWN_PREVIEW_NONE &&
                       token._lineCount != 0 && token._firstLine <= _lines.size() &&
                       token._lineCount <= _lines.size() - token._firstLine &&
                       (prefixStart == NOTEPADS_MARKDOWN_PREVIEW_NONE || prefixStart <= start) && std::isfinite(x) &&
                       std::isfinite(y));
        for (size_t i = 0; i < token._lineCount; ++i)
            RequireReading(_lines[token._firstLine + i].block == NOTEPADS_MARKDOWN_PREVIEW_NONE);
        for (size_t i = 0; i < token._lineCount; ++i)
        {
            auto &line = _lines[token._firstLine + i];
            line.block = block;
            line.x = x;
            line.localY = y;
            y += line.height;
            line.tableCell = tableCell;
            line.textIndex = textIndex;
            if (i == 0 && prefixStart != NOTEPADS_MARKDOWN_PREVIEW_NONE)
                line.text.start = prefixStart;
        }
    }
    void ReadingNavigationIndex::AddObjectLine(uint32_t block, uint32_t start, uint32_t end, float x, float y,
                                               float height)
    {
        RequireReading(start <= end && end <= _length && std::isfinite(height) && height > 0);
        _lines.push_back({{start, end}, block, x, y, 14, height, 0});
    }
    void ReadingNavigationIndex::AddParagraph(uint32_t start, uint32_t end)
    {
        RequireReading(start <= end && end <= _length);
        _paragraphs.push_back({start, end});
    }
    void ReadingNavigationIndex::AddBlock(notepads_markdown_preview_block const &block)
    {
        RequireReading(block.logical_start <= block.logical_end && block.logical_end <= _length &&
                       std::isfinite(block.y) && std::isfinite(block.height));
        _blocks.push_back({{static_cast<uint32_t>(block.logical_start), static_cast<uint32_t>(block.logical_end)},
                           block.node,
                           block.y,
                           block.height});
    }
    void ReadingTableCaretAlias::Validate(MainTextSnapshot const &main) const
    {
        RequireReading(first <= last && last <= main.Length() && target <= main.Length());
        const bool forward = target > last;
        const bool emptyCaption = kind == ReadingTableAliasKind::EmptyCaption;
        RequireReading(forward || target < first);
        RequireReading(kind != ReadingTableAliasKind::ColumnSpan || !forward);
        RequireReading(!emptyCaption || (forward && target - last == 1));
        auto begin = forward ? first : target;
        auto end = forward ? target : last;
        main.ValidateRange(begin, end - begin);
        uint32_t lineFeeds{};
        for (auto position = begin; position < end; ++position)
        {
            auto ch = main.Text()[position];
            RequireReading(ch == L'\t' || (kind != ReadingTableAliasKind::ColumnSpan && ch == L'\n'));
            lineFeeds += ch == L'\n';
        }
        RequireReading(!emptyCaption || lineFeeds == 1);
    }

    void ReadingNavigationIndex::AddTableCaretAlias(ReadingTableCaretAlias alias)
    {
        RequireReading(alias.first <= alias.last && alias.last <= _length && alias.target <= _length);
        _tableAliases.push_back(alias);
    }
    uint32_t ReadingNavigationIndex::CanonicalCaret(uint32_t position) const
    {
        position = std::clamp(position, _scope.start, _scope.end);
        auto at = std::lower_bound(_tableAliases.begin(), _tableAliases.end(), position,
                                   [](auto const &alias, uint32_t point) { return alias.last < point; });
        return at != _tableAliases.end() && at->first <= position ? at->target : position;
    }
    void ReadingNavigationIndex::Finish(MainTextSnapshot const &main, ReadingWordSegments *wordSegments)
    {
        RequireReading(main.Length() == _length &&
                       std::memcmp(&main.Identity(), &_key.model, sizeof(notepads_markdown_core_identity)) == 0);
        std::sort(_tableAliases.begin(), _tableAliases.end(),
                  [](auto const &a, auto const &b) { return a.first < b.first; });
        uint32_t priorAlias{};
        bool firstAlias = true;
        for (auto const &alias : _tableAliases)
        {
            RequireReading(firstAlias || alias.first > priorAlias);
            firstAlias = false;
            priorAlias = alias.last;
            alias.Validate(main);
        }
        for (auto const &alias : _tableAliases)
            RequireReading(CanonicalCaret(alias.target) == alias.target);
        auto normalize = [&](auto &points)
        {
            for (auto &point : points)
                point = CanonicalCaret(point);
            SortUnique(points);
        };
        auto last = _scope.end;
        if (_visibility && last > _scope.start)
            if (auto hidden = _visibility->HiddenAt(last - 1))
                last = std::max(hidden->start, _scope.start);
        _document = {CanonicalCaret(_scope.start), CanonicalCaret(last)};
        std::sort(_coverage.begin(), _coverage.end(),
                  [](auto a, auto b) { return a.start < b.start || (a.start == b.start && a.end < b.end); });
        uint32_t cursor = _scope.start;
        auto gap = [&](uint32_t end)
        {
            while (cursor < end)
            {
                if (_visibility)
                    if (auto hidden = _visibility->HiddenAt(cursor))
                    {
                        RequireReading((cursor == hidden->start || cursor == _scope.start) &&
                                       (hidden->end <= end || end == _scope.end));
                        _carets.push_back(cursor);
                        cursor = std::min(hidden->end, end);
                        if (cursor < _scope.end)
                            _carets.push_back(cursor);
                        continue;
                    }
                auto ch = main.Text()[cursor];
                RequireReading(ch == L'\n' || ch == L'\r' || ch == L'\t' || ch == L' ' || ch == L'\xfffc');
                _carets.push_back(cursor++);
                _carets.push_back(cursor);
            }
        };
        for (auto range : _coverage)
        {
            RequireReading(range.start >= cursor && range.end <= _scope.end);
            RequireReading(!_visibility || !_visibility->FirstHidden(range));
            gap(range.start);
            cursor = range.end;
        }
        gap(_scope.end);
        _carets.push_back(_scope.start);
        _carets.push_back(last);
        normalize(_carets);
        for (auto point : _carets)
            RequireReading(main.IsBoundary(point));
        std::sort(_lines.begin(), _lines.end(),
                  [](auto const &a, auto const &b) { return a.text.start < b.text.start; });
        double followY = -std::numeric_limits<double>::infinity();
        for (auto &line : _lines)
        {
            RequireReading(line.block < _blocks.size());
            line.y = _blocks[line.block].y + line.localY;
            RequireReading(std::isfinite(line.y));
            followY = std::max(followY, line.y);
            line.followY = followY;
        }
        RequireReading(_lines.size() <= UINT32_MAX);
        _visualLines.reserve(_lines.size());
        for (uint32_t i = 0; i < _lines.size(); ++i)
            _visualLines.push_back(i);
        std::sort(_visualLines.begin(), _visualLines.end(),
                  [&](auto a, auto b)
                  {
                      auto const &left = _lines[a];
                      auto const &right = _lines[b];
                      return left.y < right.y || (left.y == right.y && left.x < right.x);
                  });
        _visualLineBottoms.reserve(_visualLines.size());
        auto bottom = -std::numeric_limits<double>::infinity();
        for (auto i : _visualLines)
        {
            bottom = std::max(bottom, _lines[i].y + _lines[i].height);
            _visualLineBottoms.push_back(bottom);
        }
        std::sort(_paragraphs.begin(), _paragraphs.end(), [](auto a, auto b) { return a.start < b.start; });
        _lineStops.push_back(_scope.start);
        for (auto const &line : _lines)
            _lineStops.push_back(line.text.start);
        _lineStops.push_back(last);
        normalize(_lineStops);
        _paragraphStops.push_back(_scope.start);
        for (auto range : _paragraphs)
            _paragraphStops.push_back(range.start);
        _paragraphStops.push_back(last);
        normalize(_paragraphStops);

        using namespace winrt::Windows::Data::Text;
        auto languages = winrt::Windows::System::UserProfile::GlobalizationPreferences::Languages();
        auto language = languages.Size() ? languages.GetAt(0) : winrt::hstring{L"en-US"};
        _words.push_back(_scope.start);
        _words.push_back(last);
        auto addWord = [&](ReadingSpan span)
        {
            if (!_visibility || !_visibility->FirstHidden(span))
            {
                _words.push_back(Snap(span.start, false));
                _words.push_back(Snap(span.end, true));
            }
        };
        if (wordSegments && wordSegments->textGeneration == main.TextGeneration() &&
            wordSegments->scope.start == _scope.start && wordSegments->scope.end == _scope.end &&
            wordSegments->language == language)
        {
            for (auto span : wordSegments->spans)
                addWord(span);
        }
        else
        {
            if (wordSegments)
                *wordSegments = ReadingWordSegments{_memory};
            SelectableWordsSegmenter segmenter{language};
            auto segment = [&]
            {
                auto funded = main.CopyText(_scope.start, _scope.end - _scope.start);
                return segmenter.GetTokens(winrt::hstring{std::wstring_view(funded.data(), funded.size())});
            };
            auto tokens = segment();
            auto at = tokens.begin();
            auto end = tokens.end();
            auto boundaryCount = _words.size();
            auto gather = [&](ReadingWordSegments *replacement)
            {
                auto length = _scope.end - _scope.start;
                for (; at != end; ++at)
                {
                    auto range = (*at).SourceTextSegment();
                    RequireReading(range.StartPosition <= length && range.Length <= length - range.StartPosition);
                    auto start = _scope.start + range.StartPosition;
                    ReadingSpan span{start, start + range.Length};
                    addWord(span);
                    if (replacement)
                        replacement->spans.push_back(span);
                    boundaryCount = _words.size();
                }
            };

            // Preserve raw spans across relayouts; visibility and caret snapping
            // are recomputed for the current layout. Bound the optional cache.
            constexpr size_t maxCachedSpans = (1024 * 1024) / sizeof(ReadingSpan);
            auto tokenCount = tokens.Size();
            if (wordSegments && tokenCount <= maxCachedSpans)
            {
                try
                {
                    ReadingWordSegments replacement{_memory};
                    replacement.spans.reserve(tokenCount);
                    gather(&replacement);
                    replacement.textGeneration = main.TextGeneration();
                    replacement.scope = _scope;
                    replacement.language = language;
                    *wordSegments = std::move(replacement);
                }
                catch (ReadingFailure const &error)
                {
                    if (error.status != NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT &&
                        error.status != NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY)
                        throw;
                    // Optional storage is gone. Retry any partially appended
                    // word without losing the boundaries already built.
                    _words.resize(boundaryCount);
                }
            }
            gather(nullptr);
        }
        normalize(_words);
    }
    uint32_t ReadingNavigationIndex::SnapTextBoundary(uint32_t position, bool forward) const
    {
        position = std::clamp(position, _scope.start, _scope.end);
        // Finish only removes validated one-unit TSV separator boundaries from
        // the caret stops. Restore those exact text endpoints for nonempty
        // ranges without retaining a duplicate full-document cluster index.
        auto alias = std::lower_bound(_tableAliases.begin(), _tableAliases.end(), position,
                                      [](auto const &value, uint32_t point) { return value.last < point; });
        if (alias != _tableAliases.end() && alias->first <= position)
            return position;
        auto next = std::lower_bound(_carets.begin(), _carets.end(), position);
        if (next == _carets.end())
            return _carets.back();
        if (*next == position || forward || next == _carets.begin())
            return *next;
        return *--next;
    }
    uint32_t ReadingNavigationIndex::Snap(uint32_t position, bool forward) const
    {
        return CanonicalCaret(SnapTextBoundary(CanonicalCaret(position), forward));
    }
    void ReadingNavigationIndex::SnapSelection(uint32_t &anchor, uint32_t &active) const
    {
        if (anchor == active)
            anchor = active = Snap(active, true);
        else
        {
            auto forward = anchor < active;
            anchor = SnapTextBoundary(anchor, !forward);
            active = SnapTextBoundary(active, forward);
        }
    }
    std::span<uint32_t const> ReadingNavigationIndex::Boundaries(TextUnit unit) const
    {
        if (unit == TextUnit::Character)
            return _carets;
        if (unit == TextUnit::Word || unit == TextUnit::Format)
            return _words;
        if (unit == TextUnit::Line)
            return _lineStops;
        if (unit == TextUnit::Paragraph)
            return _paragraphStops;
        return _document;
    }
    uint32_t ReadingNavigationIndex::Move(uint32_t position, TextUnit unit, int32_t count, int32_t &moved) const
    {
        moved = 0;
        position = CanonicalCaret(position);
        if (!count)
            return position;
        auto points = Boundaries(unit);
        if (count > 0)
        {
            auto next = std::upper_bound(points.begin(), points.end(), position);
            if (next == points.end())
                return position;
            auto first = static_cast<int64_t>(next - points.begin());
            auto target = std::min<int64_t>(points.size() - 1, first + static_cast<int64_t>(count) - 1);
            moved = static_cast<int32_t>(target - first + 1);
            return points[static_cast<size_t>(target)];
        }
        auto next = std::lower_bound(points.begin(), points.end(), position);
        if (next == points.begin())
            return position;
        auto first = static_cast<int64_t>(next - points.begin()) - 1;
        auto target = std::max<int64_t>(0, first + static_cast<int64_t>(count) + 1);
        moved = -static_cast<int32_t>(first - target + 1);
        return points[static_cast<size_t>(target)];
    }
    ReadingSpan ReadingNavigationIndex::Unit(uint32_t position, TextUnit unit) const
    {
        // A nonempty document range includes Main's exact TSV separators.
        // Caret movement still uses the canonical stops in _document.
        if (unit == TextUnit::Document || unit == TextUnit::Page)
            return _scope;
        position = CanonicalCaret(position);
        if (unit == TextUnit::Line && !_lines.empty())
            return _lines[LineAt(position)].text;
        if (unit == TextUnit::Paragraph && !_paragraphs.empty())
        {
            auto it = std::upper_bound(_paragraphs.begin(), _paragraphs.end(), position,
                                       [](uint32_t value, auto span) { return value < span.start; });
            return it == _paragraphs.begin() ? _paragraphs.front() : *--it;
        }
        auto points = Boundaries(unit);
        if (points.size() < 2)
            return {position, position};
        auto end = std::upper_bound(points.begin(), points.end(), position);
        if (end == points.begin())
            return {points.front(), points.front()};
        if (end == points.end())
        {
            if (points.size() < 2)
                return {position, position};
            return {points[points.size() - 2], points.back()};
        }
        return {*(end - 1), *end};
    }
    size_t ReadingNavigationIndex::LineAt(uint32_t position, bool trailing) const
    {
        position = CanonicalCaret(position);
        if (_lines.empty())
            return 0;
        auto it = std::upper_bound(_lines.begin(), _lines.end(), position,
                                   [](uint32_t value, auto const &line) { return value < line.text.start; });
        auto index = it == _lines.begin() ? 0 : static_cast<size_t>(it - _lines.begin() - 1);
        if (trailing && index && _lines[index].text.start == position &&
            _lines[index - 1].wrappedEnd && _lines[index - 1].text.end == position)
            --index;
        return index;
    }
    std::span<uint32_t const> ReadingNavigationIndex::VisibleLines(double top, double bottom) const noexcept
    {
        // Script and ruby rectangles can extend beyond their owner's line box.
        // Expand the query once; nominal tops and prefix bottoms remain sorted.
        top -= _visualOverflowBelow;
        bottom += _visualOverflowAbove;
        auto first = std::upper_bound(_visualLineBottoms.begin(), _visualLineBottoms.end(), top);
        auto lines = std::span{_visualLines}.subspan(static_cast<size_t>(first - _visualLineBottoms.begin()));
        auto last = std::lower_bound(lines.begin(), lines.end(), bottom,
                                     [&](auto index, double y) { return _lines[index].y < y; });
        return lines.first(static_cast<size_t>(last - lines.begin()));
    }
    size_t ReadingNavigationIndex::VisualLine(size_t current, bool forward, double pageDistance, float preferredX) const
    {
        if (current >= _lines.size())
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_RANGE};
        auto const &origin = _lines[current];
        if (_visualLines.empty())
            return current;
        const auto lineCount = static_cast<int64_t>(_visualLines.size());
        const auto direction = forward ? 1 : -1;
        const auto validCursor = [lineCount](int64_t index) { return index >= 0 && index < lineCount; };
        const auto reachedOrigin = [&](double y)
        {
            return (forward && y <= origin.y + 0.25) || (!forward && y >= origin.y - 0.25);
        };
        const auto distanceFromPreferredX = [preferredX](ReadingLine const &line)
        {
            return std::max({line.x - preferredX, preferredX - line.x - line.width, 0.0f});
        };
        const auto overlappingColumn = [&](ReadingLine const &line)
        {
            return line.tableCell && line.x < origin.x + origin.width - 0.25f &&
                   origin.x < line.x + line.width - 0.25f;
        };
        auto closestOnRow = [&](int64_t &cursor, int step, double y, bool sameTableOnly)
        {
            std::optional<size_t> best;
            float bestDistance = std::numeric_limits<float>::infinity();
            do
            {
                auto index = _visualLines[static_cast<size_t>(cursor)];
                auto const &candidate = _lines[index];
                auto distance = distanceFromPreferredX(candidate);
                const bool sameTable = origin.tableCell && candidate.block == origin.block;
                // Adjacent cells that overlap a colspan remain viable when the
                // preferred X falls in the intervening padding.
                const bool eligible = sameTable
                    ? (distance <= 0.25f || overlappingColumn(candidate)) && (!sameTableOnly || candidate.tableCell)
                    : !sameTableOnly;
                if (eligible && !reachedOrigin(candidate.y) && distance < bestDistance)
                {
                    best = index;
                    bestDistance = distance;
                }
                cursor += step;
            } while (validCursor(cursor) && std::abs(_lines[_visualLines[static_cast<size_t>(cursor)]].y - y) <= 0.25);
            return best;
        };

        auto targetY = origin.y + (forward ? pageDistance : -pageDistance);
        auto at = forward ? std::lower_bound(_visualLines.begin(), _visualLines.end(),
                                             pageDistance > 0 ? targetY : origin.y + 0.25,
                                             [&](auto index, double value) { return _lines[index].y < value; })
                          : std::upper_bound(_visualLines.begin(), _visualLines.end(),
                                             pageDistance > 0 ? targetY : origin.y - 0.25,
                                             [&](double value, auto index) { return value < _lines[index].y; });
        int64_t cursor = static_cast<int64_t>(at - _visualLines.begin()) - (forward ? 0 : 1);
        if (pageDistance > 0)
        {
            cursor = std::clamp<int64_t>(cursor, 0, lineCount - 1);
            // Clamping can land on the wrong edge of a multi-column row.
            // Include every cell in that row when choosing the preferred X.
            auto y = _lines[_visualLines[static_cast<size_t>(cursor)]].y;
            while (validCursor(cursor - direction) &&
                   std::abs(_lines[_visualLines[static_cast<size_t>(cursor - direction)]].y - y) <= 0.25)
                cursor -= direction;
        }
        while (validCursor(cursor))
        {
            auto y = _lines[_visualLines[static_cast<size_t>(cursor)]].y;
            if (reachedOrigin(y))
                return current;
            if (auto best = closestOnRow(cursor, direction, y, false))
                return *best;
        }
        if (pageDistance > 0 && origin.tableCell)
        {
            // The target can lie past this column's last line while another
            // cell continues wrapping. Search back from the document edge for
            // the farthest same-column line strictly beyond the origin.
            cursor = forward ? lineCount - 1 : 0;
            while (validCursor(cursor))
            {
                auto y = _lines[_visualLines[static_cast<size_t>(cursor)]].y;
                if (reachedOrigin(y))
                    break;
                if (auto best = closestOnRow(cursor, -direction, y, true))
                    return *best;
            }
        }
        return current;
    }
    ReadingLine const &ReadingNavigationIndex::Line(size_t index) const
    {
        if (index >= _lines.size())
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_RANGE};
        return _lines[index];
    }
    double ReadingNavigationIndex::RevealY(uint32_t position) const
    {
        if (!_lines.empty())
            return _lines[LineAt(position)].y;
        return 0;
    }
    double ReadingNavigationIndex::RevealBottomY(ReadingSpan span) const
    {
        if (_lines.empty())
            return 0;
        auto lineBottom = [](ReadingLine const &line) { return line.y + line.height; };
        if (span.start == span.end)
            return lineBottom(_lines[LineAt(span.start)]);

        // For nonempty ranges, separators belong to the preceding measured line
        // up to the next line's start. Raw positions keep caret aliases separate.
        auto after = std::lower_bound(_lines.begin(), _lines.end(), span.end,
            [](auto const &line, uint32_t end) { return line.text.start < end; });
        if (after == _lines.begin())
            return lineBottom(_lines.front());
        auto last = after - 1;
        auto bottom = lineBottom(*last);
        if (last->tableCell)
        {
            // Text order groups lines by cell, so an earlier cell can extend
            // farther down than the logically last line of the selected range.
            for (auto at = last; at != _lines.begin() && at->text.start > span.start;)
            {
                auto const &line = *--at;
                if (line.block != last->block)
                    break;
                bottom = std::max(bottom, lineBottom(line));
            }
        }
        return bottom;
    }
    double ReadingNavigationIndex::FollowY(uint32_t position) const
    {
        if (!_lines.empty())
            return _lines[LineAt(position)].followY;
        return 0;
    }
    double ReadingNavigationIndex::ContinuousFollowY(uint32_t position) const
    {
        if (position > _length)
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_RANGE};
        position = CanonicalCaret(position);
        if (_visibility)
            if (auto hidden = _visibility->HiddenAt(position))
                return FollowY(hidden->start);
        auto line = [&](size_t index)
        {
            auto const &value = _lines[index];
            return FollowCoordinates::Line{value.text.start, value.followY, value.height};
        };
        double y{};
        RequireReading(FollowCoordinates::Ordinate(_lines.size(), _scope.end, position, line, y));
        return y;
    }
    FollowCoordinates::Span ReadingNavigationIndex::ContinuousFollowSpan(double y) const
    {
        if (!std::isfinite(y))
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_RANGE};
        auto line = [&](size_t index)
        {
            auto const &value = _lines[index];
            return FollowCoordinates::Line{value.text.start, value.followY, value.height};
        };
        FollowCoordinates::Span span;
        RequireReading(FollowCoordinates::Inverse(_lines.size(), _scope.end, y, line, span));
        span.start = CanonicalCaret(span.start);
        span.end = CanonicalCaret(span.end);
        if (_visibility)
        {
            if (auto hidden = _visibility->HiddenAt(span.start))
                span.start = span.end = hidden->start;
            else if (auto crossing = _visibility->FirstHidden({span.start, span.end}))
                span.end = crossing->start;
        }
        if (span.start == span.end)
            span.progress = 0;
        return span;
    }
    double ReadingNavigationIndex::RevealBlockY(uint32_t node) const
    {
        // Block ordinals remain in measurement order for ReadingLine::block.
        // Navigation is occasional; avoid a second permanent node/geometry cache.
        std::optional<double> result;
        for (auto const &block : _blocks)
        {
            if (block.node != node)
                continue;
            RequireReading(!result.has_value());
            result = block.y;
        }
        RequireReading(result.has_value());
        return *result;
    }
}
