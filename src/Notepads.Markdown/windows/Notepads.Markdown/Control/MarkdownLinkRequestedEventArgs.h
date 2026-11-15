// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "MarkdownLinkRequestedEventArgs.g.h"
#include <optional>
#include <utility>

namespace winrt::Notepads::Markdown::implementation
{
    struct MarkdownLinkRequestedEventArgs : MarkdownLinkRequestedEventArgsT<MarkdownLinkRequestedEventArgs>
    {
        MarkdownLinkRequestedEventArgs(guid document, uint64_t revision, uint64_t textGeneration,
                                       uint64_t start, uint64_t length, hstring destination, hstring title,
                                       std::optional<uint32_t> localPreviewPosition, uint32_t localHeadingNode,
                                       bool isWikiLink)
            : _document(document), _revision(revision), _textGeneration(textGeneration), _start(start),
              _length(length), _destination(std::move(destination)), _title(std::move(title)),
              _localPreviewPosition(localPreviewPosition), _localHeadingNode(localHeadingNode), _isWikiLink(isWikiLink)
        {
        }
        guid DocumentId() const noexcept
        {
            return _document;
        }
        uint64_t Revision() const noexcept
        {
            return _revision;
        }
        uint64_t TextGeneration() const noexcept
        {
            return _textGeneration;
        }
        uint64_t Start() const noexcept
        {
            return _start;
        }
        uint64_t Length() const noexcept
        {
            return _length;
        }
        hstring Destination() const
        {
            return _destination;
        }
        hstring Title() const
        {
            return _title;
        }
        Windows::Foundation::IReference<uint64_t> LocalPreviewPosition() const
        {
            return _localPreviewPosition ? box_value(static_cast<uint64_t>(*_localPreviewPosition)).as<Windows::Foundation::IReference<uint64_t>>() : nullptr;
        }
        uint32_t LocalHeadingNode() const noexcept
        {
            return _localHeadingNode;
        }
        bool IsWikiLink() const noexcept
        {
            return _isWikiLink;
        }

      private:
        guid _document;
        uint64_t _revision;
        uint64_t _textGeneration;
        uint64_t _start;
        uint64_t _length;
        hstring _destination;
        hstring _title;
        std::optional<uint32_t> _localPreviewPosition;
        uint32_t _localHeadingNode;
        bool _isWikiLink;
    };
}
