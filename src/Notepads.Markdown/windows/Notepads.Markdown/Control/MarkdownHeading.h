// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "MarkdownHeading.g.h"
#include "Reading/NativeReadingOutline.h"
#include "Core/ModuleLifetime.h"

namespace winrt::Notepads::Markdown::implementation
{
    struct MarkdownHeading : MarkdownHeadingT<MarkdownHeading>
    {
        MarkdownHeading(guid document, uint64_t revision, uint64_t textGeneration,
                        std::shared_ptr<::Notepads::Markdown::Native::ReadingOutline const> outline, size_t index);
        guid DocumentId() const noexcept;
        uint64_t Revision() const noexcept;
        uint64_t TextGeneration() const noexcept;
        uint32_t HeadingId() const noexcept;
        uint32_t Level() const noexcept;
        uint64_t Start() const noexcept;
        uint64_t Length() const noexcept;
        hstring Title() const;
        hstring Anchor() const;

      private:
        // Keep the component loaded through the last immutable row/payload free.
        ::Notepads::Markdown::Native::ModuleLifetime _module;
        guid _document;
        uint64_t _revision;
        uint64_t _textGeneration;
        size_t _index;
        std::shared_ptr<::Notepads::Markdown::Native::ReadingOutline const> _outline;
        ::Notepads::Markdown::Native::ReadingHeadingDescriptor const &Row() const noexcept;
    };
}
