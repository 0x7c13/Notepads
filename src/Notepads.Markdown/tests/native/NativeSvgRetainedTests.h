// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "notepads/markdown/core.h"
#include <cstdint>

namespace Notepads::Markdown::Native::SvgRetainedTests
{
    struct Results
    {
        uint32_t checks{};
        bool encodedShared{};
        bool failedFreePreserved{};
        bool actualRetryReleased{};
        bool freeOrderObserved{};
        bool pixelMetadataFunded{};
        bool weakMetadataRetained{};
        bool allocationFailureRetired{};
        bool sizeFailureRetired{};
        bool combinedFailureRetry{};
        bool cancelledSourceCleanup{};
        bool ordinaryRasterPreserved{};
        bool ordinaryEncodedChargePinned{};
        bool rawAdoptionRefusalPreserved{};
        bool failedRawHandoffRecovered{};
    };

    // MTA/background only. The host owns these real model/cancel handles through
    // the complete call. The explicit borrowed-model fixture does not fabricate
    // a CoreModelPin/controller ledger and conveys no UI/AppContainer qualification.
    Results Run(notepads_markdown_core_model *model, notepads_markdown_core_cancel *sourceFlag);
}
