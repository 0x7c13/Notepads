// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "MarkdownSelection.g.h"

namespace winrt::Notepads::Markdown::implementation
{
    struct MarkdownSelection : MarkdownSelectionT<MarkdownSelection>
    {
        MarkdownSelection(bool available, guid document, uint64_t revision, uint64_t textGeneration,
                          uint64_t start, uint64_t length) noexcept
            : _available(available), _document(document), _revision(revision), _textGeneration(textGeneration),
              _start(start), _length(length)
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
        uint64_t Start() const noexcept
        {
            return _start;
        }
        uint64_t Length() const noexcept
        {
            return _length;
        }

      private:
        bool _available;
        guid _document;
        uint64_t _revision;
        uint64_t _textGeneration;
        uint64_t _start;
        uint64_t _length;
    };
}
