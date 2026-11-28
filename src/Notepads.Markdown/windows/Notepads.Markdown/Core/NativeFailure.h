// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "notepads/markdown/core.h"
#include <winrt/base.h>

namespace Notepads::Markdown::Native
{
    inline winrt::hresult NativeStatusHresult(int32_t status) noexcept
    {
        if (status < 0)
            return winrt::hresult{status};
        switch (status)
        {
        case NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT:
        case NOTEPADS_MARKDOWN_CORE_INVALID_UTF8:
        case NOTEPADS_MARKDOWN_CORE_INVALID_UTF16:
        case NOTEPADS_MARKDOWN_CORE_INVALID_BOUNDARY:
        case NOTEPADS_MARKDOWN_CORE_INVALID_RANGE:
        case NOTEPADS_MARKDOWN_CORE_INVALID_EDITS:
        case NOTEPADS_MARKDOWN_CORE_WRONG_DOCUMENT:
        case NOTEPADS_MARKDOWN_CORE_WRONG_REVISION:
        case NOTEPADS_MARKDOWN_CORE_WRONG_GENERATION:
        case NOTEPADS_MARKDOWN_CORE_UNKNOWN_ID:
        case NOTEPADS_MARKDOWN_CORE_OUTSIDE_SCOPE:
            return winrt::hresult{E_INVALIDARG};
        case NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY:
            return winrt::hresult{E_OUTOFMEMORY};
        case NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT:
            return winrt::hresult{HRESULT_FROM_WIN32(ERROR_NOT_ENOUGH_QUOTA)};
        case NOTEPADS_MARKDOWN_CORE_CANCELLED:
            return winrt::hresult{HRESULT_FROM_WIN32(ERROR_CANCELLED)};
        case NOTEPADS_MARKDOWN_CORE_DEADLINE:
            return winrt::hresult{HRESULT_FROM_WIN32(ERROR_TIMEOUT)};
        case NOTEPADS_MARKDOWN_CORE_ARITHMETIC_OVERFLOW:
            return winrt::hresult{HRESULT_FROM_WIN32(ERROR_ARITHMETIC_OVERFLOW)};
        case NOTEPADS_MARKDOWN_CORE_UNSUPPORTED:
            return winrt::hresult{E_NOTIMPL};
        default:
            return winrt::hresult{E_FAIL};
        }
    }

    // Generated WinRT/UIA boundaries recognize hresult_error. Keep the original
    // status for native worker arbitration; never throw a bare status record
    // through winrt::to_hresult(), whose conversion function is noexcept.
    struct NativeFailure : winrt::hresult_error
    {
        int32_t status;
        explicit NativeFailure(int32_t value) noexcept : winrt::hresult_error(NativeStatusHresult(value)), status(value)
        {
        }
    };
}
