// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Core/CoreController.h"
#include "Diagnostics/CoreValidationSuite.h"
#include <exception>

namespace Notepads::Markdown::Native::ControlConcurrencyTests
{
    winrt::Windows::Foundation::IAsyncAction RunAsync(
        winrt::Windows::UI::Core::CoreDispatcher dispatcher,
        std::shared_ptr<CoreValidation::Context> context, std::exception_ptr &failure);
}
