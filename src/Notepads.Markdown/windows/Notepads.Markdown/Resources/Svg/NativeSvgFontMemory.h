// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include <windows.h>
#include "notepads/markdown/svg_resources.h"
#include <cstddef>
#include <cstdint>
#include <new>
#include <vector>

// Private Windows allocation mechanism shared by the original font session
// and its synchronous generated-font coverage work. There is one ledger.
namespace Notepads::Markdown::Native
{
    namespace SvgFontMemory
    {
        constexpr HRESULT Closed = RO_E_CLOSED;
        constexpr HRESULT Quota = HRESULT_FROM_WIN32(ERROR_NOT_ENOUGH_QUOTA);
        constexpr HRESULT Timeout = HRESULT_FROM_WIN32(ERROR_TIMEOUT);
        constexpr uint32_t Font = 1, Geometry = 3, Scratch = 6;
        inline void Check(HRESULT hr)
        {
            if (hr != S_OK)
                throw hr < 0 ? hr : E_UNEXPECTED;
        }
        inline uint64_t Bytes(uint64_t count, uint64_t stride)
        {
            if (stride && count > UINT64_MAX / stride)
                throw E_INVALIDARG;
            return count * stride;
        }

        constexpr uint64_t FontLimit = 64ull * 1024 * 1024;
        inline notepads_markdown_core_error Error() noexcept
        {
            notepads_markdown_core_error error{};
            error.header = {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(error)};
            return error;
        }
        inline HRESULT Status(notepads_markdown_svg_resources *resources, int32_t status) noexcept
        {
            if (status == NOTEPADS_MARKDOWN_CORE_OK)
                return S_OK;
            notepads_markdown_svg_fault fault{};
            auto error = Error();
            if (resources && notepads_markdown_svg_resources_read_fault(resources, &fault, &error) == NOTEPADS_MARKDOWN_CORE_OK && fault.kind == 1 &&
                fault.native_hr_present && fault.native_hr != S_OK)
                return fault.native_hr < 0 ? fault.native_hr : E_UNEXPECTED;
            switch (status)
            {
            case NOTEPADS_MARKDOWN_CORE_CANCELLED:
            case NOTEPADS_MARKDOWN_CORE_STALE_RESULT:
                return HRESULT_FROM_WIN32(ERROR_CANCELLED);
            case NOTEPADS_MARKDOWN_CORE_DEADLINE:
                return Timeout;
            case NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY:
                return E_OUTOFMEMORY;
            case NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT:
                return Quota;
            case NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT:
                return E_INVALIDARG;
            case NOTEPADS_MARKDOWN_CORE_INVALID_STATE:
                return Closed;
            default:
                return E_UNEXPECTED;
            }
        }
        inline HRESULT RecordFault(notepads_markdown_svg_resources *resources, HRESULT hr) noexcept
        {
            if (hr == S_OK)
                return S_OK;
            notepads_markdown_svg_fault fault{1, 0, hr, 1};
            auto error = Error();
            (void)notepads_markdown_svg_resources_record_fault(resources, &fault, &error);
            return hr;
        }
    }
    struct SvgFontServices
    {
        notepads_markdown_svg_resources *context{};
        static HRESULT checkpoint(notepads_markdown_svg_resources *resources, uint64_t work) noexcept
        {
            auto error = SvgFontMemory::Error();
            return SvgFontMemory::Status(resources, notepads_markdown_svg_resources_checkpoint(resources, work, &error));
        }
        static HRESULT reserve(notepads_markdown_svg_resources *resources, uint32_t kind, uint64_t bytes,
                               notepads_markdown_svg_work_token *output) noexcept
        {
            auto error = SvgFontMemory::Error();
            return SvgFontMemory::Status(resources, notepads_markdown_svg_resources_reserve(resources, kind, bytes, output, &error));
        }
        static HRESULT admit(notepads_markdown_svg_resources *resources, notepads_markdown_svg_work_token const &token, uint64_t bytes) noexcept
        {
            auto error = SvgFontMemory::Error();
            return SvgFontMemory::Status(resources, notepads_markdown_svg_resources_admit(resources, &token, bytes, &error));
        }
        static HRESULT release(notepads_markdown_svg_resources *resources, notepads_markdown_svg_work_token const &token) noexcept
        {
            auto error = SvgFontMemory::Error();
            return SvgFontMemory::Status(resources, notepads_markdown_svg_resources_release_after_free(resources, &token, &error));
        }
    };

