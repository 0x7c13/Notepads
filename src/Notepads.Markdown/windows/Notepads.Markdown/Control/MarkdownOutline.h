// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "MarkdownOutline.g.h"
#include "Reading/NativeReadingOutline.h"
#include "Core/ModuleLifetime.h"
#include <winrt/Windows.Foundation.Collections.h>
#include <winrt/Windows.UI.Core.h>

namespace winrt::Notepads::Markdown::implementation
{
    struct MarkdownOutline : MarkdownOutlineT<MarkdownOutline>
    {
        MarkdownOutline(guid document, uint64_t revision, uint64_t textGeneration,
                        std::shared_ptr<::Notepads::Markdown::Native::ReadingOutline const> outline,
                        Windows::UI::Core::CoreDispatcher dispatcher,
                        std::optional<::Notepads::Markdown::Native::ReadingSection> section = {});
        guid DocumentId() const noexcept;
        uint64_t Revision() const noexcept;
        uint64_t TextGeneration() const noexcept;
        uint32_t Version() const noexcept;
        uint32_t Count() const noexcept;
        Windows::Foundation::Collections::IVectorView<Markdown::MarkdownHeading> GetHeadings(uint32_t start, uint32_t count);

      private:
        ::Notepads::Markdown::Native::ModuleLifetime _module;
        guid _document;
        uint64_t _revision;
        uint64_t _textGeneration;
        Windows::UI::Core::CoreDispatcher _dispatcher;
        std::shared_ptr<::Notepads::Markdown::Native::ReadingOutline const> _outline;
        size_t _first{};
        uint32_t _count{};
    };
}
