// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Resources/Images/NativeImageCoordinator.h"
#include "Resources/Images/NativeImageReuseCache.h"
#include "Reading/NativeReadingNavigation.h"
#include <algorithm>
#include <cstring>

namespace Notepads::Markdown::Native
{
    namespace
    {
        std::mutex capacityRegistration;
        uint64_t capacityPolicies{};
        std::mutex capacityRegistry;
        ImageCoordinator *capacityHead{};
        uint64_t nextCapacityIdentity{};
        struct CapacityLifetime
        {
            ModuleLifetime module;
            CapacityLifetime() noexcept = default;
            CapacityLifetime(CapacityLifetime const &) noexcept {}
            CapacityLifetime(CapacityLifetime &&) noexcept {}
        };

        void BroadcastCapacityNotice(uint32_t notices) noexcept
        {
            ModuleLifetime module;
            uint64_t cursor{}, end{};
            {
                std::lock_guard guard(capacityRegistry);
                end = nextCapacityIdentity;
            }
            for (;;)
            {
                std::array<std::shared_ptr<ImageCoordinator>, 16> batch;
                size_t count{};
                uint64_t next = end;
                {
                    std::lock_guard guard(capacityRegistry);
                    // The intrusive list is newest first. Select a bounded
                    // descending-ID batch; new registrations wait for a future
                    // notification and removed waiters need no acknowledgement.
                    for (auto waiter = capacityHead; waiter; waiter = waiter->capacityNext)
                    {
                        if (waiter->capacityIdentity > end || (cursor && waiter->capacityIdentity >= cursor))
                            continue;
                        auto owner = waiter->weak_from_this().lock();
                        if (!owner)
                            continue;
                        batch[count++] = std::move(owner);
                        next = waiter->capacityIdentity;
                        if (count == batch.size())
                            break;
                    }
                }
                if (!count)
                    return;
                for (size_t index = 0; index < count; ++index)
                    batch[index]->ScheduleCapacity(notices);
                cursor = next;
            }
        }

        void NOTEPADS_MARKDOWN_FFI_CALL CapacityAvailable(uint64_t) noexcept
        {
            BroadcastCapacityNotice(ImageCoordinator::CapacityReturned);
        }

        struct TaskAllocation
        {
            void *block{};
            size_t bytes{};
        };
        void *AllocateTask(void *context, ReadingMemory const &memory, size_t bytes)
        {
            auto &allocation = *static_cast<TaskAllocation *>(context);
            if (allocation.block)
                throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
            allocation.block = memory.Allocate(bytes);
            allocation.bytes = bytes;
            return allocation.block;
        }
        int32_t FreeTaskMetadata(void *context) noexcept
        {
            return notepads_markdown_svg_retained_after_physical_free_checked(static_cast<notepads_markdown_svg_retained *>(context));
        }
    }

    void RequestOptionalImageReclaim() noexcept
    {
        BroadcastCapacityNotice(ImageCoordinator::OptionalReclaim);
    }

    ImageCoordinator::~ImageCoordinator()
    {
        std::lock_guard guard(capacityRegistry);
        auto link = &capacityHead;
        while (*link && *link != this)
            link = &(*link)->capacityNext;
        if (*link)
            *link = capacityNext;
    }

    void RegisterImageCapacityNotification() noexcept
    {
        std::lock_guard registration(capacityRegistration);
        if (capacityPolicies++ == 0)
            notepads_markdown_image_set_process_capacity_notify(&CapacityAvailable);
    }

    void UnregisterImageCapacityNotification() noexcept
    {
        // The NativeImageBytes being destroyed owns its NativeImagePolicy
        // throughout Rust's synchronous callback. Thus the last policy cannot
        // die inside that callback, even if its last weak waiter dies there.
        // This gate is never taken by CapacityAvailable; no registry/ledger
        // lock is held while the Rust unregister drains in-flight callbacks.
        std::lock_guard registration(capacityRegistration);
        if (--capacityPolicies == 0)
            notepads_markdown_image_set_process_capacity_notify(nullptr);
    }

