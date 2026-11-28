// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "CoreControllerTests.h"

namespace Notepads::Markdown::Native::HighlightSnapshotTests
{
    // Invoke on UI. Three real G3/model-root checks build on a worker and read
    // immutable tokens after retiring the controller and original source flag.
    // Coroutine parameters are owned by value; no caller stack is borrowed.
    winrt::Windows::Foundation::IAsyncAction RunAsync(winrt::Windows::UI::Core::CoreDispatcher dispatcher,
                                                      std::shared_ptr<CoreControllerTests::Results> results,
                                                      std::shared_ptr<CoreValidation::Context> context);
}
