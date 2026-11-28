// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Resources/Diagrams/NativeDiagramWorker.h"
#include "Core/NativeThreadpool.h"
#include "Resources/Diagrams/NativeDiagramCoordinator.h"
#include <cmath>
#include <cstring>
#include <new>
#include <type_traits>

namespace Notepads::Markdown::Native
{
    namespace
    {
        constexpr SIZE_T InitialStackCommit = 7680 * 1024;
        constexpr uint64_t StackBudget = 8 * 1024 * 1024;
        constexpr uint64_t EntrySlack = 32 * 1024;
        std::atomic<uint32_t> liveNodes{}, waitFailures{}, freeFailures{}, deferredRetirements{}, callbackPinFailures{};
        notepads_markdown_core_error Error() noexcept
        {
            return {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(notepads_markdown_core_error)};
        }
    }

    struct NativeDiagramWorker::PreparedNode final
    {
        enum class Phase : uint8_t
        {
            New,
            Queued,
            Starting,
            Running,
            Complete,
            Retiring,
            Quarantined
        };
        std::atomic<uint32_t> references{1};
        std::atomic<Phase> phase{Phase::New};
        std::atomic<bool> canceled{};
        std::atomic<bool> retirementArmed{};
        std::atomic_flag pumping = ATOMIC_FLAG_INIT;
        std::atomic<uint32_t> streamSlots{};
        SRWLOCK timerLock = SRWLOCK_INIT;
        PTP_TIMER timer{};
        HANDLE thread{};
        PTP_WAIT wait{};
        notepads_markdown_diagram_job *job{};
        Completion completion{};
        Admission admission{};
        void *owner{};
        PreparedNode *next{};
        uint64_t startedTicks{};
        uint32_t ordinal{};
        NativeDiagramKey key;
        notepads_markdown_diagram_font_capability fonts{};
        IUnknown *fontOwners[3]{};
        notepads_markdown_diagram_style style{};
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        NativeDiagramWorkerTestHooks hooks;
#endif
        NativeDiagramWorkerResult result;
        NativeDiagramArtifactResult artifact;
        alignas(NativeDiagramPresentation) uint8_t presentation[sizeof(NativeDiagramPresentation)]{};
        alignas(std::max_align_t) uint8_t transport[NativeDiagramTransport::EmbeddedBytes]{};
        alignas(
            std::max_align_t) uint8_t streams[NativeDiagramArtifact::StreamSlots][NativeDiagramArtifact::StreamBytes]{};
    };
    static_assert(sizeof(NativeDiagramWorker::PreparedNode) <= NOTEPADS_MARKDOWN_DIAGRAM_NATIVE_NODE_BYTES);
    // Last free must preserve the still-live Node if HeapFree fails. Its POD
    // identity, original FFI reference and manual module pin remain intact.
    static_assert(std::is_trivially_destructible_v<NativeDiagramWorker::PreparedNode>);

    namespace
    {
        using Node = NativeDiagramWorker::PreparedNode;
        using Phase = Node::Phase;
        void CloseTimer(Node *node, PTP_CALLBACK_INSTANCE instance) noexcept;
        void CALLBACK PumpQueued(PTP_CALLBACK_INSTANCE, void *, PTP_TIMER) noexcept;
        void CALLBACK RetryRetirement(PTP_CALLBACK_INSTANCE, void *, PTP_TIMER) noexcept;

