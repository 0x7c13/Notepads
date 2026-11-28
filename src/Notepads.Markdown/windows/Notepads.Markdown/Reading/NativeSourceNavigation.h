// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Reading/NativeReadingSnapshot.h"

namespace Notepads::Markdown::Native
{
    struct ReadingSourcePosition
    {
        uint64_t sourceByteOffset{};
        uint64_t previewUtf16Offset{};
        uint32_t correspondence{};
        uint32_t effects{};
    };
    struct ReadingSourceSpan
    {
        ReadingSourcePosition start;
        ReadingSourcePosition end;
        bool discontinuous{};
    };

    // A chosen boundary with affinity, never an envelope or a range bijection.
    ReadingSourcePosition MapReadingPosition(MainTextSnapshot const &main, notepads_markdown_core_cancel *cancel,
                                            bool fromSource, uint64_t offset, bool after);
    ReadingSourceSpan MapReadingSpan(MainTextSnapshot const &main, notepads_markdown_core_cancel *cancel,
                                    bool fromSource, uint64_t start, uint64_t end, bool after);
}
