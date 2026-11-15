// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

#pragma once
#include <stddef.h>
#include <stdint.h>

#if defined(_MSC_VER)
#define NOTEPADS_MARKDOWN_FFI_ALIGN8 __declspec(align(8))
#define NOTEPADS_MARKDOWN_FFI_CALL __cdecl
#else
#define NOTEPADS_MARKDOWN_FFI_ALIGN8 __attribute__((aligned(8)))
#if defined(__i386__)
#define NOTEPADS_MARKDOWN_FFI_CALL __attribute__((cdecl))
#else
#define NOTEPADS_MARKDOWN_FFI_CALL
#endif
#endif

enum notepads_markdown_status
{
    NOTEPADS_MARKDOWN_OK = 0,
    NOTEPADS_MARKDOWN_INVALID_ARGUMENT = 1,
    NOTEPADS_MARKDOWN_ABI_MISMATCH = 2,
    NOTEPADS_MARKDOWN_INVALID_UTF8 = 3,
    NOTEPADS_MARKDOWN_CANCELED = 4,
    NOTEPADS_MARKDOWN_INVALID_STATE = 5,
    NOTEPADS_MARKDOWN_PANIC = 6,
    NOTEPADS_MARKDOWN_OUT_OF_MEMORY = 7
};
