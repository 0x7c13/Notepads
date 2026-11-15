// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Reading/NativeReadingSnapshot.h"

namespace Notepads::Markdown::Native::MathRendererTests
{
    struct Results
    {
        bool pendingLabel{};
        bool typedLabels{};
        bool sourceAndCopy{};
        bool nativeReady{};
        bool sameFactoryDraw{};
        bool widthLimited{};
        bool arenaLimited{};
        bool immutableFrame{};
        bool arenaRecovered{};
        bool staleArtifactRejected{};
        bool staleSourceRejected{};
        bool nativeOwnersDropped{};
        bool baselineInkWithinInlineBox{};
        bool baselineInkMatchesTextBaseline{};
        uint64_t baselineInkPixels{};
        double baselineInkTop{}, baselineInkBottom{};
        double baselineExpectedTop{}, baselineExpectedBottom{};
        double baselineTextY{}, baselineInlineTop{}, baselineInlineBottom{};
    };

    // Background-only. Borrows the real existing Math fixture's ReadingMemory
    // (`pre $a\r\nb$ post`). Creates a real builtin-profile worker/artifact and
    // a private MULTI factory/WARP target. Does not activate a window or use UI.
    Results Run(std::shared_ptr<ReadingMemory> const &memory);
}
