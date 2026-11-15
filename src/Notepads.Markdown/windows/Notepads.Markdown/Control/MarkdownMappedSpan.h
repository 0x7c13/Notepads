// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "MarkdownMappedSpan.g.h"

namespace winrt::Notepads::Markdown::implementation
{
    struct MarkdownMappedSpan : MarkdownMappedSpanT<MarkdownMappedSpan>
    {
        MarkdownMappedSpan(Markdown::MarkdownMappedPosition const &start, Markdown::MarkdownMappedPosition const &end, bool discontinuous)
            : _start(start), _end(end), _discontinuous(discontinuous)
        {
        }
        Markdown::MarkdownMappedPosition Start() const noexcept { return _start; }
        Markdown::MarkdownMappedPosition End() const noexcept { return _end; }
        bool IsDiscontinuous() const noexcept { return _discontinuous; }

      private:
        Markdown::MarkdownMappedPosition _start{nullptr};
        Markdown::MarkdownMappedPosition _end{nullptr};
        bool _discontinuous{};
    };
}
