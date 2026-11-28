// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Reading/NativeReadingNavigation.h"
#include "notepads/markdown/outline.h"
#include <optional>
#include <span>

namespace Notepads::Markdown::Native
{
    struct ReadingHeadingDescriptor
    {
        uint32_t node{};
        uint32_t level{};
        ReadingSpan text;
        ReadingString title;
        ReadingString anchor;
        explicit ReadingHeadingDescriptor(std::shared_ptr<ReadingMemory> const &memory)
            : title(ReadingAllocator<wchar_t>{memory}), anchor(ReadingAllocator<wchar_t>{memory})
        {
        }
    };

    struct ReadingAnchorDescriptor
    {
        uint32_t position{};
        uint32_t node{};
        bool legacy{};
        ReadingString name;
        explicit ReadingAnchorDescriptor(std::shared_ptr<ReadingMemory> const &memory)
            : name(ReadingAllocator<wchar_t>{memory})
        {
        }
    };

    // One immutable, same-model semantic table. It never stores layout geometry
    // or a control; source/theme/layout currentness belongs to the view.
    class ReadingOutline final
    {
        struct ConstructionKey
        {
        };

      public:
        static std::shared_ptr<ReadingOutline const> Build(MainTextSnapshot const &main, notepads_markdown_core_cancel *sourceFlag);
        ReadingOutline(ConstructionKey, std::shared_ptr<ReadingMemory> memory, uint64_t textGeneration,
                       ReadingVector<ReadingHeadingDescriptor> headings, ReadingVector<uint32_t> anchors,
                       ReadingVector<uint32_t> nodes, ReadingVector<ReadingAnchorDescriptor> customAnchors) noexcept;
        ReadingOutline(ReadingOutline const &) = delete;
        ReadingOutline &operator=(ReadingOutline const &) = delete;
        std::shared_ptr<ReadingMemory> const &Memory() const noexcept;
        uint64_t TextGeneration() const noexcept;
        std::span<ReadingHeadingDescriptor const> Headings() const noexcept;
        // Returns the zero-based document-order row; comparison is exact UTF16.
        std::optional<size_t> FindAnchor(std::wstring_view anchor) const noexcept;
        ReadingHeadingDescriptor const *FindNode(uint32_t node) const noexcept;
        ReadingHeadingDescriptor const *FindTitle(std::wstring_view title) const noexcept;
        std::optional<ReadingSection> Section(std::wstring_view title, uint32_t end) const noexcept;
        ReadingAnchorDescriptor const *FindCustomAnchor(std::wstring_view anchor) const noexcept;

      private:
        std::shared_ptr<ReadingMemory> _memory;
        uint64_t _textGeneration{};
        ReadingVector<ReadingHeadingDescriptor> _headings;
        ReadingVector<uint32_t> _anchors;
        ReadingVector<uint32_t> _nodes;
        ReadingVector<ReadingAnchorDescriptor> _customAnchors;
    };

    // A pure '#...' destination with exactly one strict percent/UTF8 decode.
    // nullopt denotes invalid/non-fragment syntax, not resource exhaustion.
    // The empty returned string is the bare '#' document-top request.
    std::optional<ReadingString> DecodeReadingFragment(std::wstring_view destination,
                                                       std::shared_ptr<ReadingMemory> const &memory);
}
