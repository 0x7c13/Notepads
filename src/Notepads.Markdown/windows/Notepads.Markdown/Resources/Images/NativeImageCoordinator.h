// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Resources/Images/NativeImagePresentation.h"
#include "Resources/Svg/NativeSvgJob.h"
#include "Resources/Svg/NativeSvgQualityObservation.h"
#include <chrono>
#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.Storage.Streams.h>

namespace Notepads::Markdown::Native
{
    struct SemanticDescriptors;
    struct ImageLoadTask : std::enable_shared_from_this<ImageLoadTask>
    {
        uint32_t pictureSource{};
        bool pictureSourceSuperseded{};
      private:
        struct ConstructionKey
        {
        };

      public:
        explicit ImageLoadTask(ConstructionKey)
            : decodeTail(CreateEventExW(nullptr, nullptr, CREATE_EVENT_MANUAL_RESET, EVENT_MODIFY_STATE | SYNCHRONIZE))
        {
            winrt::check_bool(static_cast<bool>(decodeTail));
        }
        static std::shared_ptr<ImageLoadTask> Create(std::shared_ptr<ReadingMemory> const &memory);
        void AdoptSvgMetadata(NativeSvgBorrowSet &borrows);
        ModuleLifetime module;
        std::mutex mutex;
        std::shared_ptr<NativeImageRequest> request;
        winrt::Windows::Foundation::IAsyncOperation<winrt::Windows::Storage::Streams::IRandomAccessStream> open{
            nullptr};
        winrt::Windows::Foundation::IAsyncAction decode{nullptr};
        winrt::handle decodeTail;
        bool decodeStarted{}; // Protected by mutex; independent of IAsyncInfo cancellation status.
        std::atomic<bool> finished{};
        std::atomic<bool> cancelScheduled{};
        bool finishing{};
        // Protected by mutex. The self pin is the original quarantine owner;
        // no failed worker is moved to a second task or a retirement queue.
        NativeSvgJobState *svgState{};
        void *svgFailedNode{};
        notepads_markdown_svg_work *svgAccount{};
        notepads_markdown_svg_resources *svgResources{};
        notepads_markdown_core_cancel *svgCoreCancel{};
        notepads_markdown_svg_work_token svgFailedToken{};
        NativeSvgJobReceipt svgReceipt;
        std::shared_ptr<ReadingMemory> svgMemory;
        std::shared_ptr<NativeImageDecodeResult> svgResult;
        NativeImageDecodeResult decodeResult;
        NativeEncodedImageHandle svgEncoded;
        std::shared_ptr<NativeImageBytes> svgEncodedCharge;
        std::shared_ptr<ImagePixels> svgCandidate;
        std::shared_ptr<ImageLoadTask> svgKeepAlive;
        bool svgStarted{};
        bool svgRetired{};
        bool svgFinishPending{};
        bool svgQuarantined{};
        bool svgDecodeComplete{};
        bool svgOwnerHandoffCommitted{};
        // Published/revoked only under mutex; synchronizes retained immutable source sharing.
        bool svgEncodedShareable{};
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        // Fixed optional test input. Production never changes this value.
        NativeSvgJobTestFault svgTestFault{NativeSvgJobTestFault::None};
#endif
#if defined(NOTEPADS_MARKDOWN_SVG_COVERAGE_TESTS)
        // Fixed input is set before worker publication. The output owns only
        // the test's explicit AddRef, transferred after actual worker cleanup.
        uint32_t svgCoverageTestFailurePoint{};
        bool svgCoverageTestRetainOwner{};
        winrt::handle testCoverageFontReached, testCoverageFontProceed;
        IUnknown *svgCoverageTestRetainedOwner{};
#endif
#if defined(NOTEPADS_MARKDOWN_SVG_QUALITY_TESTS)
        // Frozen probe input and real plan output belong to this paid task.
        // They survive the actual worker and unique reaper, with no global hook.
        std::shared_ptr<NativeSvgFontQualityConfig const> svgQualityConfig;
        NativeSvgFontQualityTrace svgQualityTrace;
        NativeSvgQualityObservation svgQualityObservation;
#endif
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        // Owned, preallocated correctness gates. No production queue/timer or
        // allocation depends on them; quarantine keeps their real handles live.
        winrt::handle testBeforeCommitReached, testBeforeCommitProceed;
        winrt::handle testAfterCommitReached, testAfterCommitProceed;
        winrt::handle testJobCreated, testLeaseRetained, testLeaseProceed, testReaperWaitingLease;
        winrt::handle testWorkerTailReached, testWorkerTailProceed, testWorkerThread;
#endif
        // Actual allocate_shared implementation block, including its strong
        // and weak control state. The allocator prefix owns its final free.
        void *allocationBlock{};
        size_t allocationBytes{};
        notepads_markdown_svg_retained *metadataRetained{};
        void Cancel() noexcept;
        void Finish() noexcept;
        // The outer decode calls this only after stream Close and release of
        // its owned IO inputs. SVG retirement alone cannot release that IO slot.
        void NotifyDecodeComplete() noexcept;
        // Background only. Cancelled-before-Completed registration can awaken
        // an outer WinRT await before the decoder body has retired its IO.
        // False quarantines this same task/permit and never touches its result.
        bool WaitForDecodeComplete() noexcept;
    };
    // UI-owned current table; tasks contain no XAML/control reference. The same
    // Rust policy and two physical slots survive source/provider/unload changes.
    struct ImageCoordinator : std::enable_shared_from_this<ImageCoordinator>
    {
        // Reclaim is a separate notice, not an invented capacity generation.
        static constexpr uint32_t CapacityReturned = 1, OptionalReclaim = 2;
        ~ImageCoordinator();
        void SetCapacitySink(winrt::weak_ref<winrt::Windows::Foundation::IInspectable> target,
                             winrt::Windows::UI::Core::CoreDispatcher dispatcher,
                             void (*invoke)(winrt::Windows::Foundation::IInspectable const &, ImageCoordinator const *, uint32_t) noexcept);
        void ScheduleCapacity(uint32_t notices = CapacityReturned) noexcept;
        std::shared_ptr<NativeImagePolicy> policy;
        std::shared_ptr<NativeImageReuseCache> reuse;
        void EnsureReuse(std::shared_ptr<ReadingMemory> const &memory) noexcept;
        void ClearReuse() noexcept;
        void DiscardTransition() noexcept;
        std::array<std::shared_ptr<ImageLoadTask>, 2> tasks;
        struct Pixels
        {
            uint32_t object{};
            uint64_t publishedRequest{};
            std::shared_ptr<ImagePixels const> value;
            NativeEncodedImageHandle encoded;
            uint64_t lastUse{};
            notepads_markdown_image_target target{};
            std::chrono::steady_clock::time_point animationStart{};
            uint32_t frameIndex{};
            bool HasContent() const noexcept
            {
                return publishedRequest || value || encoded;
            }
        };
        std::array<Pixels, NOTEPADS_MARKDOWN_IMAGE_MAX_ENTRIES> pixels;
        struct Intrinsic
        {
            uint32_t object{};
            notepads_markdown_image_metadata metadata{};
            bool valid{};
        };
        struct Intrinsics
        {
            notepads_markdown_core_identity identity{};
            uint64_t textGeneration{};
            ReadingVector<Intrinsic> entries;
            explicit Intrinsics(std::shared_ptr<ReadingMemory> const &memory)
                : identity(memory->Identity()), entries(ReadingAllocator<Intrinsic>{memory}) {}
        };
        std::shared_ptr<Intrinsics> intrinsics;
        // One source transition only. Semantic matching consumes this pin
        // before the replacement layout is published; payloads retain only
        // their independent original allocation/byte owners thereafter.
        std::shared_ptr<SemanticDescriptors const> transitionDescriptors;
        notepads_markdown_image_epoch transitionEpoch{};
        bool transitionDark{};
        uint64_t useSequence{};
        void PrepareIntrinsics(std::shared_ptr<ReadingMemory> const &memory, SemanticDescriptors const &descriptors);
        void RememberMetadata(NativeImageSnapshot const &snapshot);
        void ClearIntrinsic(uint32_t object) noexcept;
        uint64_t epochGeneration{};
        uint64_t textGeneration{};
        uint64_t providerGeneration{};
        uint64_t stateGeneration{};
        bool bound{};
        Pixels *FindPixels(uint32_t object) noexcept;
        Pixels const *FindPixels(uint32_t object) const noexcept;
        Pixels *FindFreePixels() noexcept;
        // Owning UI thread only; never treats cancellation as retirement.
        void ClearFinishedTasks() noexcept;
        void Retire(bool keepReuse = false, bool keepCompleted = false) noexcept;
        std::shared_ptr<ImagePresentationSnapshot> Freeze(
            std::shared_ptr<ReadingMemory> const &memory, ReadingAllocationFactory factory = {}) const;
        void PopulatePresentation(ImagePresentationSnapshot &result, ReadingMemory const &memory) const;
        // Registry linkage is protected by the private process registry lock.
        ImageCoordinator *capacityNext{};
        uint64_t capacityIdentity{};
      private:
        winrt::weak_ref<winrt::Windows::Foundation::IInspectable> _capacityTarget;
        winrt::Windows::UI::Core::CoreDispatcher _capacityDispatcher{nullptr};
        void (*_capacityInvoke)(winrt::Windows::Foundation::IInspectable const &, ImageCoordinator const *, uint32_t) noexcept {};
        struct CapacityPost;
        void DeliverCapacity() noexcept;
        std::atomic<bool> _capacityPending{};
        std::atomic<uint32_t> _capacityNotices{};
    };
}
