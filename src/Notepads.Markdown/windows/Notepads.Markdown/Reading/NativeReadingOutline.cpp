// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Reading/NativeReadingOutline.h"
#include <algorithm>
#include <array>
#include <cstring>
#include <limits>

namespace Notepads::Markdown::Native
{
    namespace
    {
        template <typename T> T CorePod() noexcept
        {
            T result{};
            result.header = {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(T)};
            return result;
        }

        void Check(int32_t status)
        {
            if (status != NOTEPADS_MARKDOWN_CORE_OK)
                throw ReadingFailure{status};
        }

        void Require(bool valid)
        {
            if (!valid)
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
        }

        struct OutlineOwner
        {
            notepads_markdown_outline *value{};
            ~OutlineOwner() noexcept
            {
                if (value)
                    (void)notepads_markdown_outline_destroy(value);
            }
        };

        notepads_markdown_outline_summary Summary(notepads_markdown_outline *outline)
        {
            notepads_markdown_outline_summary result{};
            result.header = {NOTEPADS_MARKDOWN_OUTLINE_ABI_VERSION, sizeof(result)};
            auto error = CorePod<notepads_markdown_core_error>();
            Check(notepads_markdown_outline_get_summary(outline, &result, &error));
            Require(result.header.abi_version == NOTEPADS_MARKDOWN_OUTLINE_ABI_VERSION &&
                    result.header.struct_size == sizeof(result));
            return result;
        }

        void Read(notepads_markdown_outline *outline, uint32_t table, uint64_t start, uint32_t count, uint32_t rowSize, void *buffer,
                  uint64_t total)
        {
            auto error = CorePod<notepads_markdown_core_error>();
            auto result = CorePod<notepads_markdown_core_read_result>();
            Check(notepads_markdown_outline_read(outline, table, start, count, rowSize, buffer, &result, &error));
            Require(result.written == count && result.next == start + count &&
                    result.done == static_cast<uint32_t>(result.next == total) && result.reserved == 0);
        }

