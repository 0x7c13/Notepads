// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Reading/NativeReadingSnapshot.h"

namespace Notepads::Markdown::Native::DiagramRendererTests
{
    struct Results
    {
        bool pendingLabel{}, typedLabels{}, sourceAndCopy{}, atomicGeometry{}, nativeReady{};
        bool exactInline{}, sameContextReuse{}, nativePixels{}, streamRetainsOwner{};
        bool narrowFit{}, staleArtifactRejected{}, immutableFrame{}, nativeOwnersDropped{};
        uint64_t inkPixels{};
    };

    // Background-only dedicated accepted fixture containing one Mermaid fence.
    // Requires the real builtin compiled profile; an unavailable profile is a
    // failure of Ready coverage and never counted as a substitute pass.
    Results Run(std::shared_ptr<ReadingMemory> const &memory);
}
