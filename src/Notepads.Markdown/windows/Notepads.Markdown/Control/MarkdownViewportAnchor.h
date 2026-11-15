// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "MarkdownViewportAnchor.g.h"
#include <utility>

namespace winrt::Notepads::Markdown::implementation
{
    struct MarkdownViewportAnchor : MarkdownViewportAnchorT<MarkdownViewportAnchor>
    {
        MarkdownViewportAnchor(bool available, guid document, uint64_t revision, uint64_t textGeneration,
                               uint64_t layoutGeneration, uint64_t previewUtf16Offset, double offsetWithinLine,
                               uint64_t scrollStart, uint64_t scrollEnd, double scrollProgress) noexcept
            : _available(available), _document(document), _revision(revision), _textGeneration(textGeneration),
              _layoutGeneration(layoutGeneration), _previewUtf16Offset(previewUtf16Offset),
              _offsetWithinLine(offsetWithinLine), _scrollStart(scrollStart), _scrollEnd(scrollEnd),
              _scrollProgress(scrollProgress)
        {
        }
        bool IsAvailable() const noexcept
        {
            return _available;
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
        uint64_t LayoutGeneration() const noexcept
        {
            return _layoutGeneration;
        }
        uint64_t PreviewUtf16Offset() const noexcept
        {
            return _previewUtf16Offset;
        }
        double OffsetWithinLine() const noexcept
        {
            return _offsetWithinLine;
        }
        uint64_t ScrollStartUtf16Offset() const noexcept { return _scrollStart; }
        uint64_t ScrollEndUtf16Offset() const noexcept { return _scrollEnd; }
        double ScrollProgress() const noexcept { return _scrollProgress; }

      private:
        bool _available;
        guid _document;
        uint64_t _revision;
        uint64_t _textGeneration;
        uint64_t _layoutGeneration;
        uint64_t _previewUtf16Offset;
        double _offsetWithinLine;
        uint64_t _scrollStart;
        uint64_t _scrollEnd;
        double _scrollProgress;
    };
}
