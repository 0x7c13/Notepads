// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "CoreControllerTests.h"

namespace Notepads::Markdown::Native::ReadingSnapshotTests
{
    // Original G3/model-root cases plus actual Math and Diagram artifacts.
    // UI entry; reading and Drain run off UI.
    uint32_t CheckCount() noexcept;
    winrt::Windows::Foundation::IAsyncAction RunAsync(winrt::Windows::UI::Core::CoreDispatcher const &dispatcher,
                                                      std::shared_ptr<CoreControllerTests::Results> const &results,
                                                      std::shared_ptr<CoreValidation::Context> const &context);
}