        void ReleaseFonts(Node *node) noexcept
        {
            for (auto &owner : node->fontOwners)
            {
                if (auto value = std::exchange(owner, nullptr))
                    value->Release();
            }
            node->fonts.factory = nullptr;
            node->fonts.collection = nullptr;
            node->fonts.fallback = nullptr;
        }
        void NotifyCapacity() noexcept
        {
            NativeMathCoordinator::NotifyResourcesAvailable();
        }
        void ArmTimer(Node *node, DWORD period) noexcept
        {
            LARGE_INTEGER due{};
            due.QuadPart = -1;
            FILETIME time{due.LowPart, static_cast<DWORD>(due.HighPart)};
            SetThreadpoolTimer(node->timer, &time, period, 0);
        }
        void Quarantine(Node *node) noexcept
        {
            node->references.store(1, std::memory_order_release);
            node->phase.store(Phase::Quarantined, std::memory_order_release);
            freeFailures.fetch_add(1, std::memory_order_relaxed);
            // No destructor, FFI release, slot return or module-pin decrement.
            // The original Node and original context are the retained owner.
        }
        void DeferRetirement(Node *node, bool lastReference) noexcept
        {
            if (node->retirementArmed.exchange(true, std::memory_order_acq_rel))
                return;
            if (lastReference)
                node->references.store(1, std::memory_order_release);
            else
                NativeDiagramWorker::RetainPrepared(node);
            node->phase.store(Phase::Retiring, std::memory_order_release);
            auto timer = CreateThreadpoolTimer(RetryRetirement, node, nullptr);
            if (!timer)
            {
                node->phase.store(Phase::Quarantined, std::memory_order_release);
                freeFailures.fetch_add(1, std::memory_order_relaxed);
                return; // This new owner/pin deliberately remains alive.
            }
            deferredRetirements.fetch_add(1, std::memory_order_relaxed);
            AcquireSRWLockExclusive(&node->timerLock);
            node->timer = timer;
            ArmTimer(node, 100);
            ReleaseSRWLockExclusive(&node->timerLock);
        }
        bool ClearNativeBytes(Node *node) noexcept
        {
            if (!node->artifact.bytes)
                return true;
            if (!HeapFree(GetProcessHeap(), 0, node->artifact.bytes))
                return false;
            node->artifact.bytes = nullptr;
            node->artifact.capacity = 0;
            return true;
        }
        void CloseTimer(Node *node, PTP_CALLBACK_INSTANCE instance) noexcept
        {
            AcquireSRWLockExclusive(&node->timerLock);
            auto timer = std::exchange(node->timer, nullptr);
            if (timer)
                SetThreadpoolTimer(timer, nullptr, 0, 0);
            ReleaseSRWLockExclusive(&node->timerLock);
            if (!timer)
                return;
            if (instance)
                DisassociateCurrentThreadFromCallback(instance);
            WaitForThreadpoolTimerCallbacks(timer, TRUE);
            CloseThreadpoolTimer(timer);
            if (node->phase.load(std::memory_order_acquire) != Phase::New)
                NativeDiagramWorker::ReleasePrepared(node);
        }
        void CALLBACK RetryRetirement(PTP_CALLBACK_INSTANCE instance, void *context, PTP_TIMER) noexcept
        {
            if (!PinModuleUntilCallbackReturns(instance))
            {
                callbackPinFailures.fetch_add(1, std::memory_order_relaxed);
                return; // Keep the original Node/module pin and actual funding.
            }
            ModuleLifetime callbackModule;
            auto node = static_cast<Node *>(context);
            if (node->pumping.test_and_set(std::memory_order_acquire))
                return;
            NativeDiagramWorker::RetainPrepared(node);
            auto error = Error();
            auto status = notepads_markdown_diagram_job_retire_metadata(node->job, &error);
            if (status == NOTEPADS_MARKDOWN_CORE_OK)
            {
                node->phase.store(Phase::Complete, std::memory_order_release);
                node->retirementArmed.store(false, std::memory_order_release);
                deferredRetirements.fetch_sub(1, std::memory_order_relaxed);
                CloseTimer(node, instance);
                NotifyCapacity();
            }
            node->pumping.clear(std::memory_order_release);
            NativeDiagramWorker::ReleasePrepared(node);
        }

