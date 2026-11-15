// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Resources/Math/NativeMathWorker.h"
#include "Core/NativeThreadpool.h"
#include "Resources/Math/NativeMathText.h"
#include "Resources/Math/NativeMathCoordinator.h"
#include <new>

namespace Notepads::Markdown::Native
{
    namespace
    {
        constexpr SIZE_T InitialStackCommit = 1792 * 1024;
        constexpr uint64_t StackBudget = 2 * 1024 * 1024;
        // Bound the native entry/measurement frames and marker/alignment error.
        // This is not a Rust required-stack profile or permission to run parsing.
        constexpr uint64_t EntrySlack = 16 * 1024;
        std::atomic<uint32_t> liveNodes{};
        std::atomic<uint32_t> reaperWaitFailures{};
        std::atomic<uint32_t> callbackPinFailures{};
        std::atomic<uint32_t> nodeFreeFailures{};

        notepads_markdown_core_error Error() noexcept
        {
            return {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(notepads_markdown_core_error)};
        }

        struct WorkerNode
        {
            ModuleLifetime module;
            notepads_markdown_math_job *job{};
            NativeMathWorker::Completion completion{};
            void *userdata{};
            HANDLE thread{};
            PTP_WAIT wait{};
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            NativeMathWorkerTestHooks hooks;
#endif
            NativeMathWorkerResult result;
            bool prepared{};
        };
        static_assert(sizeof(WorkerNode) <= NOTEPADS_MARKDOWN_MATH_NATIVE_JOB_HEADER_BYTES);

    }
    struct NativeMathWorker::PreparedNode final : WorkerNode
    {
        enum class Phase : uint8_t
        {
            New,
            Queued,
            Starting,
            Running,
            Complete
        };
        std::atomic<uint32_t> references{1};
        std::atomic<Phase> phase{Phase::New};
        std::atomic<bool> canceled{};
        std::atomic_flag pumping = ATOMIC_FLAG_INIT;
        SRWLOCK timerLock = SRWLOCK_INIT;
        PTP_TIMER timer{};
        PreparedNode *next{};
        void *owner{};
        NativeMathKey key;
        winrt::com_ptr<ID2D1Factory> factory;
        uint64_t startedTicks{};
        uint32_t ordinal{};
        NativeMathWorker::Admission admission{};
        uint64_t presentation[(sizeof(NativeMathPresentation) + sizeof(uint64_t) - 1) / sizeof(uint64_t)]{};
        uint64_t transport[NativeMathTransport::EmbeddedBytes / sizeof(uint64_t)]{};
    };
    static_assert(sizeof(NativeMathWorker::PreparedNode) <= NOTEPADS_MARKDOWN_MATH_NATIVE_JOB_HEADER_BYTES);
    namespace
    {
        void CloseQueueTimer(NativeMathWorker::PreparedNode *node, PTP_CALLBACK_INSTANCE instance) noexcept;
        void CALLBACK PumpQueued(PTP_CALLBACK_INSTANCE instance, void *context, PTP_TIMER timer) noexcept;

        bool Writable(MEMORY_BASIC_INFORMATION const &info) noexcept
        {
            // Strictly accept ordinary RW stack pages only, and never a guard.
            return info.State == MEM_COMMIT && info.Protect == PAGE_READWRITE;
        }

