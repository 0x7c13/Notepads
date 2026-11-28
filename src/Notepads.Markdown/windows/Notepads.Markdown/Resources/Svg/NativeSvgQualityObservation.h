// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#if defined(NOTEPADS_MARKDOWN_SVG_QUALITY_TESTS)
#include "notepads/markdown/svg_job.h"
#include <array>
#include <cstddef>

// Private test-feature symbols. They do not extend the product job ABI.
struct alignas(8) MsSvgQualityTraceInfo
{
    notepads_markdown_svg_job_header header{};
    notepads_markdown_svg_job_identity identity{};
    uint64_t bytes{}, capacity{}, events{};
    uint32_t flags{}, traceError{};
};
static_assert(sizeof(MsSvgQualityTraceInfo) == 80);
extern "C"
{
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_job_quality_enable(notepads_markdown_svg_job *job, uint64_t capacity, notepads_markdown_svg_job_error *error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_job_quality_info(notepads_markdown_svg_job *job, MsSvgQualityTraceInfo *output, notepads_markdown_svg_job_error *error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_job_quality_copy(notepads_markdown_svg_job *job, uint8_t *output, uint64_t capacity,
                                             uint64_t *written, notepads_markdown_svg_job_error *error);
}

namespace Notepads::Markdown::Native
{
    inline constexpr std::size_t SvgQualityMaximumTraceBytes = 8 * 1024 * 1024;
    struct NativeSvgQualityObservation
    {
        // This POD is part of the paid heap task. Never copy it onto a stack.
        std::array<uint8_t, SvgQualityMaximumTraceBytes> bytes{};
        MsSvgQualityTraceInfo info{};
        uint64_t written{};
        int32_t installStatus{}, infoStatus{}, copyStatus{};
        bool installed{}, copied{};
    };
}
#endif