        __declspec(noinline) NativeDiagramStackObservation MeasureStack(uintptr_t marker) noexcept
        {
            NativeDiagramStackObservation out{};
            out.marker = marker;
            out.entrySlack = EntrySlack;
            out.threadId = GetCurrentThreadId();
            ULONG_PTR lower{}, upper{};
            GetCurrentThreadStackLimits(&lower, &upper);
            out.lowerLimit = lower;
            out.upperLimit = upper;
            MEMORY_BASIC_INFORMATION first{};
            if (lower >= upper || marker < lower || marker >= upper ||
                VirtualQuery(reinterpret_cast<void const *>(marker), &first, sizeof(first)) != sizeof(first))
            {
                out.queryError = GetLastError();
                return out;
            }
            auto base = reinterpret_cast<uintptr_t>(first.AllocationBase);
            out.allocationBase = base;
            if (!base || base > marker)
                return out;
            uintptr_t cursor = base, contiguousLower{};
            bool foundMarker{};
            while (cursor < upper)
            {
                MEMORY_BASIC_INFORMATION region{};
                if (VirtualQuery(reinterpret_cast<void const *>(cursor), &region, sizeof(region)) != sizeof(region))
                {
                    out.queryError = GetLastError();
                    return out;
                }
                auto start = reinterpret_cast<uintptr_t>(region.BaseAddress);
                if (region.AllocationBase != first.AllocationBase || start > cursor || !region.RegionSize ||
                    region.RegionSize > UINTPTR_MAX - start)
                    return out;
                auto end = start + region.RegionSize;
                if (end <= cursor || end > upper)
                    return out;
                if (region.State == MEM_COMMIT)
                {
                    out.committedBytes += region.RegionSize;
                    if (region.Protect & PAGE_GUARD)
                        out.guardBytes += region.RegionSize;
                }
                if (region.State == MEM_COMMIT && region.Protect == PAGE_READWRITE)
                {
                    if (!contiguousLower)
                        contiguousLower = (std::max)(start, static_cast<uintptr_t>(lower));
                }
                else
                    contiguousLower = 0;
                if (marker >= start && marker < end)
                {
                    if (contiguousLower && marker >= contiguousLower)
                    {
                        out.usableLower = contiguousLower;
                        auto distance = marker - contiguousLower;
                        out.headroom = distance > EntrySlack ? distance - EntrySlack : 0;
                    }
                    foundMarker = true;
                }
                cursor = end;
            }
            out.allocationBytes = cursor - base;
            out.measured = foundMarker && cursor == upper && out.usableLower != 0;
            out.withinStackBudget =
                out.measured && out.committedBytes <= StackBudget && out.allocationBytes <= StackBudget;
            return out;
        }
        __declspec(noinline) DWORD WINAPI WorkerEntry(void *context) noexcept
        {
            auto node = static_cast<Node *>(context);
            node->result.apartment = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
            if (SUCCEEDED(node->result.apartment))
            {
                volatile uint8_t marker{};
                node->result.stack = MeasureStack(reinterpret_cast<uintptr_t>(&marker));
                auto const &stack = node->result.stack;
                auto headroom = stack.measured && stack.withinStackBudget
                                    ? stack.headroom
                                    : 0;
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                if (node->hooks.forceInsufficientHeadroom)
                    headroom = 0;
#endif
                node->result.status =
                    notepads_markdown_diagram_job_run(node->job, &node->fonts, &node->style, headroom, &node->result.error);
                CoUninitialize();
            }
            else
            {
                node->result.status = NOTEPADS_MARKDOWN_DIAGRAM_FONT_UNAVAILABLE;
                node->result.error.status = node->result.status;
            }
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            if (node->hooks.tailReached && node->hooks.tailPermit)
            {
                SetEvent(node->hooks.tailReached);
                node->result.tailWaitResult = WaitForSingleObjectEx(node->hooks.tailPermit, 10000, FALSE);
            }
#endif
            return 0;
        }
        void CALLBACK Reap(PTP_CALLBACK_INSTANCE instance, void *context, PTP_WAIT wait, TP_WAIT_RESULT result) noexcept
        {
            auto node = static_cast<Node *>(context);
            auto actual = WaitForSingleObjectEx(node->thread, 0, FALSE);
            if (result != WAIT_OBJECT_0 || actual != WAIT_OBJECT_0)
            {
                waitFailures.fetch_add(1, std::memory_order_relaxed);
                if (actual == WAIT_TIMEOUT)
                    SetThreadpoolWait(wait, node->thread, nullptr);
                return;
            }
            if (!PinModuleUntilCallbackReturns(instance))
            {
                callbackPinFailures.fetch_add(1, std::memory_order_relaxed);
                return; // Keep the original Node/module pin and actual funding.
            }
            ModuleLifetime callbackModule;
            node->result.stack.threadSignaled = true;
            ReleaseFonts(node); // Original capability is held through real exit.
            node->completion(node, node->job, node->result);
            auto thread = node->thread;
            SetThreadpoolWait(wait, nullptr, nullptr);
            DisassociateCurrentThreadFromCallback(instance);
            WaitForThreadpoolWaitCallbacks(wait, TRUE);
            CloseThreadpoolWait(wait);
            CloseHandle(thread);
            node->thread = nullptr;
            node->wait = nullptr;
            node->phase.store(Phase::Complete, std::memory_order_release);
            NativeDiagramWorker::ReleasePrepared(node);
        }
        void CALLBACK PumpQueued(PTP_CALLBACK_INSTANCE instance, void *context, PTP_TIMER) noexcept
        {
            if (!PinModuleUntilCallbackReturns(instance))
            {
                callbackPinFailures.fetch_add(1, std::memory_order_relaxed);
                return; // Keep the original Node/module pin and actual funding.
            }
            ModuleLifetime callbackModule;
            auto node = static_cast<Node *>(context);
            if (node->pumping.test_and_set(std::memory_order_acquire))
                return;
            NativeDiagramWorker::RetainPrepared(node);
            if (node->phase.load(std::memory_order_acquire) != Phase::Queued)
            {
                node->pumping.clear(std::memory_order_release);
                NativeDiagramWorker::ReleasePrepared(node);
                return;
            }
            auto error = Error();
            int32_t status = node->canceled.load(std::memory_order_acquire)  ? NOTEPADS_MARKDOWN_CORE_CANCELLED
                             : GetTickCount64() - node->startedTicks >= 5000 ? NOTEPADS_MARKDOWN_CORE_DEADLINE
                                                                             : NOTEPADS_MARKDOWN_CORE_OK;
            if (status == NOTEPADS_MARKDOWN_CORE_OK && node->admission && !node->admission(node->owner, node))
                status = NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT;
            if (status == NOTEPADS_MARKDOWN_CORE_OK)
                status = notepads_markdown_diagram_job_activate(node->job, &error);
            if (status == NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT)
            {
                node->pumping.clear(std::memory_order_release);
                NativeDiagramWorker::ReleasePrepared(node);
                return;
            }
            node->phase.store(Phase::Starting, std::memory_order_release);
            CloseTimer(node, instance);
            if (status == NOTEPADS_MARKDOWN_CORE_OK && node->canceled.load(std::memory_order_acquire))
                status = NOTEPADS_MARKDOWN_CORE_CANCELLED;
            if (status == NOTEPADS_MARKDOWN_CORE_OK)
            {
                node->wait = CreateThreadpoolWait(Reap, node, nullptr);
                if (!node->wait)
                    status = NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY;
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                else if (node->hooks.simulatedCreateThreadError)
                    status = NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY;
#endif
                else
                {
                    node->phase.store(Phase::Running, std::memory_order_release);
                    NativeDiagramWorker::RetainPrepared(node);
                    node->thread = CreateThread(nullptr, InitialStackCommit, WorkerEntry, node, 0, nullptr);
                    if (!node->thread)
                    {
                        NativeDiagramWorker::ReleasePrepared(node);
                        status = NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY;
                    }
                    else
                    {
                        auto wait = node->wait;
                        auto thread = node->thread;
                        node->pumping.clear(std::memory_order_release);
                        SetThreadpoolWait(wait, thread, nullptr);
                        NativeDiagramWorker::ReleasePrepared(node);
                        return;
                    }
                }
            }
            if (node->wait)
            {
                CloseThreadpoolWait(node->wait);
                node->wait = nullptr;
            }
            node->result.status = status;
            node->result.error = error;
            node->result.error.status = status;
            ReleaseFonts(node);
            node->phase.store(Phase::Complete, std::memory_order_release);
            node->completion(node, node->job, node->result); // Proven no thread existed.
            node->pumping.clear(std::memory_order_release);
            NativeDiagramWorker::ReleasePrepared(node);
        }
    }

