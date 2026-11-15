// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

#pragma once
#include "notepads/markdown/ffi.h"

// Private fixed diagnostic ABI, separate from work ABI1 and Core ABI2.
// No handles/callbacks; synchronous, no retained pointer or worker. The caller
// owns an aligned readable 8-byte prefix; matching version/size promises a full
// exclusive writable result. Only status NOTEPADS_MARKDOWN_OK publishes the complete POD.
// Nonzero status preserves storage; do not consume success fields. This is not
// arbitrary-address validation. Rust contains panics; allocation abort remains.
enum { NOTEPADS_MARKDOWN_PLATFORM_VALIDATION_ABI_VERSION = 1 };

typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_platform_validation_result
{
    uint32_t abi_version;
    uint32_t struct_size;
    uint32_t rng_checked;
    int32_t payload_status;
    uint32_t payload_drops;
    uint32_t payload_live;
    uint32_t payload_control_owners;
    uint32_t reserved;
} notepads_markdown_platform_validation_result;

#ifdef __cplusplus
static_assert(sizeof(notepads_markdown_platform_validation_result) == 32);
static_assert(alignof(notepads_markdown_platform_validation_result) == 8);
static_assert(offsetof(notepads_markdown_platform_validation_result, abi_version) == 0);
static_assert(offsetof(notepads_markdown_platform_validation_result, struct_size) == 4);
static_assert(offsetof(notepads_markdown_platform_validation_result, rng_checked) == 8);
static_assert(offsetof(notepads_markdown_platform_validation_result, payload_status) == 12);
static_assert(offsetof(notepads_markdown_platform_validation_result, payload_drops) == 16);
static_assert(offsetof(notepads_markdown_platform_validation_result, payload_live) == 20);
static_assert(offsetof(notepads_markdown_platform_validation_result, payload_control_owners) == 24);
static_assert(offsetof(notepads_markdown_platform_validation_result, reserved) == 28);
extern "C"
{
#endif
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_platform_validation_run(notepads_markdown_platform_validation_result* result);
#ifdef __cplusplus
}
#endif
