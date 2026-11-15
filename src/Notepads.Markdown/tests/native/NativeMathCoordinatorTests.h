// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Resources/Math/NativeMathCoordinator.h"
#include "Resources/Diagrams/NativeDiagramCoordinator.h"

namespace Notepads::Markdown::Native::MathCoordinatorTests
{
    struct Results
    {
        bool queuedMetadataBeforeThread{};
        bool controlResident128{};
        bool processResident256{};
        bool rejected129And257{};
        bool residentLastFreeReopensCapacity{};
        bool queuedRealFreeRestoresBaseline{};
        bool preparedThreadReallySignaled{};
        bool terminalRetiresJobKeepsResident{};
        bool terminalLastFreeRestoresBaseline{};
        bool prepareFailureDeduplicated{};
        bool expandedOrdinalsTracked{};
        bool oversizedOrdinalLimited{};
    };
    // Background diagnostic only, with no other math jobs/retained payloads.
    // Uses the caller's actual accepted Math model/object. Does not dispatch UI,
    // synthesize a profile, inject HeapFree damage, or claim UI/runtime coverage.
    Results Run(notepads_markdown_core_model const *model, NativeMathKey const &key);
    void CheckDiagramFailurePublication(notepads_markdown_core_model const *model, NativeDiagramKey key,
                                        notepads_markdown_diagram_font_capability const &fonts,
                                        notepads_markdown_diagram_style const &style);
}