        __declspec(noinline) NativeMathStackObservation MeasureStack(uintptr_t marker) noexcept
        {
            NativeMathStackObservation out{};
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
            // A Windows user stack is a single private allocation. Query all of
            // it, including reserve and guard, to account future commit capacity.
            while (cursor < upper)
            {
                MEMORY_BASIC_INFORMATION region{};
                if (VirtualQuery(reinterpret_cast<void const *>(cursor), &region, sizeof(region)) != sizeof(region))
                {
                    out.queryError = GetLastError();
                    return out;
                }
                auto start = reinterpret_cast<uintptr_t>(region.BaseAddress);
                if (region.AllocationBase != first.AllocationBase || start > cursor || region.RegionSize == 0 ||
                    region.RegionSize > UINTPTR_MAX - start)
                    return out;
                auto end = start + region.RegionSize;
                if (end <= cursor || end > upper)
                    return out;
                if (region.State == MEM_COMMIT)
                {
                    out.committedBytes += region.RegionSize;
                    if ((region.Protect & PAGE_GUARD) != 0)
                        out.guardBytes += region.RegionSize;
                }
                if (Writable(region))
                {
                    if (contiguousLower == 0)
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
            // Reserve is not headroom. Requiring its whole span <=2MiB also
            // bounds later guard growth for this fixed candidate configuration.
            out.withinStackBudget =
                out.measured && out.committedBytes <= StackBudget && out.allocationBytes <= StackBudget;
            return out;
        }

        __declspec(noinline) DWORD WINAPI WorkerEntry(void *context) noexcept
        {
            auto node = static_cast<WorkerNode *>(context);
            // Marker and notepads_markdown_math_job_run are in this same ordinary activation.
            volatile uint8_t marker{};
            node->result.stack = MeasureStack(reinterpret_cast<uintptr_t>(&marker));
            auto &stack = node->result.stack;
            bool forced{};
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            forced = node->hooks.forceInsufficientHeadroom;
#endif
            if (forced || !stack.measured || !stack.withinStackBudget ||
                stack.headroom == 0)
            {
                // Still enter the fixed Rust rejection entry with zero usable
                // headroom, so activated state can retire normally after exit.
                node->result.status = notepads_markdown_math_job_run(node->job, 0, &node->result.error);
            }
            else
                node->result.status =
                    notepads_markdown_math_job_run_with_text(node->job, stack.headroom, ShapeChineseMathText, &node->result.error);
            // Test-only tail deliberately separates run return from thread exit.
            // The scoped DirectWrite provider has returned. No application
            // callback or observer runs on this thread.
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            if (node->hooks.tailReached && node->hooks.tailPermit)
            {
                SetEvent(node->hooks.tailReached);
                node->result.tailWaitResult = WaitForSingleObjectEx(node->hooks.tailPermit, 10000, FALSE);
            }
#endif
            return 0;
        }

        // Caller keeps a ModuleLifetime across this function and its subsequent
        // Rust cleanup. A heap integrity failure retains both funding and a
        // permanent module pin; it cannot truthfully return the real capacity.
        bool FreeNodeStorage(WorkerNode *node) noexcept
        {
            node->~WorkerNode();
            if (HeapFree(GetProcessHeap(), 0, node))
                return true;
            ++winrt::get_module_lock();
            nodeFreeFailures.fetch_add(1, std::memory_order_relaxed);
            return false;
        }
        void CALLBACK Reap(PTP_CALLBACK_INSTANCE instance, void *context, PTP_WAIT wait,
                           TP_WAIT_RESULT waitResult) noexcept
        {
            if (!PinModuleUntilCallbackReturns(instance))
            {
                callbackPinFailures.fetch_add(1, std::memory_order_relaxed);
                return; // Keep the original Node/module pin and actual funding.
            }
            auto node = static_cast<WorkerNode *>(context);
            // Neither Computed nor an event is used as proof of thread exit.
            auto actualWait = WaitForSingleObjectEx(node->thread, 0, FALSE);
            if (waitResult != WAIT_OBJECT_0 || actualWait != WAIT_OBJECT_0)
            {
                reaperWaitFailures.fetch_add(1, std::memory_order_relaxed);
                // Preserve the real grant/node on failure; never pretend exit.
                // A failed handle probe is not safe to re-register. Keep its
                // pin and funding rather than passing an invalid handle again.
                if (actualWait == WAIT_TIMEOUT)
                    SetThreadpoolWait(wait, node->thread, nullptr);
                return;
            }
            ModuleLifetime cleanupModule;
            node->result.stack.threadSignaled = true;
            node->completion(node->userdata, node->job, node->result);
            auto job = node->job;
            auto thread = node->thread;
            SetThreadpoolWait(wait, nullptr, nullptr);
            // Detach this callback before draining; otherwise waiting on our
            // own callback would deadlock. The one-shot wait is never rearmed.
            DisassociateCurrentThreadFromCallback(instance);
            WaitForThreadpoolWaitCallbacks(wait, TRUE);
            CloseThreadpoolWait(wait);
            CloseHandle(thread);
            if (node->prepared)
            {
                auto prepared = static_cast<NativeMathWorker::PreparedNode *>(node);
                prepared->thread = nullptr;
                prepared->wait = nullptr;
                prepared->phase.store(NativeMathWorker::PreparedNode::Phase::Complete, std::memory_order_release);
                NativeMathWorker::ReleasePrepared(prepared);
                return;
            }
            if (!FreeNodeStorage(node))
                return;
            liveNodes.fetch_sub(1, std::memory_order_release);
            // Node allocation and actual worker stack are both gone first.
            auto error = Error();
            (void)notepads_markdown_math_job_release(job, &error);
            NativeMathCoordinator::NotifyResourcesAvailable();
        }

        NativeMathStartResult StartWorker(notepads_markdown_math_job *job, NativeMathWorker::Completion completion, void *userdata
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                                          , NativeMathWorkerTestHooks hooks = {}
#endif
                                          ) noexcept
        {
            ModuleLifetime invocation;
            if (!job || !completion || notepads_markdown_math_job_state(job) != NOTEPADS_MARKDOWN_MATH_ACTIVATED)
                return {NOTEPADS_MARKDOWN_CORE_INVALID_STATE, ERROR_INVALID_STATE};
            auto error = Error();
            auto status = notepads_markdown_math_job_claim_start(job, &error);
            if (status != NOTEPADS_MARKDOWN_CORE_OK)
                return {status, ERROR_INVALID_STATE};
            status = notepads_markdown_math_job_retain(job, &error);
            if (status != NOTEPADS_MARKDOWN_CORE_OK)
                return {status, 0};
            // Rust prepare/activate has prepaid the complete fixed 4096-byte
            // native node in this same job's Metadata family, including slack.
            // Request the exact prepaid capacity, including unused fixed tail
            // space, so post-exit finalization can count a known 4096 bytes.
            auto storage = HeapAlloc(GetProcessHeap(), 0, NOTEPADS_MARKDOWN_MATH_NATIVE_JOB_HEADER_BYTES);
            if (!storage)
            {
                (void)notepads_markdown_math_job_release(job, &error);
                return {NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY, ERROR_NOT_ENOUGH_MEMORY};
            }
            auto node = new (storage) WorkerNode{};
            node->job = job;
            node->completion = completion;
            node->userdata = userdata;
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            node->hooks = hooks;
#endif
            node->wait = CreateThreadpoolWait(Reap, node, nullptr);
            DWORD failure{};
            if (!node->wait)
                failure = GetLastError();
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            else if (hooks.simulatedCreateThreadError)
                failure = hooks.simulatedCreateThreadError;
#endif
            else
            {
                // All worker-visible fields are initialized first. The new
                // thread may run before CreateThread returns its handle.
                node->thread = CreateThread(nullptr, InitialStackCommit, WorkerEntry, node, 0, nullptr);
                if (!node->thread)
                    failure = GetLastError();
            }
            if (failure || !node->thread)
            {
                if (node->wait)
                    CloseThreadpoolWait(node->wait);
                if (!FreeNodeStorage(node))
                    return {NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT, ERROR_INVALID_DATA};
                (void)notepads_markdown_math_job_release(job, &error);
                return {NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY, failure ? failure : ERROR_NOT_ENOUGH_MEMORY};
            }
            liveNodes.fetch_add(1, std::memory_order_release);
            auto wait = node->wait;
            auto thread = node->thread;
            SetThreadpoolWait(wait, thread, nullptr);
            // It may already have reaped/deallocated node. Never touch it here.
            return {NOTEPADS_MARKDOWN_CORE_OK, 0};
        }

        void CloseQueueTimer(NativeMathWorker::PreparedNode *node, PTP_CALLBACK_INSTANCE instance) noexcept
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
            if (node->phase.load(std::memory_order_acquire) != NativeMathWorker::PreparedNode::Phase::New)
                NativeMathWorker::ReleasePrepared(node); // The armed timer's owner.
        }

        void CALLBACK PumpQueued(PTP_CALLBACK_INSTANCE instance, void *context, PTP_TIMER) noexcept
        {
            if (!PinModuleUntilCallbackReturns(instance))
            {
                callbackPinFailures.fetch_add(1, std::memory_order_relaxed);
                return; // Keep the original Node/module pin and actual funding.
            }
            // Closing/disassociating this timer can drop its Node owner; cover
            // the entire callback, including the tail after the final release.
            ModuleLifetime callbackModule;
            using Phase = NativeMathWorker::PreparedNode::Phase;
            auto node = static_cast<NativeMathWorker::PreparedNode *>(context);
            if (node->pumping.test_and_set(std::memory_order_acquire))
                return;
            // The timer owns a reference. This local one survives closing it.
            NativeMathWorker::RetainPrepared(node);
            int32_t status = NOTEPADS_MARKDOWN_CORE_OK;
            if (node->phase.load(std::memory_order_acquire) != Phase::Queued)
            {
                node->pumping.clear(std::memory_order_release);
                NativeMathWorker::ReleasePrepared(node);
                return;
            }
            auto error = Error();
            if (node->canceled.load(std::memory_order_acquire))
                status = NOTEPADS_MARKDOWN_CORE_CANCELLED;
            else if (GetTickCount64() - node->startedTicks >= 5000)
                status = NOTEPADS_MARKDOWN_CORE_DEADLINE;
            else if (node->admission && !node->admission(node->owner, node))
                status = NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT;
            else
                status = notepads_markdown_math_job_activate(node->job, &error);
            if (status == NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT)
            {
                // Retry this exact context/deadline; no prepare or self-wake.
                node->pumping.clear(std::memory_order_release);
                NativeMathWorker::ReleasePrepared(node);
                return;
            }
            node->phase.store(Phase::Starting, std::memory_order_release);
            CloseQueueTimer(node, instance);
            if (status == NOTEPADS_MARKDOWN_CORE_OK && node->canceled.load(std::memory_order_acquire))
                status = NOTEPADS_MARKDOWN_CORE_CANCELLED;
            if (status == NOTEPADS_MARKDOWN_CORE_OK)
            {
                node->wait = CreateThreadpoolWait(Reap, static_cast<WorkerNode *>(node), nullptr);
                if (!node->wait)
                    status = NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY;
                else
                {
                    node->phase.store(Phase::Running, std::memory_order_release);
                    // Add the actual reaper owner before a thread can run.
                    NativeMathWorker::RetainPrepared(node);
                    node->thread = CreateThread(nullptr, InitialStackCommit, WorkerEntry,
                                                static_cast<WorkerNode *>(node), 0, nullptr);
                    if (!node->thread)
                    {
                        NativeMathWorker::ReleasePrepared(node);
                        status = NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY;
                    }
                    else
                    {
                        auto wait = node->wait;
                        auto thread = node->thread;
                        node->pumping.clear(std::memory_order_release);
                        SetThreadpoolWait(wait, thread, nullptr);
                        NativeMathWorker::ReleasePrepared(node);
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
            node->phase.store(Phase::Complete, std::memory_order_release);
            // No thread ever existed; this is also exclusive terminal authority.
            node->completion(node, node->job, node->result);
            node->pumping.clear(std::memory_order_release);
            NativeMathWorker::ReleasePrepared(node);
        }
    }

    NativeMathRuntime &NativeMathRuntime::Shared() noexcept
    {
        static NativeMathRuntime runtime;
        return runtime;
    }

    NativeMathRuntime::~NativeMathRuntime()
    {
        auto error = Error();
        if (_runtime)
            (void)notepads_markdown_math_runtime_release(_runtime, &error);
    }

    int32_t NativeMathRuntime::CreateControl(notepads_markdown_math_control **output, notepads_markdown_core_error *error) noexcept
    {
        AcquireSRWLockExclusive(&_lock);
        int32_t status = NOTEPADS_MARKDOWN_CORE_OK;
        if (!_runtime)
            status = notepads_markdown_math_runtime_create(&_runtime, error);
        if (status == NOTEPADS_MARKDOWN_CORE_OK)
            status = notepads_markdown_math_control_create(_runtime, output, error);
        ReleaseSRWLockExclusive(&_lock);
        return status;
    }

    int32_t NativeMathRuntime::Usage(notepads_markdown_math_usage *output, notepads_markdown_core_error *error) noexcept
    {
        AcquireSRWLockShared(&_lock);
        auto status = _runtime ? notepads_markdown_math_runtime_usage(_runtime, output, error) : NOTEPADS_MARKDOWN_CORE_INVALID_STATE;
        ReleaseSRWLockShared(&_lock);
        return status;
    }

    NativeMathControl::~NativeMathControl()
    {
        auto error = Error();
        if (_control)
            (void)notepads_markdown_math_control_release(_control, &error);
        NativeMathCoordinator::NotifyResourcesAvailable();
    }

    int32_t NativeMathControl::Initialize(notepads_markdown_core_error *error) noexcept
    {
        return _control ? NOTEPADS_MARKDOWN_CORE_INVALID_STATE : NativeMathRuntime::Shared().CreateControl(&_control, error);
    }

    int32_t NativeMathControl::Usage(notepads_markdown_math_usage *output, notepads_markdown_core_error *error) noexcept
    {
        return notepads_markdown_math_control_usage(_control, output, error);
    }

    NativeMathStartResult NativeMathWorker::Start(notepads_markdown_math_job *job, Completion completion, void *userdata) noexcept
    {
        return StartWorker(job, completion, userdata);
    }

#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
    NativeMathStartResult NativeMathWorker::StartForTesting(notepads_markdown_math_job *job, Completion completion, void *userdata,
                                                            NativeMathWorkerTestHooks const &hooks) noexcept
    {
        return StartWorker(job, completion, userdata, hooks);
    }
#endif

    uint32_t NativeMathWorker::LiveNodes() noexcept
    {
        return liveNodes.load(std::memory_order_acquire);
    }
    uint32_t NativeMathWorker::CallbackPinFailures() noexcept
    {
        return callbackPinFailures.load(std::memory_order_acquire);
    }
    uint32_t NativeMathWorker::NodeFreeFailures() noexcept
    {
        return nodeFreeFailures.load(std::memory_order_acquire);
    }
    uint32_t NativeMathWorker::ReaperWaitFailures() noexcept
    {
        return reaperWaitFailures.load(std::memory_order_acquire);
    }

    int32_t NativeMathWorker::Prepare(notepads_markdown_math_control *control, notepads_markdown_core_model const *model, NativeMathKey const &key,
                                      uint32_t ordinal, ID2D1Factory *factory, void *owner, Completion completion,
                                      PreparedNode **output, notepads_markdown_core_error *error, Admission admission) noexcept
    {
        if (!output || !control || !model || !factory || !owner || !completion || !error)
            return NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT;
        *output = nullptr;
        // Anchor before FFI prepare, conservatively earlier than its deadline.
        auto started = GetTickCount64();
        notepads_markdown_math_job *job{};
        auto status = notepads_markdown_math_job_prepare(control, model, key.object, &job, error);
        if (status != NOTEPADS_MARKDOWN_CORE_OK)
            return status;
        status = notepads_markdown_math_job_claim_start(job, error);
        if (status != NOTEPADS_MARKDOWN_CORE_OK)
        {
            (void)notepads_markdown_math_job_release(job, error);
            return status;
        }
        auto storage = HeapAlloc(GetProcessHeap(), 0, NOTEPADS_MARKDOWN_MATH_NATIVE_JOB_HEADER_BYTES);
        if (!storage)
        {
            (void)notepads_markdown_math_job_release(job, error);
            return NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY;
        }
        auto node = new (storage) PreparedNode{};
        node->prepared = true;
        node->job = job;
        node->key = key;
        node->ordinal = ordinal;
        node->owner = owner;
        node->admission = admission;
        node->factory.copy_from(factory);
        node->completion = completion;
        node->userdata = node;
        node->startedTicks = started;
        node->timer = CreateThreadpoolTimer(PumpQueued, node, nullptr);
        if (!node->timer)
        {
            node->~PreparedNode();
            if (!HeapFree(GetProcessHeap(), 0, node))
            {
                ++winrt::get_module_lock();
                nodeFreeFailures.fetch_add(1, std::memory_order_relaxed);
                return NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT;
            }
            (void)notepads_markdown_math_job_release(job, error);
            return NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY;
        }
        liveNodes.fetch_add(1, std::memory_order_release);
        *output = node;
        return NOTEPADS_MARKDOWN_CORE_OK;
    }
    NativeMathStartResult NativeMathWorker::StartPrepared(PreparedNode *node) noexcept
    {
        if (!node)
            return {NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT, ERROR_INVALID_PARAMETER};
        auto expected = PreparedNode::Phase::New;
        if (!node->phase.compare_exchange_strong(expected, PreparedNode::Phase::Queued))
            return {NOTEPADS_MARKDOWN_CORE_INVALID_STATE, ERROR_INVALID_STATE};
        RetainPrepared(node); // One owner for timer registration and callbacks.
        WakePrepared(node);
        return {NOTEPADS_MARKDOWN_CORE_OK, 0};
    }
    void NativeMathWorker::RetainPrepared(PreparedNode *node) noexcept
    {
        node->references.fetch_add(1, std::memory_order_relaxed);
    }
    void NativeMathWorker::ReleasePrepared(PreparedNode *node) noexcept
    {
        if (node->references.fetch_sub(1, std::memory_order_acq_rel) != 1)
            return;
        ModuleLifetime tail;
        auto job = node->job;
        node->~PreparedNode();
        if (!HeapFree(GetProcessHeap(), 0, node))
        {
            ++winrt::get_module_lock();
            nodeFreeFailures.fetch_add(1, std::memory_order_relaxed);
            return;
        }
        liveNodes.fetch_sub(1, std::memory_order_release);
        auto error = Error();
        (void)notepads_markdown_math_job_release(job, &error);
        NativeMathCoordinator::NotifyResourcesAvailable();
    }
    void NativeMathWorker::CancelPrepared(PreparedNode *node) noexcept
    {
        node->canceled.store(true, std::memory_order_release);
        auto error = Error();
        (void)notepads_markdown_math_job_cancel(node->job, &error);
        WakePrepared(node);
    }
    void NativeMathWorker::WakePrepared(PreparedNode *node) noexcept
    {
        AcquireSRWLockExclusive(&node->timerLock);
        if (node->timer && node->phase.load(std::memory_order_acquire) == PreparedNode::Phase::Queued)
        {
            LARGE_INTEGER due{};
            due.QuadPart = -1;
            FILETIME time{due.LowPart, static_cast<DWORD>(due.HighPart)};
            SetThreadpoolTimer(node->timer, &time, 100, 0);
        }
        ReleaseSRWLockExclusive(&node->timerLock);
    }
    void NativeMathWorker::DestroyQueued(PreparedNode *node) noexcept
    {
        if (!node || node->phase.load(std::memory_order_acquire) != PreparedNode::Phase::New)
            return;
        CloseQueueTimer(node, nullptr); // Not armed: no timer reference yet.
        ReleasePrepared(node);
    }
    NativeMathWorker::PreparedNode *NativeMathWorker::NextPrepared(PreparedNode *node) noexcept
    {
        return node->next;
    }
    bool NativeMathWorker::IsQueued(PreparedNode *node) noexcept
    {
        return node->phase.load(std::memory_order_acquire) == PreparedNode::Phase::Queued;
    }
    void NativeMathWorker::SetNextPrepared(PreparedNode *node, PreparedNode *next) noexcept
    {
        node->next = next;
    }
    NativeMathKey const &NativeMathWorker::PreparedKey(PreparedNode *node) noexcept
    {
        return node->key;
    }
    uint32_t NativeMathWorker::PreparedOrdinal(PreparedNode *node) noexcept
    {
        return node->ordinal;
    }
    ID2D1Factory *NativeMathWorker::PreparedFactory(PreparedNode *node) noexcept
    {
        return node->factory.get();
    }
    void *NativeMathWorker::PreparedOwner(PreparedNode *node) noexcept
    {
        return node->owner;
    }
    void NativeMathWorker::ClearPreparedOwner(PreparedNode *node) noexcept
    {
        node->owner = nullptr;
    }
    MathPresentationHandle NativeMathWorker::FailurePresentation(PreparedNode *node, int32_t status) noexcept
    {
        RetainPrepared(node);
        auto value = new (node->presentation) NativeMathPresentation(
            node->key, NativeMathPresentation::Classify(status), status,
            [](NativeMathPresentation *self) noexcept
            {
                auto owner = static_cast<PreparedNode *>(self->Owner());
                self->~NativeMathPresentation();
                ReleasePrepared(owner);
            },
            node);
        return MathPresentationHandle::Adopt(value);
    }
    HRESULT NativeMathWorker::PostFailure(PreparedNode *node, NativeMathDeliverySink const &sink,
                                          MathPresentationHandle payload) noexcept
    {
        RetainPrepared(node);
        return NativeMathTransport::PostEmbedded(sink, std::move(payload), node->transport, sizeof(node->transport),
                                                 node, [](void *value) noexcept
                                                 { ReleasePrepared(static_cast<PreparedNode *>(value)); });
    }
}
