// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Resources/Math/NativeMathArtifact.h"

namespace Notepads::Markdown::Native::MathArtifactTests
{
    struct Results
    {
        bool geometryBuilt{};
        bool numericOracle{};
        bool retainedBounded{};
        bool heapGrowthRejected{};
        bool arenaExhausted{};
        bool arenaReused{};
        bool lineLimitTyped{};
        bool lastWeakFreed{};
        bool nativeOwnersDropped{};
        uint64_t commands{};
        uint64_t primitives{};
        uint64_t actualRetained{};
        uint64_t liveWithDrawing{};
        uint64_t liveWithWeak{};
        uint64_t liveAfterLastWeak{};
        double advance{}, ascent{}, descent{};
    };

    // Background-only, after the real worker thread handle is signaled. The job
    // is an actual successful builtin-profile result for inline `$a b$` (the
    // existing fixture's CRLF whitespace is equivalent). Borrow job/control;
    // the caller/reaper keeps both alive and serializes this completion.
    // Tests contain no profile override, synthetic grant or parser bypass.
    // After this returns, the caller must finish reaping and release its own job
    // reference, then verify the real control/process baseline is restored.
    // Its borrowed job and still-live worker node intentionally prevent that
    // final retained-grant release inside this helper.
    Results RunComputed(notepads_markdown_math_job *job, notepads_markdown_math_control *control);
}