    int32_t NativeDiagramWorker::Prepare(notepads_markdown_math_control *control, notepads_markdown_core_model const *model,
                                         NativeDiagramKey const &key, uint32_t ordinal,
                                         notepads_markdown_diagram_font_capability const &fonts, notepads_markdown_diagram_style const &style,
                                         void *owner, Completion completion, PreparedNode **output,
                                         notepads_markdown_core_error *error, Admission admission) noexcept
    {
        if (!output || !error)
            return NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT;
        *output = nullptr;
        if (!control || !model || !owner || !completion || ordinal >= NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT ||
            fonts.version != NOTEPADS_MARKDOWN_DIAGRAM_ABI_VERSION || fonts.size != sizeof(fonts) || !fonts.factory ||
            !fonts.collection || !fonts.fallback ||
            fonts.font_epoch != key.fontEpoch || style.header.abi_version != NOTEPADS_MARKDOWN_DIAGRAM_ABI_VERSION ||
            style.header.struct_size != sizeof(style) || !std::isfinite(key.targetWidth) || !std::isfinite(key.dpi) ||
            key.targetWidth <= 0 || key.dpi <= 0)
        {
            error->status = NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT;
            return error->status;
        }
        auto started = GetTickCount64();
        notepads_markdown_diagram_job *job{};
        auto status = notepads_markdown_diagram_job_prepare(control, model, key.object, &job, error);
        if (status != NOTEPADS_MARKDOWN_CORE_OK)
            return status;
        status = notepads_markdown_diagram_job_claim_start(job, error);
        if (status != NOTEPADS_MARKDOWN_CORE_OK)
        {
            (void)notepads_markdown_diagram_job_release(job, error);
            return status;
        }
        auto storage = HeapAlloc(GetProcessHeap(), 0, NOTEPADS_MARKDOWN_DIAGRAM_NATIVE_NODE_BYTES);
        if (!storage)
        {
            (void)notepads_markdown_diagram_job_release(job, error);
            error->status = NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY;
            return error->status;
        }
        auto node = new (storage) PreparedNode{};
        ++winrt::get_module_lock(); // Manual pin survives failed HeapFree without destroying Node.
        liveNodes.fetch_add(1, std::memory_order_relaxed);
        node->job = job;
        node->key = key;
        node->ordinal = ordinal;
        node->owner = owner;
        node->completion = completion;
        node->admission = admission;
        node->startedTicks = started;
        node->fonts = fonts;
        node->style = style;
        void *pointers[3]{fonts.factory, fonts.collection, fonts.fallback};
        for (size_t index = 0; index < 3; ++index)
        {
            node->fontOwners[index] = static_cast<IUnknown *>(pointers[index]);
            node->fontOwners[index]->AddRef();
        }
        node->timer = CreateThreadpoolTimer(PumpQueued, node, nullptr);
        if (!node->timer)
        {
            ReleasePrepared(node);
            error->status = NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY;
            return error->status;
        }
        *output = node;
        return NOTEPADS_MARKDOWN_CORE_OK;
    }
    NativeDiagramStartResult NativeDiagramWorker::StartPrepared(PreparedNode *node) noexcept
    {
        if (!node)
            return {NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT, ERROR_INVALID_PARAMETER};
        auto expected = Phase::New;
        if (!node->phase.compare_exchange_strong(expected, Phase::Queued))
            return {NOTEPADS_MARKDOWN_CORE_INVALID_STATE, ERROR_INVALID_STATE};
        RetainPrepared(node);
        WakePrepared(node);
        return {NOTEPADS_MARKDOWN_CORE_OK, 0};
    }
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
    NativeDiagramStartResult NativeDiagramWorker::StartPreparedForTesting(
        PreparedNode *node, NativeDiagramWorkerTestHooks const &hooks) noexcept
    {
        if (!node || node->phase.load(std::memory_order_acquire) != Phase::New)
            return {NOTEPADS_MARKDOWN_CORE_INVALID_STATE, ERROR_INVALID_STATE};
        node->hooks = hooks;
        return StartPrepared(node);
    }
#endif
    void NativeDiagramWorker::RetainPrepared(PreparedNode *node) noexcept
    {
        node->references.fetch_add(1, std::memory_order_relaxed);
    }
    void NativeDiagramWorker::ReleasePrepared(PreparedNode *node) noexcept
    {
        if (node->references.fetch_sub(1, std::memory_order_acq_rel) != 1)
            return;
        ModuleLifetime callbackModule;
        ReleaseFonts(node);
        if (!ClearNativeBytes(node))
        {
            Quarantine(node);
            return;
        }
        auto state = notepads_markdown_diagram_job_state(node->job);
        if (state != NOTEPADS_MARKDOWN_DIAGRAM_FINALIZED && state != NOTEPADS_MARKDOWN_DIAGRAM_METADATA_ONLY)
        {
            if (state == NOTEPADS_MARKDOWN_DIAGRAM_POISONED || state == NOTEPADS_MARKDOWN_DIAGRAM_RUNNING)
            {
                Quarantine(node);
                return;
            }
            auto error = Error();
            if (notepads_markdown_diagram_job_retire_metadata(node->job, &error) != NOTEPADS_MARKDOWN_CORE_OK)
            {
                DeferRetirement(node, true);
                return;
            }
        }
        auto job = node->job;
        // Trivial Node has no RAII destruction that could erase a failed-free
        // obligation. The original job/pin remain stored in the live block.
        if (!HeapFree(GetProcessHeap(), 0, node))
        {
            Quarantine(node);
            return;
        }
        liveNodes.fetch_sub(1, std::memory_order_release);
        auto error = Error();
        // Root's ABI guarantee: sole post-signaled ref in FINALIZED or
        // METADATA_ONLY has no remaining fallible release gate or Rust owner.
        (void)notepads_markdown_diagram_job_release(job, &error);
        --winrt::get_module_lock();
        NotifyCapacity();
    }
    void NativeDiagramWorker::CancelPrepared(PreparedNode *node) noexcept
    {
        node->canceled.store(true, std::memory_order_release);
        auto error = Error();
        (void)notepads_markdown_diagram_job_cancel(node->job, &error);
        WakePrepared(node);
    }
    void NativeDiagramWorker::WakePrepared(PreparedNode *node) noexcept
    {
        AcquireSRWLockExclusive(&node->timerLock);
        if (node->timer && node->phase.load(std::memory_order_acquire) == Phase::Queued)
            ArmTimer(node, 100);
        ReleaseSRWLockExclusive(&node->timerLock);
    }
    void NativeDiagramWorker::DestroyQueued(PreparedNode *node) noexcept
    {
        if (!node || node->phase.load(std::memory_order_acquire) != Phase::New)
            return;
        CloseTimer(node, nullptr);
        ReleasePrepared(node);
    }
    NativeDiagramWorker::PreparedNode *NativeDiagramWorker::NextPrepared(PreparedNode *node) noexcept
    {
        return node->next;
    }
    void NativeDiagramWorker::SetNextPrepared(PreparedNode *node, PreparedNode *next) noexcept
    {
        node->next = next;
    }
    bool NativeDiagramWorker::IsQueued(PreparedNode *node) noexcept
    {
        return node->phase.load(std::memory_order_acquire) == Phase::Queued;
    }
    NativeDiagramKey const &NativeDiagramWorker::PreparedKey(PreparedNode *node) noexcept
    {
        return node->key;
    }
    uint32_t NativeDiagramWorker::PreparedOrdinal(PreparedNode *node) noexcept
    {
        return node->ordinal;
    }
    void *NativeDiagramWorker::PreparedOwner(PreparedNode *node) noexcept
    {
        return node->owner;
    }
    void NativeDiagramWorker::ClearPreparedOwner(PreparedNode *node) noexcept
    {
        node->owner = nullptr;
    }
    DiagramPresentationHandle NativeDiagramWorker::BuildPresentationAfterThreadExit(PreparedNode *node,
                                                                                    int32_t &status) noexcept
    {
        if (status == NOTEPADS_MARKDOWN_CORE_OK)
        {
            if (!node->result.stack.threadSignaled)
                status = NOTEPADS_MARKDOWN_CORE_INVALID_STATE;
            else
            {
                node->artifact = NativeDiagramArtifact::BuildAfterThreadExit(node->job);
                status = node->artifact.status;
                auto const &summary = node->artifact.summary;
                if (status == NOTEPADS_MARKDOWN_CORE_OK &&
                    (std::memcmp(&summary.identity, &node->key.identity, sizeof(summary.identity)) != 0 ||
                     summary.object != node->key.object))
                    status = NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE;
            }
        }
        if (status != NOTEPADS_MARKDOWN_CORE_OK && ClearNativeBytes(node))
        {
            auto state = notepads_markdown_diagram_job_state(node->job);
            if (state == NOTEPADS_MARKDOWN_DIAGRAM_QUEUED || state == NOTEPADS_MARKDOWN_DIAGRAM_ACTIVATED || state == NOTEPADS_MARKDOWN_DIAGRAM_COMPUTED ||
                state == NOTEPADS_MARKDOWN_DIAGRAM_FINISH_FAILED)
            {
                auto error = Error();
                if (notepads_markdown_diagram_job_retire_metadata(node->job, &error) != NOTEPADS_MARKDOWN_CORE_OK)
                    DeferRetirement(node, false);
            }
        }
        RetainPrepared(node);
        auto ready = status == NOTEPADS_MARKDOWN_CORE_OK;
        auto value = new (node->presentation) NativeDiagramPresentation(
            node->key, NativeDiagramPresentation::Classify(status), status, ready ? node->artifact.bytes : nullptr,
            ready ? node->artifact.capacity : 0, ready ? node->artifact.summary.width : 0,
            ready ? node->artifact.summary.height : 0,
            [](NativeDiagramPresentation *self) noexcept
            {
                auto owner = static_cast<PreparedNode *>(self->Owner());
                self->~NativeDiagramPresentation();
                ReleasePrepared(owner);
            },
            node);
        return DiagramPresentationHandle::Adopt(value);
    }
    HRESULT NativeDiagramWorker::Post(PreparedNode *node, NativeDiagramDeliverySink const &sink,
                                      DiagramPresentationHandle payload, DerivedDeliveryReceipt receipt) noexcept
    {
        RetainPrepared(node);
        return NativeDiagramTransport::PostEmbedded(sink, std::move(payload), node->transport, sizeof(node->transport),
                                                    node, [](void *owner) noexcept
                                                    { ReleasePrepared(static_cast<PreparedNode *>(owner)); }, std::move(receipt));
    }
    void *NativeDiagramWorker::AcquireStreamStorage(PreparedNode *node, size_t size, size_t alignment,
                                                    uint32_t *slot) noexcept
    {
        if (!node || !slot || size > NativeDiagramArtifact::StreamBytes || alignment > alignof(std::max_align_t))
            return nullptr;
        auto used = node->streamSlots.load(std::memory_order_acquire);
        for (;;)
        {
            uint32_t index{};
            while (index < NativeDiagramArtifact::StreamSlots && (used & (1u << index)))
                ++index;
            if (index == NativeDiagramArtifact::StreamSlots)
                return nullptr;
            if (node->streamSlots.compare_exchange_weak(used, used | (1u << index), std::memory_order_acq_rel))
            {
                RetainPrepared(node);
                *slot = index;
                return node->streams[index];
            }
        }
    }
    void NativeDiagramWorker::ReleaseStreamStorage(PreparedNode *node, uint32_t slot) noexcept
    {
        node->streamSlots.fetch_and(~(1u << slot), std::memory_order_release);
        ReleasePrepared(node);
    }
    uint32_t NativeDiagramWorker::LiveNodes() noexcept
    {
        return liveNodes.load(std::memory_order_acquire);
    }
    uint32_t NativeDiagramWorker::ReaperWaitFailures() noexcept
    {
        return waitFailures.load(std::memory_order_acquire);
    }
    uint32_t NativeDiagramWorker::CallbackPinFailures() noexcept
    {
        return callbackPinFailures.load(std::memory_order_acquire);
    }
    uint32_t NativeDiagramWorker::NodeFreeFailures() noexcept
    {
        return freeFailures.load(std::memory_order_acquire);
    }
    uint32_t NativeDiagramWorker::DeferredRetirements() noexcept
    {
        return deferredRetirements.load(std::memory_order_acquire);
    }
}