        ReadingString FromUtf8(std::span<uint8_t const> blob, notepads_markdown_outline_blob slice,
                               std::shared_ptr<ReadingMemory> const &memory)
        {
            Require(slice.start <= blob.size() && slice.length <= blob.size() - slice.start);
            if (slice.length > INT_MAX)
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_ARITHMETIC_OVERFLOW};
            ReadingString result{ReadingAllocator<wchar_t>{memory}};
            if (!slice.length)
                return result;
            auto source = reinterpret_cast<char const *>(blob.data() + static_cast<size_t>(slice.start));
            auto length = static_cast<int>(slice.length);
            auto units = ::MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, source, length, nullptr, 0);
            if (!units)
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_UTF8};
            result.resize(static_cast<size_t>(units));
            if (::MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, source, length, result.data(), units) != units)
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_UTF8};
            return result;
        }

        int Hex(wchar_t value) noexcept
        {
            if (value >= L'0' && value <= L'9')
                return value - L'0';
            if (value >= L'a' && value <= L'f')
                return value - L'a' + 10;
            if (value >= L'A' && value <= L'F')
                return value - L'A' + 10;
            return -1;
        }

        // emit receives one encoded byte. The counting pass and the writing
        // pass share syntax validation but never call a URI/path normalizer.
        template <typename Emit> bool VisitFragment(std::wstring_view input, Emit &&emit)
        {
            for (size_t offset = 1; offset < input.size(); ++offset)
            {
                uint32_t scalar = input[offset];
                if (scalar == L'%')
                {
                    if (input.size() - offset < 3)
                        return false;
                    auto high = Hex(input[offset + 1]);
                    auto low = Hex(input[offset + 2]);
                    if (high < 0 || low < 0 || (high == 0 && low == 0))
                        return false;
                    emit(static_cast<uint8_t>((high << 4) | low));
                    offset += 2;
                    continue;
                }
                if (!scalar)
                    return false;
                if (scalar >= 0xD800 && scalar < 0xDC00)
                {
                    if (offset + 1 == input.size())
                        return false;
                    auto low = static_cast<uint32_t>(input[++offset]);
                    if (low < 0xDC00 || low >= 0xE000)
                        return false;
                    scalar = 0x10000 + ((scalar - 0xD800) << 10) + low - 0xDC00;
                }
                else if (scalar >= 0xDC00 && scalar < 0xE000)
                {
                    return false;
                }
                if (scalar < 0x80)
                {
                    emit(static_cast<uint8_t>(scalar));
                }
                else if (scalar < 0x800)
                {
                    emit(static_cast<uint8_t>(0xC0 | (scalar >> 6)));
                    emit(static_cast<uint8_t>(0x80 | (scalar & 0x3F)));
                }
                else if (scalar < 0x10000)
                {
                    emit(static_cast<uint8_t>(0xE0 | (scalar >> 12)));
                    emit(static_cast<uint8_t>(0x80 | ((scalar >> 6) & 0x3F)));
                    emit(static_cast<uint8_t>(0x80 | (scalar & 0x3F)));
                }
                else
                {
                    emit(static_cast<uint8_t>(0xF0 | (scalar >> 18)));
                    emit(static_cast<uint8_t>(0x80 | ((scalar >> 12) & 0x3F)));
                    emit(static_cast<uint8_t>(0x80 | ((scalar >> 6) & 0x3F)));
                    emit(static_cast<uint8_t>(0x80 | (scalar & 0x3F)));
                }
            }
            return true;
        }
    }

    ReadingOutline::ReadingOutline(ConstructionKey, std::shared_ptr<ReadingMemory> memory, uint64_t textGeneration,
                                   ReadingVector<ReadingHeadingDescriptor> headings,
                                   ReadingVector<uint32_t> anchors,
                                   ReadingVector<uint32_t> nodes,
                                   ReadingVector<ReadingAnchorDescriptor> customAnchors) noexcept
        : _memory(std::move(memory)), _textGeneration(textGeneration), _headings(std::move(headings)),
          _anchors(std::move(anchors)), _nodes(std::move(nodes)), _customAnchors(std::move(customAnchors))
    {
    }

    std::shared_ptr<ReadingOutline const> ReadingOutline::Build(MainTextSnapshot const &main, notepads_markdown_core_cancel *sourceFlag)
    {
        if (!sourceFlag || !main.Memory())
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
        if (notepads_markdown_outline_abi_version() != NOTEPADS_MARKDOWN_OUTLINE_ABI_VERSION)
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_ABI_MISMATCH};
        auto const &memory = main.Memory();
        OutlineOwner outline;
        auto error = CorePod<notepads_markdown_core_error>();
        Check(notepads_markdown_outline_create(main.ModelOwner().Get(), sourceFlag, &outline.value, &error));
        auto summary = Summary(outline.value);
        Require(std::memcmp(&summary.model, &main.Identity(), sizeof(summary.model)) == 0);
        if (summary.headings > UINT32_MAX || summary.anchors > UINT32_MAX || summary.blob_bytes > SIZE_MAX)
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_ARITHMETIC_OVERFLOW};
        ReadingVector<uint8_t> blob{ReadingAllocator<uint8_t>{memory}};
        blob.resize(static_cast<size_t>(summary.blob_bytes));
        for (uint64_t start = 0; start < summary.blob_bytes;)
        {
            auto count = static_cast<uint32_t>(std::min<uint64_t>(65536, summary.blob_bytes - start));
            Read(outline.value, NOTEPADS_MARKDOWN_OUTLINE_BLOB, start, count, 1, blob.data() + static_cast<size_t>(start),
                 summary.blob_bytes);
            start += count;
        }
        ReadingVector<ReadingHeadingDescriptor> headings{ReadingAllocator<ReadingHeadingDescriptor>{memory}};
        ReadingVector<uint32_t> anchors{ReadingAllocator<uint32_t>{memory}};
        headings.reserve(static_cast<size_t>(summary.headings));
        anchors.reserve(static_cast<size_t>(summary.headings));
        uint64_t priorEnd{};
        for (uint64_t start = 0; start < summary.headings;)
        {
            std::array<notepads_markdown_outline_heading, 256> rows{};
            auto count = static_cast<uint32_t>(std::min<uint64_t>(rows.size(), summary.headings - start));
            Read(outline.value, NOTEPADS_MARKDOWN_OUTLINE_HEADINGS, start, count, sizeof(notepads_markdown_outline_heading), rows.data(),
                 summary.headings);
            for (uint32_t index = 0; index < count; ++index)
            {
                auto const &row = rows[index];
                Require(row.node != 0 && row.level >= 1 && row.level <= 6 && row.logical_start >= priorEnd &&
                        row.logical_start <= row.logical_end && row.logical_end <= main.Length());
                main.ValidateRange(static_cast<uint32_t>(row.logical_start),
                                   static_cast<uint32_t>(row.logical_end - row.logical_start));
                ReadingHeadingDescriptor heading{memory};
                heading.node = row.node;
                heading.level = row.level;
                heading.text = {static_cast<uint32_t>(row.logical_start), static_cast<uint32_t>(row.logical_end)};
                heading.title = FromUtf8(blob, row.title, memory);
                heading.anchor = FromUtf8(blob, row.anchor, memory);
                headings.push_back(std::move(heading));
                anchors.push_back(static_cast<uint32_t>(start + index));
                priorEnd = row.logical_end;
            }
            start += count;
        }
        auto nodes = anchors;
        std::sort(nodes.begin(), nodes.end(),
                  [&](auto left, auto right) { return headings[left].node < headings[right].node; });
        for (size_t index = 1; index < nodes.size(); ++index)
            Require(headings[nodes[index - 1]].node != headings[nodes[index]].node);
        std::sort(anchors.begin(), anchors.end(),
                  [&](auto left, auto right)
                  {
                      if (headings[left].anchor != headings[right].anchor)
                          return headings[left].anchor < headings[right].anchor;
                      return left < right;
                  });
        ReadingVector<ReadingAnchorDescriptor> custom{ReadingAllocator<ReadingAnchorDescriptor>{memory}};
        custom.reserve(static_cast<size_t>(summary.anchors));
        for (uint64_t start = 0; start < summary.anchors;)
        {
            std::array<notepads_markdown_outline_anchor, 256> rows{};
            auto count = static_cast<uint32_t>(std::min<uint64_t>(rows.size(), summary.anchors - start));
            Read(outline.value, NOTEPADS_MARKDOWN_OUTLINE_ANCHORS, start, count,
                 sizeof(notepads_markdown_outline_anchor), rows.data(), summary.anchors);
            for (uint32_t index = 0; index < count; ++index)
            {
                auto const &row = rows[index];
                Require(row.position <= main.Length() && row.legacy <= 1 && row.name.length != 0);
                ReadingAnchorDescriptor anchor{memory};
                anchor.position = static_cast<uint32_t>(row.position);
                anchor.node = row.node;
                main.ValidateRange(anchor.position, 0);
                anchor.legacy = row.legacy != 0;
                anchor.name = FromUtf8(blob, row.name, memory);
                custom.push_back(std::move(anchor));
            }
            start += count;
        }
        std::sort(custom.begin(), custom.end(), [](auto const &left, auto const &right)
        {
            if (left.name != right.name)
                return left.name < right.name;
            if (left.legacy != right.legacy)
                return !left.legacy;
            return left.position < right.position;
        });
        // This final FFI call checks the original cancellation/first-fault state
        // after all local conversion, including the zero-heading case.
        auto final = Summary(outline.value);
        Require(std::memcmp(&final, &summary, sizeof(final)) == 0);
        return std::allocate_shared<ReadingOutline>(ReadingAllocator<ReadingOutline>{memory}, ConstructionKey{}, memory,
                                                    main.TextGeneration(), std::move(headings), std::move(anchors),
                                                    std::move(nodes), std::move(custom));
    }

    std::shared_ptr<ReadingMemory> const &ReadingOutline::Memory() const noexcept
    {
        return _memory;
    }

    uint64_t ReadingOutline::TextGeneration() const noexcept
    {
        return _textGeneration;
    }

    std::span<ReadingHeadingDescriptor const> ReadingOutline::Headings() const noexcept
    {
        return {_headings.data(), _headings.size()};
    }

    std::optional<size_t> ReadingOutline::FindAnchor(std::wstring_view anchor) const noexcept
    {
        auto found = std::lower_bound(_anchors.begin(), _anchors.end(), anchor, [&](auto index, auto needle)
                                     { return std::wstring_view{_headings[index].anchor} < needle; });
        if (found == _anchors.end() || std::wstring_view{_headings[*found].anchor} != anchor)
            return std::nullopt;
        return *found;
    }

    ReadingHeadingDescriptor const *ReadingOutline::FindNode(uint32_t node) const noexcept
    {
        auto found = std::lower_bound(_nodes.begin(), _nodes.end(), node,
                                     [&](auto index, auto needle) { return _headings[index].node < needle; });
        return found != _nodes.end() && _headings[*found].node == node ? &_headings[*found] : nullptr;
    }

    ReadingHeadingDescriptor const *ReadingOutline::FindTitle(std::wstring_view title) const noexcept
    {
        if (title.empty() || title.size() > static_cast<size_t>((std::numeric_limits<int>::max)()))
            return nullptr;
        auto equal = [](std::wstring_view left, std::wstring_view right)
        {
            if (left.size() != right.size())
                return false;
            auto length = static_cast<int>(right.size());
            return CompareStringOrdinal(left.data(), length, right.data(), length, TRUE) == CSTR_EQUAL;
        };
        auto found = std::find_if(_headings.begin(), _headings.end(), [&](auto const &heading)
        {
            return equal(heading.title, title);
        });
        if (found != _headings.end())
            return &*found;
        if (title.find(L'#') == std::wstring_view::npos)
            return nullptr;

        std::array<std::wstring_view, 6> titles;
        size_t count{};
        for (;;)
        {
            auto separator = title.find(L'#');
            auto part = title.substr(0, separator);
            auto start = part.find_first_not_of(L" \t");
            if (count == titles.size() || start == std::wstring_view::npos)
                return nullptr;
            titles[count++] = part.substr(start, part.find_last_not_of(L" \t") - start + 1);
            if (separator == std::wstring_view::npos)
                break;
            title.remove_prefix(separator + 1);
        }

        std::array<ReadingHeadingDescriptor const *, 6> ancestors;
        size_t depth{};
        for (auto const &heading : _headings)
        {
            while (depth && ancestors[depth - 1]->level >= heading.level)
                --depth;
            ancestors[depth++] = &heading;
            if (count > depth || !equal(heading.title, titles[count - 1]))
                continue;
            auto first = ancestors.begin() + (depth - count);
            if (std::equal(first, ancestors.begin() + depth, titles.begin(),
                           [&](auto entry, auto expected) { return equal(entry->title, expected); }))
                return &heading;
        }
        return nullptr;
    }

    std::optional<ReadingSection> ReadingOutline::Section(std::wstring_view title, uint32_t end) const noexcept
    {
        auto heading = FindTitle(title);
        if (!heading)
            return {};
        auto last = _headings.data() + _headings.size();
        auto next = std::find_if(heading + 1, last, [&](auto const &candidate)
            { return candidate.level <= heading->level; });
        return ReadingSection{heading->node, {heading->text.start, next == last ? end : next->text.start},
                              static_cast<size_t>(heading - _headings.data()), static_cast<size_t>(next - _headings.data())};
    }

    ReadingAnchorDescriptor const *ReadingOutline::FindCustomAnchor(std::wstring_view anchor) const noexcept
    {
        auto found = std::lower_bound(_customAnchors.begin(), _customAnchors.end(), anchor,
                                     [](auto const &row, auto needle) { return std::wstring_view{row.name} < needle; });
        if (found == _customAnchors.end() || std::wstring_view{found->name} != anchor)
            return nullptr;
        return &*found;
    }

    std::optional<ReadingString> DecodeReadingFragment(std::wstring_view destination,
                                                       std::shared_ptr<ReadingMemory> const &memory)
    {
        if (destination.empty() || destination.front() != L'#')
            return std::nullopt;
        if (!memory)
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
        size_t byteCount{};
        auto count = [&](uint8_t)
        {
            if (byteCount == INT_MAX)
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_ARITHMETIC_OVERFLOW};
            ++byteCount;
        };
        if (!VisitFragment(destination, count))
            return std::nullopt;
        ReadingVector<uint8_t> bytes{ReadingAllocator<uint8_t>{memory}};
        bytes.reserve(byteCount);
        if (!VisitFragment(destination, [&](uint8_t value) { bytes.push_back(value); }))
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
        ReadingString result{ReadingAllocator<wchar_t>{memory}};
        if (!byteCount)
            return result;
        auto source = reinterpret_cast<char const *>(bytes.data());
        auto units = ::MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, source, static_cast<int>(byteCount), nullptr, 0);
        if (!units)
            return std::nullopt;
        result.resize(static_cast<size_t>(units));
        if (::MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, source, static_cast<int>(byteCount), result.data(),
                                  units) != units)
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
        return result;
    }
}
