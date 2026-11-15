// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "CoreControllerTests.h"

namespace Notepads::Markdown::Native
{
    class ReadingMemory;
    struct ImageCoordinator;
    struct ImageLoadTask;
}

namespace Notepads::Markdown::Native::ImagePayloadTests
{
    struct Results
    {
        uint32_t checks{};
        bool scopedAllocation{}, constructorFailure{}, immutablePixels{}, immutableEncoded{};
    };

    // Synchronous background checks using the caller's genuine accepted root.
    // The caller still owns its model; this entry does not qualify last-model release.
    Results Run(std::shared_ptr<ReadingMemory> const &memory);
    void RunDirectSvgFreezeRegression(std::shared_ptr<ReadingMemory> const &memory,
                                     std::shared_ptr<ImageLoadTask> *retiredTask = nullptr);
    // Returns two finished paid task slots, including canonical SVG metadata,
    // so the caller can verify their strong/weak owners outlive the real model.
    std::shared_ptr<ImageCoordinator> RunTaskRetirementRegression(std::shared_ptr<ReadingMemory> const &memory);

    // Actual UI dispatcher/controller fixture, then background teardown while
    // only immutable payloads/weak metadata remain. No fake dispatcher/pin.
    winrt::Windows::Foundation::IAsyncAction RunAsync(
        winrt::Windows::UI::Core::CoreDispatcher dispatcher,
        std::shared_ptr<CoreControllerTests::Results> results,
        std::shared_ptr<CoreValidation::Context> context);
}
