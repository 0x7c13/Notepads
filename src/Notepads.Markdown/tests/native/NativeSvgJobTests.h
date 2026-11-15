// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Reading/NativeReadingSnapshot.h"
#include "Resources/Svg/NativeSvgJob.h"

namespace Notepads::Markdown::Native::SvgJobTests
{
    struct Results
    {
        uint32_t checks{};
        bool basic{}, fractional{}, invalidXml{}, preCancelled{}, finishGate{}, mixedText{};
        uint64_t stackReserved{}, stackCommitted{}, stackGuard{}, stackHeadroom{}, nodeBytes{};
        uint32_t workerThread{}, reaperThread{};
        int32_t invalidXmlStatus{};
        uint64_t fragmentReads{}, fragmentReleases{};
        uint32_t latinPixels{}, chinesePixels{}, arabicPixels{};
    };

    struct FreeFailureResults
    {
        uint32_t checks{};
        NativeSvgJobTestFault mode{NativeSvgJobTestFault::None};
        bool notReady{}, quarantined{}, selfPinned{}, slotHeld{}, feesHeld{}, physicalFailure{}, threadSignaled{};
        int32_t firstWorkStatus{}, cleanupStatus{};
        uint64_t jobBytes{}, controlBytes{}, physicalFailures{}, rootFailures{};
        uint64_t failedOwnerNativeBytes{}, failedOwnerPreviewBytes{}, failedOwnerHeaderBytes{};
        uint64_t failedTotal{}, prefixTotal{}, failedOwnerRefs{}, failedOwnerJobBytes{}, rootLiveBytes{};
        uint32_t failedOwnerPhase{};
    };
    struct HandoffResults
    {
        uint32_t checks{}, destroyAttempts{};
        bool afterCommit{}, noReady{}, ownersTransferred{}, discardedBeforeCommit{}, retired{}, slotReleased{};
        uint64_t discardPhysicalFreed{};
        int32_t status{};
    };
    struct LeaseResults
    {
        uint32_t checks{}, destroyAttempts{};
        bool retainedAcrossExit{}, destroyBlockedWhileHeld{}, oldTaskRetired{}, slotReleased{}, successorReady{};
        bool threadSignaled{};
        bool tokenUnwrittenAtExit{}, tokenCancelledAfterLease{};
        int32_t cancellationStatus{};
    };
    enum class OverlapRetireMode : uint32_t
    {
        SourceEpoch = 0,
        ProviderEpoch = 1,
        CoordinatorRetire = 2
    };
    struct OverlapResults
    {
        uint32_t checks{};
        OverlapRetireMode mode{OverlapRetireMode::SourceEpoch};
        uint32_t firstWorkerThread{}, secondWorkerThread{};
        int32_t busyBeforeStatus{}, busyAfterStatus{};
        uint32_t activeBefore{}, activeAfterRetire{}, activeAfterFirst{}, activeAfterBoth{};
        bool originalSlots{}, overlappingWorkers{}, distinctWorkers{}, distinctJobs{};
        bool thirdBusyBefore{}, thirdBusyAfter{}, retirementKeptSlots{}, retirementKeptFees{};
        bool earlyFinishBlocked{}, firstReleaseIndependent{}, bothRetired{}, noReadyAfterRetire{};
        bool transientChargesReleased{}, taskMetadataReleased{}, successorReady{}, successorReleased{};
        uint64_t firstJobBytes{}, secondJobBytes{}, firstJobBytesAfter{}, secondJobBytesAfter{};
        uint64_t controlBytesBefore{}, controlBytesAfter{}, controlBytesAfterBoth{}, remainingTaskMetadataBytes{};
        uint64_t encodedBefore{}, encodedAfterFirst{}, encodedAfterBoth{}, decodedAfterBoth{};
        uint64_t sourceGenerationBefore{}, providerGenerationBefore{}, sourceGenerationRetired{},
            providerGenerationRetired{};
        uint64_t taskMetadataPhysicalFrees{}, taskMetadataRootReleases{}, successorPhysicalFrees{};
    };

    // Background MTA only. ReadingMemory must own a real CoreController model
    // pin; the borrowed-test-model retained fixture is explicitly rejected.
    // The caller keeps the current real model/source alive throughout the run.
    Results Run(std::shared_ptr<ReadingMemory> const &memory);
    // Exactly one fault per fresh process. Quarantine is permanent and the
    // test must not free/retry its original owner to make process release pass.
    FreeFailureResults RunUnpublishedFreeFailure(std::shared_ptr<ReadingMemory> const &memory,
                                                 NativeSvgJobTestFault mode);
    HandoffResults RunHandoffCancellation(std::shared_ptr<ReadingMemory> const &memory, bool afterCommit);
    LeaseResults RunLeaseAcrossExit(std::shared_ptr<ReadingMemory> const &memory);
    // Same original coordinator/policy/two physical tasks. Direct native entry
    // makes the real tail/weak-control release deterministic; the independent
    // encoded consumer exercises DecodeImageAsync/stream.Size separately.
    // CoordinatorRetire invokes the Native method, not a real XAML event.
    OverlapResults RunOverlapAndRetire(std::shared_ptr<ReadingMemory> const &memory, OverlapRetireMode mode);
}
