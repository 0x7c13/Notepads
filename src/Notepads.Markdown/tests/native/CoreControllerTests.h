// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Core/CoreController.h"
#include "Diagnostics/CoreValidationSuite.h"

namespace Notepads::Markdown::Native::CoreControllerTests
{
    struct Results
    {
        std::vector<CoreValidation::CheckResult> checks;
        std::vector<std::wstring> untested;
        CoreDrainResult lastDrain;
        CoreLifetimeSnapshot lastLifetime;
    };

    // Invoke on the actual application's UI thread before its first background
    // switch. The coroutine frame exists before any controller request starts.
    // Observation waits and Drain occur off UI. Deliberate UI correctness gates
    // use prebuilt background coordinators; UI code never joins a worker.
    // The two drain races bound their gate holds and cannot pass after rescue.
    // No fake dispatcher is supported.
    // Opt-in: six ticket checks plus one actual DWrite viewport/padding control.
    // The legacy suite and its existing oracle stay unchanged.
    winrt::Windows::Foundation::IAsyncAction RunPreviewTicketsAsync(
        winrt::Windows::UI::Core::CoreDispatcher const &dispatcher, std::shared_ptr<Results> const &results,
        std::shared_ptr<CoreValidation::Context> const &context);

    winrt::Windows::Foundation::IAsyncAction RunAsync(winrt::Windows::UI::Core::CoreDispatcher const &dispatcher,
                                                      std::shared_ptr<Results> const &results,
                                                      std::shared_ptr<CoreValidation::Context> const &context);
}
