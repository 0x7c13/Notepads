// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "MarkdownFindResult.g.h"

namespace winrt::Notepads::Markdown::implementation
{
    struct MarkdownFindResult : MarkdownFindResultT<MarkdownFindResult>
    {
        MarkdownFindResult(guid document, uint64_t revision, uint64_t textGeneration, bool found,
                           uint64_t start, uint64_t length) noexcept
            : _document(document), _revision(revision), _textGeneration(textGeneration), _found(found),
              _start(found ? start : 0), _length(found ? length : 0)
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
        bool Found() const noexcept
        {
            return _found;
        }
        uint64_t Start() const noexcept
        {
            return _start;
        }
        uint64_t Length() const noexcept
        {
            return _length;
        }

      private:
        guid _document;
        uint64_t _revision;
        uint64_t _textGeneration;
        bool _found;
        uint64_t _start;
        uint64_t _length;
    };
}
