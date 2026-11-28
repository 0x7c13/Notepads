// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "NativeMathMemoryTests.h"

namespace Notepads::Markdown::Native::MathMemoryTests
{
    namespace
    {
        void Require(bool value, wchar_t const *detail)
        {
            if (!value)
                throw winrt::hresult_error(E_FAIL, detail);
        }
        notepads_markdown_core_error Error() noexcept
        {
            return {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(notepads_markdown_core_error)};
        }
        notepads_markdown_math_usage Usage(notepads_markdown_math_control *control)
        {
            notepads_markdown_math_usage value{};
            value.header = {NOTEPADS_MARKDOWN_MATH_ABI_VERSION, sizeof(value)};
            auto error = Error();
            Require(notepads_markdown_math_control_usage(control, &value, &error) == NOTEPADS_MARKDOWN_CORE_OK, L"real allocator control usage");
            return value;
        }
        bool Same(notepads_markdown_math_usage_row const &left, notepads_markdown_math_usage_row const &right) noexcept
        {
            return left.bytes == right.bytes && left.retained_bytes == right.retained_bytes &&
                   left.metadata_bytes == right.metadata_bytes && left.jobs == right.jobs &&
                   left.residents == right.residents;
        }
    }

    Results RunUnavailable(notepads_markdown_math_job *job, notepads_markdown_math_control *control)
    {
        Results result{};
        Require(job && control && notepads_markdown_math_job_state(job) == NOTEPADS_MARKDOWN_MATH_COMPUTED,
                L"borrowed unavailable job follows actual thread termination");
        auto before = Usage(control);
        Require(before.control.jobs == 1 && before.control.bytes >= 64ull * 1024 * 1024,
                L"real full job remains funded before unavailable native construction");
        auto error = Error();
        MathMemoryHandle memory;
        auto status = NativeMathMemory::TryCreate(job, &memory, &error);
        Require(status == NOTEPADS_MARKDOWN_MATH_STACK_UNAVAILABLE && error.status == status && !memory,
                L"StackUnavailable cannot create a native allocation authority or manufacture Ready");
        auto after = Usage(control);
        Require(Same(before.process, after.process) && Same(before.control, after.control),
                L"unavailable construction leaves existing full source/process/control ownership unchanged");
        result.unavailablePreservesGrant = true;
        status = NativeMathMemory::TryCreate(nullptr, &memory, &error);
        Require(status == NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT && !memory, L"null allocator owner has no fallback funding path");
        result.nullOwnerRejected = true;

        try
        {
            auto value = std::allocate_shared<uint64_t>(MathAllocator<uint64_t>{}, uint64_t{9});
            (void)value;
        }
        catch (MathMemoryFailure const &failure)
        {
            result.emptySharedAllocatorRejected = failure.status == NOTEPADS_MARKDOWN_CORE_INVALID_STATE;
        }
        Require(result.emptySharedAllocatorRejected,
                L"shared control block uses the supplied real allocation authority");
        try
        {
            MathVector<uint64_t> values;
            values.push_back(11);
        }
        catch (MathMemoryFailure const &failure)
        {
            result.emptyVectorAllocatorRejected = failure.status == NOTEPADS_MARKDOWN_CORE_INVALID_STATE;
        }
        Require(result.emptyVectorAllocatorRejected,
                L"vector growth cannot fall back to source-only or global funding");
        return result;
    }
}
