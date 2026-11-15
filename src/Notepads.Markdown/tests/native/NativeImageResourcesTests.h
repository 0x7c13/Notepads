// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "CoreControllerTests.h"

namespace Notepads::Markdown::Native
{
    class ReadingMemory;
}

namespace Notepads::Markdown::Native::ImageResourcesTests
{
    constexpr uint32_t CheckCount() noexcept
    {
        return 16;
    }
    enum class ReadRetirementCase : uint32_t
    {
        Synchronous,
        Pending,
        CancelPending,
        CancelDuringCreate,
        CancelDuringRegistration,
        CancelTail,
        LateCancel,
        CloseFailure,
        ReadAndCloseFailure,
        CancelBeforeDecoderObserver,
        PartialRead,
        RegistrationFailure,
        PreCancelledRead,
        InvalidDecodeTail
    };
    struct ReadRetirementResult
    {
        bool passed{}, quarantined{};
        uint32_t closes{}, closesWhileActive{}, cancelCalls{}, activeReads{}, activeRequests{}, readCalls{};
    };
    // Background MTA only, using the caller's actual accepted ReadingMemory.
    // The last three cases deliberately retain their original unobservable IO
    // and IO permit forever. Run those only in an isolated fresh process.
    ReadRetirementResult RunReadRetirementCase(std::shared_ptr<ReadingMemory> const &memory,
                                               ReadRetirementCase scenario);
    // Actual product owners/prefixes/policy, callable from a genuine-model MTA
    // console harness as well as the packaged diagnostic suite.
    void RunRawEncodedFailureRegression(std::shared_ptr<ReadingMemory> const &memory);
    winrt::Windows::Foundation::IAsyncAction RunMetadataReAdmissionRegression(std::shared_ptr<ReadingMemory> memory);

    // Real G3/model-root funded decode, independent pixels and ownership checks.
    // Invoke on UI; all coroutine arguments are owned by value.
    winrt::Windows::Foundation::IAsyncAction RunAsync(winrt::Windows::UI::Core::CoreDispatcher dispatcher,
                                                      std::shared_ptr<CoreControllerTests::Results> results,
                                                      std::shared_ptr<CoreValidation::Context> context);
}
