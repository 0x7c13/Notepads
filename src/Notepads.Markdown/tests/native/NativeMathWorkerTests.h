// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Resources/Math/NativeMathWorker.h"

namespace Notepads::Markdown::Native::MathWorkerTests
{
    struct Results
    {
        NativeMathStackObservation stack;
        bool insufficientHeadroomRejected{};
        bool tailHeldFullGrant{};
        bool secondControlJobRejected{};
        bool completionAfterSignaled{};
        bool cancellationRetired{};
        bool injectedCreateFailureRetired{};
        bool baselineRestored{};
        bool computedArtifactRetired{};
        uint64_t actualRetained{}, commands{}, primitives{};
        double advance{};
    };

    // Background thread only; throws on a failed assertion. model/object are a
    // real installed G3+Math model and 1-based math descriptor object ID.
    // No old compatibility/oracle suite is rerun. Actual OS creation/wait/OOM
    // failure and ARM64/min17763 runtime are explicitly outside these checks.
    Results Run(notepads_markdown_core_model const *model, uint32_t object);
}
