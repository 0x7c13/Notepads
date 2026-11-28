// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Control/MarkdownOutline.h"
#include "Control/MarkdownHeading.h"
#include "MarkdownOutline.g.cpp"
#include <algorithm>
#include <atomic>

namespace winrt::Notepads::Markdown::implementation
{
    namespace
    {
        using Heading = Markdown::MarkdownHeading;
        using HeadingView = Windows::Foundation::Collections::IVectorView<Heading>;
        using HeadingIterator = Windows::Foundation::Collections::IIterator<Heading>;
        using HeadingIterable = Windows::Foundation::Collections::IIterable<Heading>;
        using PaidHeadings = ::Notepads::Markdown::Native::ReadingVector<Heading>;

        struct HeadingPage;

        struct PageIterator : implements<PageIterator, HeadingIterator>
        {
            explicit PageIterator(com_ptr<HeadingPage> page) noexcept;
            Heading Current() const;
            bool HasCurrent() const noexcept;
            bool MoveNext() noexcept;
            uint32_t GetMany(array_view<Heading> values);

          private:
            ::Notepads::Markdown::Native::ModuleLifetime _module;
            com_ptr<HeadingPage> _page;
            std::atomic<uint32_t> _index{};
        };

        // The component owns both the immutable page and every heading wrapper.
        // The only variable page allocation is this same-root ReadingVector.
        struct HeadingPage : implements<HeadingPage, HeadingView, HeadingIterable>
        {
            explicit HeadingPage(PaidHeadings values) noexcept : _values(std::move(values))
            {
            }

            uint32_t Size() const noexcept
            {
                return static_cast<uint32_t>(_values.size());
            }

            Heading GetAt(uint32_t index) const
            {
                if (index >= _values.size())
                    throw hresult_out_of_bounds();
                return _values[index];
            }

            bool IndexOf(Heading const &value, uint32_t &index) noexcept
            {
                // COM identity comparison queries the caller's object. Its QI
                // or temporary identity Release can reenter and release the
                // caller's final page reference before this loop finishes.
                auto lifetime = get_strong();
                index = 0;
                for (uint32_t offset = 0; offset < Size(); ++offset)
                    if (_values[offset] == value)
                    {
                        index = offset;
                        return true;
                    }
                return false;
            }

            uint32_t GetMany(uint32_t start, array_view<Heading> values) const
            {
                if (start > Size())
                    throw hresult_out_of_bounds();
                auto count = std::min<uint32_t>(Size() - start, values.size());
                for (uint32_t index = 0; index < count; ++index)
                    values[index] = _values[start + index];
                return count;
            }

            HeadingIterator First()
            {
                return make<PageIterator>(get_strong());
            }

          private:
            ::Notepads::Markdown::Native::ModuleLifetime _module;
            PaidHeadings _values;
        };

        PageIterator::PageIterator(com_ptr<HeadingPage> page) noexcept : _page(std::move(page))
        {
        }

        Heading PageIterator::Current() const
        {
            return _page->GetAt(_index.load(std::memory_order_acquire));
        }

        bool PageIterator::HasCurrent() const noexcept
        {
            return _index.load(std::memory_order_acquire) < _page->Size();
        }

        bool PageIterator::MoveNext() noexcept
        {
            auto index = _index.load(std::memory_order_acquire);
            while (index < _page->Size())
            {
                if (_index.compare_exchange_weak(index, index + 1, std::memory_order_acq_rel))
                    return index + 1 < _page->Size();
            }
            return false;
        }

        uint32_t PageIterator::GetMany(array_view<Heading> values)
        {
            auto index = _index.load(std::memory_order_acquire);
            for (;;)
            {
                auto count = std::min<uint32_t>(_page->Size() - index, values.size());
                if (!count)
                    return 0;
                if (_index.compare_exchange_weak(index, index + count, std::memory_order_acq_rel))
                    return _page->GetMany(index, values);
            }
        }
    }

    MarkdownOutline::MarkdownOutline(guid document, uint64_t revision, uint64_t textGeneration,
                                     std::shared_ptr<::Notepads::Markdown::Native::ReadingOutline const> outline,
                                     Windows::UI::Core::CoreDispatcher dispatcher,
                                     std::optional<::Notepads::Markdown::Native::ReadingSection> section)
        : _document(document), _revision(revision), _textGeneration(textGeneration), _dispatcher(std::move(dispatcher)),
          _outline(std::move(outline))
    {
        if (!_outline || !_dispatcher || _outline->TextGeneration() != textGeneration ||
            _outline->Memory()->Identity().revision != revision || _outline->Headings().size() > UINT32_MAX)
            throw hresult_invalid_argument();
        _first = section ? section->firstHeading : 0;
        _count = static_cast<uint32_t>((section ? section->endHeading : _outline->Headings().size()) - _first);
    }

    guid MarkdownOutline::DocumentId() const noexcept
    {
        return _document;
    }

    uint64_t MarkdownOutline::Revision() const noexcept
    {
        return _revision;
    }

    uint64_t MarkdownOutline::TextGeneration() const noexcept
    {
        return _textGeneration;
    }

    uint32_t MarkdownOutline::Version() const noexcept
    {
        return 1;
    }

    uint32_t MarkdownOutline::Count() const noexcept
    {
        return _count;
    }

    Windows::Foundation::Collections::IVectorView<Markdown::MarkdownHeading> MarkdownOutline::GetHeadings(uint32_t start,
                                                                                                  uint32_t count)
    {
        if (!_dispatcher.HasThreadAccess())
            throw hresult_wrong_thread();
        if (count == 0 || count > 256 || start > Count())
            throw hresult_invalid_argument();
        auto available = std::min<uint32_t>(count, Count() - start);
        auto const &memory = _outline->Memory();
        PaidHeadings values{::Notepads::Markdown::Native::ReadingAllocator<Heading>{memory}};
        values.reserve(available);
        for (uint32_t index = 0; index < available; ++index)
            values.push_back(make<implementation::MarkdownHeading>(_document, _revision, _textGeneration, _outline,
                                                                    _first + start + index));
        return make<HeadingPage>(std::move(values));
    }
}
