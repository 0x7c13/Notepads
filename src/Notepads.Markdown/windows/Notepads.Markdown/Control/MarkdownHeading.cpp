// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Control/MarkdownHeading.h"
#include "MarkdownHeading.g.cpp"

namespace winrt::Notepads::Markdown::implementation
{
    MarkdownHeading::MarkdownHeading(guid document, uint64_t revision, uint64_t textGeneration,
                                     std::shared_ptr<::Notepads::Markdown::Native::ReadingOutline const> outline, size_t index)
        : _document(document), _revision(revision), _textGeneration(textGeneration), _index(index),
          _outline(std::move(outline))
    {
        if (!_outline || _outline->TextGeneration() != textGeneration || index >= _outline->Headings().size() ||
            index >= UINT32_MAX || _outline->Memory()->Identity().revision != revision)
            throw hresult_invalid_argument();
    }

    ::Notepads::Markdown::Native::ReadingHeadingDescriptor const &MarkdownHeading::Row() const noexcept
    {
        return _outline->Headings()[_index];
    }

    guid MarkdownHeading::DocumentId() const noexcept
    {
        return _document;
    }

    uint64_t MarkdownHeading::Revision() const noexcept
    {
        return _revision;
    }

    uint64_t MarkdownHeading::TextGeneration() const noexcept
    {
        return _textGeneration;
    }

    uint32_t MarkdownHeading::HeadingId() const noexcept
    {
        return static_cast<uint32_t>(_index + 1);
    }

    uint32_t MarkdownHeading::Level() const noexcept
    {
        return Row().level;
    }

    uint64_t MarkdownHeading::Start() const noexcept
    {
        return Row().text.start;
    }

    uint64_t MarkdownHeading::Length() const noexcept
    {
        auto const &text = Row().text;
        return static_cast<uint64_t>(text.end) - text.start;
    }

    hstring MarkdownHeading::Title() const
    {
        auto const &text = Row().title;
        return hstring{std::wstring_view{text.data(), text.size()}};
    }

    hstring MarkdownHeading::Anchor() const
    {
        auto const &text = Row().anchor;
        return hstring{std::wstring_view{text.data(), text.size()}};
    }
}