    struct SvgFontAllocationContext
    {
        SvgFontServices services;
        uint64_t live[7]{};
        uint64_t failed_releases{}, failed_bytes{};
        HRESULT cleanup_error{S_OK};
    };
    struct alignas(std::max_align_t) SvgFontAllocationHeader
    {
        notepads_markdown_svg_work_token lease;
        uint64_t bytes;
    };
    template <class T> struct SvgFontAllocator
    {
        using value_type = T;
        SvgFontAllocationContext *context{};
        uint32_t category{SvgFontMemory::Scratch};
        SvgFontAllocator() noexcept = default;
        SvgFontAllocator(SvgFontAllocationContext *c, uint32_t k = SvgFontMemory::Scratch) noexcept : context(c), category(k)
        {
        }
        template <class U>
        SvgFontAllocator(SvgFontAllocator<U> const &other) noexcept : context(other.context), category(other.category)
        {
        }
        T *allocate(size_t count)
        {
            static_assert(alignof(T) <= alignof(SvgFontAllocationHeader));
            uint64_t n = SvgFontMemory::Bytes(count, sizeof(T));
            if (n > SIZE_MAX - sizeof(SvgFontAllocationHeader))
                throw E_OUTOFMEMORY;
            n += sizeof(SvgFontAllocationHeader);
            notepads_markdown_svg_work_token lease{};
            SvgFontMemory::Check(context->services.reserve(context->services.context, category, n, &lease));
            void *raw = ::operator new(static_cast<size_t>(n), std::nothrow);
            if (!raw)
            {
                SvgFontMemory::RecordFault(context->services.context, E_OUTOFMEMORY);
                HRESULT released = context->services.release(context->services.context, lease);
                if (released != S_OK)
                {
                    ++context->failed_releases;
                    context->failed_bytes += n;
                    if (context->cleanup_error == S_OK)
                        context->cleanup_error = released;
                }
                throw E_OUTOFMEMORY;
            }
            HRESULT admitted = context->services.admit(context->services.context, lease, n);
            if (admitted != S_OK)
            {
                SvgFontMemory::RecordFault(context->services.context, admitted);
                ::operator delete(raw);
                HRESULT released = context->services.release(context->services.context, lease);
                if (released != S_OK)
                {
                    ++context->failed_releases;
                    context->failed_bytes += n;
                    if (context->cleanup_error == S_OK)
                        context->cleanup_error = released;
                }
                throw admitted;
            }
            auto header = new (raw) SvgFontAllocationHeader{lease, n};
            context->live[category] += n;
            return reinterpret_cast<T *>(header + 1);
        }
        void deallocate(T *value, size_t) noexcept
        {
            if (!value)
                return;
            auto header = reinterpret_cast<SvgFontAllocationHeader *>(value) - 1;
            auto lease = header->lease;
            uint64_t bytes = header->bytes;
            ::operator delete(header);
            context->live[category] -= bytes;
            HRESULT released = context->services.release(context->services.context, lease);
            if (released != S_OK)
            {
                ++context->failed_releases;
                context->failed_bytes += bytes;
                if (context->cleanup_error == S_OK)
                    context->cleanup_error = released;
                SvgFontMemory::RecordFault(context->services.context, released);
            }
        }
        template <class U> bool operator==(SvgFontAllocator<U> const &b) const noexcept
        {
            return context == b.context && category == b.category;
        }
        template <class U> bool operator!=(SvgFontAllocator<U> const &b) const noexcept
        {
            return !(*this == b);
        }
    };
    template <class T> using SvgFontVector = std::vector<T, SvgFontAllocator<T>>;
    namespace SvgFontMemory
    {
        template <class T> inline void Free(SvgFontVector<T> &value)
        {
            SvgFontVector<T> empty(value.get_allocator());
            value.swap(empty);
        }
    }
}
