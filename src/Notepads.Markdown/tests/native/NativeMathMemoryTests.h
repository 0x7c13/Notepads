// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Resources/Math/NativeMathMemory.h"

namespace Notepads::Markdown::Native::MathMemoryTests
{
    struct Results
    {
        bool unavailablePreservesGrant{};
        bool emptySharedAllocatorRejected{};
        bool emptyVectorAllocatorRejected{};
        bool nullOwnerRejected{};
    };

    // Caller holds a real StackUnavailable job after the actual Windows thread
    // is signaled, and its matching long-lived control. Both are borrowed.
    // This negative helper introduces no profile or renderer bypass. Positive
    // allocation/arena/finalize tests use the actual qualified worker separately.
    Results RunUnavailable(notepads_markdown_math_job *job, notepads_markdown_math_control *control);
}
