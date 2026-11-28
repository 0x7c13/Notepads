// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "notepads/markdown/math.h"

namespace Notepads::Markdown::Native
{
    int32_t NOTEPADS_MARKDOWN_FFI_CALL ShapeChineseMathText(const uint16_t *text, uint32_t length, uint32_t style,
                                             void *sink, notepads_markdown_math_text_write write, notepads_markdown_math_text_metrics *metrics) noexcept;
}