    void ImageCoordinator::SetCapacitySink(winrt::weak_ref<winrt::Windows::Foundation::IInspectable> target,
                                           winrt::Windows::UI::Core::CoreDispatcher dispatcher,
                                           void (*invoke)(winrt::Windows::Foundation::IInspectable const &, ImageCoordinator const *, uint32_t) noexcept)
    {
        if (capacityIdentity)
            return;
        if (!dispatcher || !invoke)
            throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
        {
            std::lock_guard guard(capacityRegistry);
            if (nextCapacityIdentity == UINT64_MAX)
                throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_ARITHMETIC_OVERFLOW};
            _capacityTarget = std::move(target);
            _capacityDispatcher = std::move(dispatcher);
            _capacityInvoke = invoke;
            capacityIdentity = ++nextCapacityIdentity;
            capacityNext = capacityHead;
            capacityHead = this;
        }
    }

    // The dispatched delegate owns the existing coordinator, never its target
    // control/model. If the dispatcher destroys a queued delegate without
    // invoking it, abandon pending state rather than permanently wedging it.
    struct ImageCoordinator::CapacityPost
    {
        std::shared_ptr<ImageCoordinator> owner;
        CapacityLifetime lifetime;
        explicit CapacityPost(std::shared_ptr<ImageCoordinator> value) noexcept : owner(std::move(value)) {}
        CapacityPost(CapacityPost &&other) noexcept : owner(std::move(other.owner)) {}
        CapacityPost(CapacityPost const &) = delete;
        ~CapacityPost()
        {
            if (owner) owner->_capacityPending.store(false, std::memory_order_release);
        }
        void Invoke() noexcept
        {
            auto current = std::move(owner);
            current->DeliverCapacity();
        }
    };

    void ImageCoordinator::DeliverCapacity() noexcept
    {
        // Clear pending BEFORE consuming bits. A racing producer either merges
        // into this delivery or posts another one; no merged pressure is lost.
        // An extra already-posted delivery may consume zero and is harmless.
        _capacityPending.store(false, std::memory_order_release);
        auto notices = _capacityNotices.exchange(0, std::memory_order_acq_rel);
        if (notices)
            if (auto target = _capacityTarget.get())
                _capacityInvoke(target, this, notices);
    }

    void ImageCoordinator::ScheduleCapacity(uint32_t notices) noexcept
    {
        _capacityNotices.fetch_or(notices, std::memory_order_acq_rel);
        if (_capacityPending.exchange(true, std::memory_order_acq_rel))
            return;
        try
        {
            (void)_capacityDispatcher.RunAsync(winrt::Windows::UI::Core::CoreDispatcherPriority::Low,
                [delivery = CapacityPost{shared_from_this()}]() mutable { delivery.Invoke(); });
        }
        catch (...)
        {
            // Retain notice bits for a future genuine notification. There is
            // no polling/repost loop when the dispatcher is unavailable.
            _capacityPending.store(false, std::memory_order_release);
        }
    }

    void ImageCoordinator::RememberMetadata(NativeImageSnapshot const &snapshot)
    {
        if (!intrinsics || intrinsics->textGeneration != textGeneration)
            return;
        for (uint32_t index = 0; index < snapshot.header.count; ++index)
        {
            auto const &row = snapshot.entries[index];
            if (!(row.status_flags & NOTEPADS_MARKDOWN_IMAGE_HAS_METADATA))
                continue;
            auto found = std::lower_bound(intrinsics->entries.begin(), intrinsics->entries.end(), row.object,
                                         [](auto const &entry, uint32_t object) { return entry.object < object; });
            if (found != intrinsics->entries.end() && found->object == row.object)
            {
                found->metadata = row.metadata;
                found->valid = true;
            }
        }
    }

    void ImageCoordinator::PrepareIntrinsics(std::shared_ptr<ReadingMemory> const &memory, SemanticDescriptors const &descriptors)
    {
        if (intrinsics)
            return;
        auto metadata = std::allocate_shared<Intrinsics>(ReadingAllocator<Intrinsics>{memory}, memory);
        metadata->textGeneration = descriptors.textGeneration;
        auto count = std::count_if(descriptors.objects.begin(), descriptors.objects.end(),
                                  [](auto const &object) { return object.kind == ReadingObjectKind::Image; });
        metadata->entries.reserve(static_cast<size_t>(count));
        for (auto const &object : descriptors.objects)
            if (object.kind == ReadingObjectKind::Image)
                metadata->entries.push_back({object.object});
        std::sort(metadata->entries.begin(), metadata->entries.end(), [](auto const &left, auto const &right) { return left.object < right.object; });
        intrinsics = std::move(metadata);
    }
    void ImageCoordinator::ClearIntrinsic(uint32_t object) noexcept
    {
        if (!intrinsics)
            return;
        auto row = std::lower_bound(intrinsics->entries.begin(), intrinsics->entries.end(), object,
            [](auto const &entry, uint32_t id) { return entry.object < id; });
        if (row != intrinsics->entries.end() && row->object == object)
            row->valid = false;
    }

    std::shared_ptr<ImageLoadTask> ImageLoadTask::Create(std::shared_ptr<ReadingMemory> const &memory)
    {
        if (!memory)
            throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
        TaskAllocation allocation;
        ImagePayloadConstruction construction{*memory, {&allocation, AllocateTask}};
        auto task = std::allocate_shared<ImageLoadTask>(ImagePayloadAllocator<ImageLoadTask>{construction.Nonce()}, ConstructionKey{});
        task->allocationBlock = allocation.block;
        task->allocationBytes = allocation.bytes;
        return task;
    }
    void ImageLoadTask::AdoptSvgMetadata(NativeSvgBorrowSet &borrows)
    {
        if (!allocationBlock || !allocationBytes || metadataRetained || !borrows.CanAdd())
            throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
        auto info = ReadingMemory::InspectAllocation(allocationBlock);
        if (info.hasAction || info.requestedTotal != ReadingMemory::AllocationRequestBytes(allocationBytes))
            throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
        notepads_markdown_svg_work_token token{};
        auto error = notepads_markdown_core_error{NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(notepads_markdown_core_error)};
        auto status = notepads_markdown_svg_resources_retained_prepare(NOTEPADS_MARKDOWN_SVG_RESOURCES_ABI_VERSION, borrows.Resources(),
                                                        NOTEPADS_MARKDOWN_SVG_RETAINED_METADATA, nullptr, &info.lease,
                                                        info.requestedTotal, 0, &metadataRetained, &token, &error);
        if (status != NOTEPADS_MARKDOWN_CORE_OK)
            throw NativeImageFailure{status};
        ReadingMemory::BindFreeAction(allocationBlock, {metadataRetained, FreeTaskMetadata});
        notepads_markdown_svg_retained_bind_physical(metadataRetained);
        borrows.Add(metadataRetained, token);
        status = notepads_markdown_svg_retained_admit(metadataRetained, &error);
        if (status != NOTEPADS_MARKDOWN_CORE_OK)
            throw NativeImageFailure{status};
    }
    void ImageLoadTask::Cancel() noexcept
    {
        winrt::Windows::Foundation::IAsyncInfo opening{nullptr}, decoding{nullptr};
        {
            std::lock_guard lock(mutex);
            if (request)
                request->Cancel();
            opening = open;
            decoding = decode;
        }
        CancelSvgImageTask(*this);
        try
        {
            if (opening)
                opening.Cancel();
        }
        catch (...)
        {
        }
        try
        {
            if (decoding)
                decoding.Cancel();
        }
        catch (...)
        {
        }
    }
    void ImageLoadTask::Finish() noexcept
    {
        winrt::Windows::Foundation::IAsyncOperation<winrt::Windows::Storage::Streams::IRandomAccessStream> opening{
            nullptr};
        winrt::Windows::Foundation::IAsyncAction decoding{nullptr};
        std::shared_ptr<NativeImageRequest> permit;
        {
            std::lock_guard lock(mutex);
            if (finishing || finished.load(std::memory_order_acquire))
                return;
            if ((!svgStarted && svgEncoded) || !IsSvgImageTaskRetiredLocked(*this) ||
                ((decodeStarted || svgStarted) && !svgDecodeComplete))
            {
                svgFinishPending = true;
                return;
            }
            finishing = true;
            decoding = std::exchange(decode, nullptr);
            opening = std::exchange(open, nullptr);
            permit = std::move(request);
        }
        // External COM releases and the final native permit drop hold no task
        // mutex. The module remains pinned by this task through these releases.
        decoding = nullptr;
        opening = nullptr;
        permit.reset();
        finished.store(true, std::memory_order_release);
    }
    void ImageLoadTask::NotifyDecodeComplete() noexcept
    {
        bool finishPending{};
        {
            std::lock_guard lock(mutex);
            svgDecodeComplete = true;
            if (!SetEvent(decodeTail.get()))
            {
                svgQuarantined = true;
                svgKeepAlive = weak_from_this().lock();
            }
            finishPending = svgFinishPending && IsSvgImageTaskRetiredLocked(*this);
        }
        if (finishPending)
            Finish();
    }
    bool ImageLoadTask::WaitForDecodeComplete() noexcept
    {
        {
            std::lock_guard lock(mutex);
            if (svgQuarantined)
                return false;
            if (!decodeStarted || svgDecodeComplete)
                return true;
        }
        APTTYPE apartment{};
        APTTYPEQUALIFIER qualifier{};
        auto apartmentStatus = CoGetApartmentType(&apartment, &qualifier);
        const auto mayWait = apartmentStatus == CO_E_NOTINITIALIZED ||
                             (SUCCEEDED(apartmentStatus) &&
                              (apartment == APTTYPE_MTA ||
                               (apartment == APTTYPE_NA &&
                                (qualifier == APTTYPEQUALIFIER_NA_ON_MTA ||
                                 qualifier == APTTYPEQUALIFIER_NA_ON_IMPLICIT_MTA))));
        // Scheduling the observer onto a background thread can itself fail.
        // Its catch path must never block an STA needed by the active provider.
        auto waited = mayWait ? WaitForSingleObjectEx(decodeTail.get(), INFINITE, FALSE) : WAIT_FAILED;
        std::lock_guard lock(mutex);
        if ((waited == WAIT_OBJECT_0 || !mayWait) && svgDecodeComplete && !svgQuarantined)
            return true;
        // Waiting is observation only: a failure is never permission to close
        // or recycle an active read, nor to mutate its decoder-owned result.
        svgQuarantined = true;
        svgKeepAlive = weak_from_this().lock();
        return false;
    }
    ImageCoordinator::Pixels *ImageCoordinator::FindPixels(uint32_t object) noexcept
    {
        auto found = std::find_if(pixels.begin(), pixels.end(),
                                  [&](auto const &entry) { return entry.HasContent() && entry.object == object; });
        return found == pixels.end() ? nullptr : &*found;
    }
    ImageCoordinator::Pixels const *ImageCoordinator::FindPixels(uint32_t object) const noexcept
    {
        auto found = std::find_if(pixels.begin(), pixels.end(),
                                  [&](auto const &entry) { return entry.HasContent() && entry.object == object; });
        return found == pixels.end() ? nullptr : &*found;
    }
    ImageCoordinator::Pixels *ImageCoordinator::FindFreePixels() noexcept
    {
        auto found = std::find_if(pixels.begin(), pixels.end(), [](auto const &entry) { return !entry.HasContent(); });
        return found == pixels.end() ? nullptr : &*found;
    }
    void ImageCoordinator::ClearFinishedTasks() noexcept
    {
        // UI-owned slots only. Finish publishes this flag after actual worker,
        // decode/borrow retirement and external COM/permit release. Pending or
        // quarantined work must keep its original slot and physical permit.
        for (auto &task : tasks)
            if (task && task->finished.load(std::memory_order_acquire))
                task.reset();
    }
    void ImageCoordinator::EnsureReuse(std::shared_ptr<ReadingMemory> const &memory) noexcept
    {
        if (reuse)
            return;
        try
        {
            reuse = NativeImageReuseCache::Create(memory);
        }
        catch (...)
        {
            // Warmth is optional. Cache metadata admission/allocation failure
            // cannot reject an otherwise valid current provider request.
        }
    }
    void ImageCoordinator::ClearReuse() noexcept
    {
        if (reuse)
            reuse->Clear();
    }
    void ImageCoordinator::DiscardTransition() noexcept
    {
        if (!transitionDescriptors)
            return;
        transitionDescriptors.reset();
        transitionEpoch = {};
        for (auto &entry : pixels)
            entry = {};
    }
    void ImageCoordinator::Retire(bool keepReuse, bool keepCompleted) noexcept
    {
        // A newer source can be superseded after Bind but before publishing
        // descriptors or a Frame. In that interval pixels still have the
        // inherited descriptor/epoch IDs, not the newer empty policy's IDs.
        if (keepCompleted && bound && transitionDescriptors && transitionDescriptors->textGeneration == textGeneration)
        {
            try
            {
                auto snapshot = policy->Snapshot();
                transitionEpoch = snapshot.header.epoch;
                for (auto &entry : pixels)
                {
                    auto row = std::find_if(snapshot.entries.begin(), snapshot.entries.begin() + snapshot.header.count,
                        [&](auto const &value) { return value.object == entry.object; });
                    if (!entry.value || !entry.value->IsFrozen() ||
                        row == snapshot.entries.begin() + snapshot.header.count ||
                        row->state != NOTEPADS_MARKDOWN_IMAGE_READY ||
                        !(row->status_flags & NOTEPADS_MARKDOWN_IMAGE_TARGET_SATISFIED) ||
                        row->published_request != entry.publishedRequest ||
                        !SameImageMetadata(row->metadata, entry.value->metadata))
                        entry = {};
                    else
                        entry.target = row->published_target;
                }
                keepCompleted = std::any_of(pixels.begin(), pixels.end(), [](auto const &entry) { return !!entry.value; });
            }
            catch (...)
            {
                keepCompleted = false;
            }
        }
        for (auto const &task : tasks)
            if (task)
            {
                {
                    std::lock_guard lock(task->mutex);
                    if (task->request)
                        task->request->Cancel();
                }
                CancelSvgImageTask(*task);
            }
        ClearFinishedTasks();
        if (!keepReuse)
            ClearReuse();
        if (!keepCompleted)
        {
            transitionDescriptors.reset();
            transitionEpoch = {};
            for (auto &entry : pixels)
                entry = {};
        }
        bound = false;
        textGeneration = 0;
        intrinsics.reset();
    }
    std::shared_ptr<ImagePresentationSnapshot> ImageCoordinator::Freeze(
        std::shared_ptr<ReadingMemory> const &memory, ReadingAllocationFactory factory) const
    {
        if (!bound)
            return {};
        auto result = std::allocate_shared<ImagePresentationSnapshot>(
            ReadingAllocator<ImagePresentationSnapshot>{memory, factory}, memory);
        PopulatePresentation(*result, *memory);
        return result;
    }

    void ImageCoordinator::PopulatePresentation(ImagePresentationSnapshot &result, ReadingMemory const &memory) const
    {
        result.textGeneration = textGeneration;
        result.policy = policy->Snapshot();
        auto sameSidecar = intrinsics && intrinsics->textGeneration == textGeneration &&
                           std::memcmp(&intrinsics->identity, &memory.Identity(), sizeof(intrinsics->identity)) == 0;
        result.entries.clear();
        result.entries.reserve(result.policy.header.count);
        for (uint32_t i = 0; i < result.policy.header.count; ++i)
        {
            ImagePresentationEntry row{result.policy.entries[i], {}};
            if (!(row.state.status_flags & NOTEPADS_MARKDOWN_IMAGE_HAS_METADATA) &&
                row.state.state != NOTEPADS_MARKDOWN_IMAGE_READY && sameSidecar)
            {
                auto intrinsic = std::lower_bound(intrinsics->entries.begin(), intrinsics->entries.end(), row.state.object,
                    [](auto const &entry, uint32_t object) { return entry.object < object; });
                if (intrinsic != intrinsics->entries.end() && intrinsic->object == row.state.object && intrinsic->valid)
                {
                    // A re-registered policy row has fresh authorization and
                    // may not know dimensions until WIC returns. Its same-
                    // source sidecar supplies geometry only, never Ready,
                    // pixels, a request ID or target satisfaction authority.
                    row.state.metadata = intrinsic->metadata;
                    row.state.status_flags |= NOTEPADS_MARKDOWN_IMAGE_HAS_METADATA;
                }
            }
            if (row.state.state == NOTEPADS_MARKDOWN_IMAGE_READY)
            {
                if (auto entry = FindPixels(row.state.object))
                    if (entry->value && entry->publishedRequest == row.state.published_request &&
                        SameImageMetadata(entry->value->metadata, row.state.metadata))
                    {
                        row.pixels = entry->value;
                        row.frameIndex = entry->frameIndex;
                    }
            }
            result.entries.push_back(std::move(row));
        }
        if (sameSidecar)
            for (auto const &intrinsic : intrinsics->entries)
            {
                if (!intrinsic.valid || std::any_of(result.policy.entries.begin(), result.policy.entries.begin() + result.policy.header.count,
                    [&](auto const &entry) { return entry.object == intrinsic.object; }))
                    continue;
                ImagePresentationEntry row{};
                row.state.object = intrinsic.object;
                row.state.state = NOTEPADS_MARKDOWN_IMAGE_UNREQUESTED;
                row.state.metadata = intrinsic.metadata;
                row.state.status_flags = NOTEPADS_MARKDOWN_IMAGE_HAS_METADATA;
                result.entries.push_back(std::move(row));
            }
        std::sort(result.entries.begin(), result.entries.end(),
                  [](auto const &a, auto const &b) { return a.state.object < b.state.object; });
    }
}
