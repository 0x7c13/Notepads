// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Core/CoreController.h"
#include "Core/ModuleLifetime.h"
#include "notepads/markdown/preview.h"
#include <winrt/Windows.System.Threading.h>
#include <cassert>
#include <cstddef>
#include <limits>
#include <new>
#include <utility>

namespace Notepads::Markdown::Native
{
    namespace
    {
        template <typename T> T Pod() noexcept
        {
            T value{};
            value.header = {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, static_cast<uint32_t>(sizeof(T))};
            return value;
        }

        winrt::handle Event(bool manual = false)
        {
            winrt::handle result{
                ::CreateEventExW(nullptr, nullptr, manual ? CREATE_EVENT_MANUAL_RESET : 0, EVENT_ALL_ACCESS)};
            winrt::check_bool(static_cast<bool>(result));
            return result;
        }

        int32_t CurrentHresult() noexcept
        {
            return static_cast<int32_t>(winrt::to_hresult());
        }

        enum class Publication : uint8_t
        {
            Open,
            Canceled,
            Claimed
        };
        enum class RequestPhase : uint8_t
        {
            Ready,
            Running,
            AwaitUI,
            Decided
        };
        enum class Borrow : uint8_t
        {
            Awaiting,
            Entered,
            Acked,
            RejectedBeforeEntry
        };
    }

    struct CoreControllerLedger
    {
        std::atomic<uint32_t> states{};
        std::atomic<uint32_t> stateControlBlocks{};
        std::atomic<uint32_t> driverDelegates{};
        std::atomic<uint32_t> uiDelegates{};
        std::atomic<uint32_t> requestObjects{};
        std::atomic<uint32_t> requestControlBlocks{};
        std::atomic<uint32_t> completionControlBlocks{};
        std::atomic<uint32_t> modelPins{};
        std::atomic<uint32_t> controlHandles{};
        std::atomic<uint32_t> observationTickets{};
        std::atomic<int32_t> cleanupStatus{};
        std::atomic<int32_t> cleanupHresult{};
        std::mutex cleanupMutex;

        void Cleanup(int32_t status) noexcept
        {
            if (status != NOTEPADS_MARKDOWN_CORE_OK)
            {
                std::lock_guard guard(cleanupMutex);
                if (cleanupStatus.load() == 0 && cleanupHresult.load() == 0)
                {
                    cleanupStatus.store(status);
                }
            }
        }

        void CleanupHr(int32_t value) noexcept
        {
            if (value < 0)
            {
                std::lock_guard guard(cleanupMutex);
                if (cleanupStatus.load() == 0 && cleanupHresult.load() == 0)
                {
                    cleanupHresult.store(value);
                }
            }
        }

        CoreLifetimeSnapshot Snapshot() const noexcept
        {
            CoreLifetimeSnapshot value;
            value.states = states.load();
            value.stateControlBlocks = stateControlBlocks.load();
            value.driverDelegates = driverDelegates.load();
            value.uiDelegates = uiDelegates.load();
            value.requestObjects = requestObjects.load();
            value.requestControlBlocks = requestControlBlocks.load();
            value.completionControlBlocks = completionControlBlocks.load();
            value.modelPins = modelPins.load();
            value.controlHandles = controlHandles.load();
            value.observationTickets = observationTickets.load();
            value.observationAllocationBlocks = 1;
            value.cleanupStatus = cleanupStatus.load();
            value.cleanupHresult = cleanupHresult.load();
            value.fullyReleased = value.states == 0 && value.stateControlBlocks == 0 && value.driverDelegates == 0 &&
                                  value.uiDelegates == 0 && value.requestObjects == 0 &&
                                  value.requestControlBlocks == 0 && value.completionControlBlocks == 0 &&
                                  value.modelPins == 0 && value.controlHandles == 0;
            return value;
        }
    };

    struct CoreCompletionRecord
    {
        ModuleLifetime module;
        winrt::handle event{Event(true)};
        std::weak_ptr<void> request;
        void (*cancelRequest)(std::shared_ptr<void> const &) noexcept {};
        uint64_t serial{};
        std::atomic<bool> done{};
        int32_t status{NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
        notepads_markdown_core_error error{Pod<notepads_markdown_core_error>()};

        void Complete(int32_t terminal, notepads_markdown_core_error const &detail) noexcept
        {
            // Cleanup serializes terminal publication through the request gate.
            if (done.load(std::memory_order_acquire))
                return;
            status = terminal;
            error = detail;
            error.status = terminal;
            done.store(true, std::memory_order_release);
            ::SetEvent(event.get());
        }
    };

    CoreCompletionTicket::CoreCompletionTicket(std::shared_ptr<CoreCompletionRecord> record) noexcept
        : _record(std::move(record))
    {
    }
    HANDLE CoreCompletionTicket::EventHandle() const noexcept
    {
        return _record ? _record->event.get() : nullptr;
    }
    uint64_t CoreCompletionTicket::Serial() const noexcept
    {
        return _record ? _record->serial : 0;
    }
    bool CoreCompletionTicket::Done() const noexcept
    {
        return _record && _record->done.load(std::memory_order_acquire);
    }
    int32_t CoreCompletionTicket::Status() const noexcept
    {
        return Done() ? _record->status : NOTEPADS_MARKDOWN_CORE_INVALID_STATE;
    }
    notepads_markdown_core_error CoreCompletionTicket::Error() const noexcept
    {
        return Done() ? _record->error : Pod<notepads_markdown_core_error>();
    }
    void CoreCompletionTicket::Cancel() const noexcept
    {
        if (_record && !Done())
        {
            if (auto request = _record->request.lock())
            {
                _record->cancelRequest(request);
            }
        }
    }

    namespace
    {
        enum class AllocationOwner : uint8_t
        {
            Observation,
            State,
            DriverDelegate,
            UiDelegate,
            ControlHandle,
            Request,
            Completion
        };

        struct AllocationFailure
        {
            int32_t status;
            notepads_markdown_core_error error;
        };

        struct alignas(MEMORY_ALLOCATION_ALIGNMENT) AllocationPrefix
        {
            ModuleLifetime module;
            notepads_markdown_core_allocation *lease;
            std::shared_ptr<CoreControllerLedger> ledger;
            AllocationOwner owner;
            std::array<std::byte, MEMORY_ALLOCATION_ALIGNMENT - 1> tailPadding{};
        };
        static_assert(sizeof(AllocationOwner) == 1);
        static_assert(alignof(AllocationPrefix) == MEMORY_ALLOCATION_ALIGNMENT);
        static_assert(sizeof(AllocationPrefix) == (sizeof(void *) == 8 ? 48 : 24));

        std::atomic<uint32_t> *AllocationCounter(CoreControllerLedger &ledger, AllocationOwner owner) noexcept
        {
            switch (owner)
            {
            case AllocationOwner::State:
                return &ledger.stateControlBlocks;
            case AllocationOwner::DriverDelegate:
                return &ledger.driverDelegates;
            case AllocationOwner::UiDelegate:
                return &ledger.uiDelegates;
            case AllocationOwner::ControlHandle:
                return &ledger.controlHandles;
            case AllocationOwner::Request:
                return &ledger.requestControlBlocks;
            case AllocationOwner::Observation:
                return nullptr;
            case AllocationOwner::Completion:
                return &ledger.completionControlBlocks;
            }
            return nullptr;
        }

        void FreeAllocation(void *pointer) noexcept
        {
            // The allocation prefix pins this DLL through the last weak release.
            // A local pin covers code/FFI after its native memory has been freed.
            ModuleLifetime module;
            auto prefix = reinterpret_cast<AllocationPrefix *>(pointer) - 1;
            auto lease = prefix->lease;
            auto ledger = prefix->ledger;
            auto owner = prefix->owner;
            prefix->~AllocationPrefix();
            if (::HeapFree(::GetProcessHeap(), 0, prefix))
            {
                auto status = notepads_markdown_core_allocation_destroy(lease);
                if (status != NOTEPADS_MARKDOWN_CORE_OK)
                {
                    // The native prefix is gone; an uncertain Rust owner may
                    // still need this DLL even when no observation ledger lives.
                    ++winrt::get_module_lock();
                }
                if (ledger)
                {
                    ledger->Cleanup(status);
                    if (auto counter = AllocationCounter(*ledger, owner))
                    {
                        --*counter;
                    }
                }
                // Observation storage has no self-reference. Its last free/lease
                // status occurs after the final observer and is not reported as
                // previously observed Rust quota zero or physical-memory release.
            }
            else
            {
                // Reconstitute the prefix on the still-live native allocation.
                // Neither its lease nor native live counter may be retired.
                ::new (prefix) AllocationPrefix{{}, lease, ledger, owner};
                if (ledger)
                {
                    ledger->CleanupHr(E_UNEXPECTED);
                }
            }
        }

        // One allocator for known init storage and update allocate_shared. Init
        // uses an independent Preparing-only job; update always passes its own job.
        // The copied job pointer is never dereferenced during final deallocation.
        template <typename T> struct HeapAllocator
        {
            using value_type = T;
            notepads_markdown_core_job *job;
            std::shared_ptr<CoreControllerLedger> ledger;
            AllocationOwner owner;
            HeapAllocator(notepads_markdown_core_job *value, std::shared_ptr<CoreControllerLedger> observer,
                          AllocationOwner kind) noexcept
                : job(value), ledger(std::move(observer)), owner(kind)
            {
            }
            template <typename U>
            HeapAllocator(HeapAllocator<U> const &other) noexcept
                : job(other.job), ledger(other.ledger), owner(other.owner)
            {
            }
            T *allocate(size_t count)
            {
                static_assert(alignof(T) <= MEMORY_ALLOCATION_ALIGNMENT);
                if (count > (std::numeric_limits<size_t>::max() - sizeof(AllocationPrefix)) / sizeof(T))
                {
                    auto error = Pod<notepads_markdown_core_error>();
                    error.status = NOTEPADS_MARKDOWN_CORE_ARITHMETIC_OVERFLOW;
                    throw AllocationFailure{error.status, error};
                }
                auto requested = sizeof(AllocationPrefix) + count * sizeof(T);
                auto error = Pod<notepads_markdown_core_error>();
                notepads_markdown_core_allocation *lease{};
                auto status = notepads_markdown_core_job_reserve_allocation(job, requested, &lease, &error);
                if (status != NOTEPADS_MARKDOWN_CORE_OK)
                {
                    throw AllocationFailure{status, error};
                }
                auto memory = ::HeapAlloc(::GetProcessHeap(), 0, requested);
                if (memory == nullptr)
                {
                    status = notepads_markdown_core_job_allocation_failed(job, requested, &error);
                    auto cleanup = notepads_markdown_core_allocation_destroy(lease);
                    if (ledger)
                    {
                        ledger->Cleanup(cleanup);
                    }
                    throw AllocationFailure{status, error};
                }
                auto prefix = ::new (memory) AllocationPrefix{{}, lease, ledger, owner};
                if (ledger)
                {
                    if (auto counter = AllocationCounter(*ledger, owner))
                    {
                        ++*counter;
                    }
                }
                auto actual = ::HeapSize(::GetProcessHeap(), 0, memory);
                if (actual == static_cast<SIZE_T>(-1))
                {
                    FreeAllocation(prefix + 1);
                    error.status = NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT;
                    throw AllocationFailure{error.status, error};
                }
                status = notepads_markdown_core_job_admit_allocation(job, lease, actual, &error);
                if (status != NOTEPADS_MARKDOWN_CORE_OK)
                {
                    FreeAllocation(prefix + 1);
                    throw AllocationFailure{status, error};
                }
                return reinterpret_cast<T *>(prefix + 1);
            }
            void deallocate(T *pointer, size_t) noexcept
            {
                FreeAllocation(pointer);
            }
            template <typename U> bool operator==(HeapAllocator<U> const &other) const noexcept
            {
                return job == other.job && ledger == other.ledger && owner == other.owner;
            }
        };

        struct Request
        {
            uint64_t serial;
            notepads_markdown_core_job *job;
            notepads_markdown_core_cancel *cancel;
            std::shared_ptr<CoreControllerLedger> ledger;
            std::mutex cancelMutex;
            Publication publication{Publication::Open};
            std::atomic<bool> canceled{};
            RequestPhase phase{RequestPhase::Ready};
            Borrow borrow{Borrow::Awaiting};
            int32_t status{};
            notepads_markdown_core_error error{Pod<notepads_markdown_core_error>()};
            std::shared_ptr<CoreCompletionRecord> completion;

            Request(uint64_t number, notepads_markdown_core_job *work, notepads_markdown_core_cancel *flag,
                    std::shared_ptr<CoreControllerLedger> owner) noexcept
                : serial(number), job(work), cancel(flag), ledger(std::move(owner))
            {
                ++ledger->requestObjects;
            }
            ~Request()
            {
                Cleanup();
                --ledger->requestObjects;
            }
            bool Cancel() noexcept
            {
                std::lock_guard guard(cancelMutex);
                if (publication == Publication::Open)
                {
                    publication = Publication::Canceled;
                    canceled.store(true, std::memory_order_release);
                    if (cancel != nullptr)
                    {
                        ledger->Cleanup(notepads_markdown_core_cancel_request(cancel));
                    }
                    return true;
                }
                return publication == Publication::Canceled;
            }
            bool Claim() noexcept
            {
                std::lock_guard guard(cancelMutex);
                if (publication != Publication::Open)
                {
                    return false;
                }
                publication = Publication::Claimed;
                return true;
            }
            bool Cleanup() noexcept
            {
                bool clean = true;
                // Withdrawal and cancel writes use the same lock. No job borrow
                // exists here: the sole owner proved run-return and UI acknowledgement.
                std::lock_guard guard(cancelMutex);
                auto oldCancel = std::exchange(cancel, nullptr);
                auto oldJob = std::exchange(job, nullptr);
                if (oldJob != nullptr)
                {
                    auto statusValue = notepads_markdown_core_job_destroy(oldJob);
                    ledger->Cleanup(statusValue);
                    clean = clean && statusValue == NOTEPADS_MARKDOWN_CORE_OK;
                }
                if (oldCancel != nullptr)
                {
                    auto statusValue = notepads_markdown_core_cancel_destroy(oldCancel);
                    ledger->Cleanup(statusValue);
                    clean = clean && statusValue == NOTEPADS_MARKDOWN_CORE_OK;
                }
                if (completion)
                {
                    auto terminal = status;
                    if (terminal == NOTEPADS_MARKDOWN_CORE_OK && canceled.load(std::memory_order_acquire))
                    {
                        terminal = NOTEPADS_MARKDOWN_CORE_CANCELLED;
                    }
                    if (terminal == NOTEPADS_MARKDOWN_CORE_OK && !clean)
                    {
                        terminal = NOTEPADS_MARKDOWN_CORE_INVALID_STATE;
                    }
                    completion->Complete(terminal, error);
                }
                return clean;
            }
        };

        struct DelegatePin
        {
            ModuleLifetime module;
        };
    }

    struct CoreControllerState
    {
        ModuleLifetime module;
        std::shared_ptr<CoreControllerLedger> ledger;
        mutable std::mutex mutex;
        std::mutex preparingCancelMutex;
        winrt::handle changed{Event()};
        // Each auto-reset event has exactly one consumer. Drain must never take
        // the driver's last Stop/EndBorrow wake before its INFINITE idle wait.
        winrt::handle drainChanged{Event()};
        winrt::Windows::UI::Core::CoreDispatcher dispatcher{nullptr};
        winrt::Windows::UI::Core::DispatchedHandler uiHandler{nullptr};
        winrt::Windows::Foundation::IAsyncAction uiAction{nullptr};
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        std::shared_ptr<CoreControllerTestHooks> hooks;
#endif
        DWORD uiThreadId{};
        notepads_markdown_core_session *session{};
        notepads_markdown_core_model *installed{};
        std::shared_ptr<Request> active;
        std::shared_ptr<Request> pending;
        bool closed{};
        bool preparing{};
        bool preparingIntent{}; // preparingCancelMutex, with State lock for slot transfer
        notepads_markdown_core_cancel *preparingCancel{};
        uint32_t submitBorrows{};
        bool installEntered{};
        uint64_t serial{};
        uint64_t lastInstalled{};
        uint64_t retired{};
        uint64_t runs{};
        uint64_t installs{};
        uint32_t driverStarts{};
        CoreWakePhase wake{CoreWakePhase::Idle};
        uint64_t wakeGeneration{};
        uint64_t wakeSerial{};
        bool wakeEntered{};
        bool wakeAck{};
        uint64_t postingEntries{};
        uint64_t ackWhileStarted{};
        uint64_t rejectedBeforeEntry{};
        bool coreDrained{};
        bool driverExited{};
        bool canceled{};
        bool coreCleanupFailed{};
        int32_t primaryStatus{};
        int32_t primaryHresult{};
        notepads_markdown_core_submit_result sourceState{Pod<notepads_markdown_core_submit_result>()};

        explicit CoreControllerState(std::shared_ptr<CoreControllerLedger> owner) : ledger(std::move(owner))
        {
            ++ledger->states;
        }
        ~CoreControllerState()
        {
            // Initialization failure is the only path with a session but no driver.
            if (installed != nullptr)
            {
                ledger->Cleanup(notepads_markdown_core_model_destroy(installed));
            }
            if (session != nullptr)
            {
                ledger->Cleanup(notepads_markdown_core_session_destroy(session));
            }
            --ledger->states;
        }
        void Signal() noexcept
        {
            ::SetEvent(changed.get());
            ::SetEvent(drainChanged.get());
        }
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        void Hook(CoreTestPoint point) noexcept
        {
            if (hooks)
            {
                hooks->Reach(point);
            }
        }
#endif
        void Primary(int32_t status) noexcept // State mutex held
        {
            if (status == NOTEPADS_MARKDOWN_CORE_CANCELLED)
            {
                canceled = true;
            }
            else if (status != NOTEPADS_MARKDOWN_CORE_OK && primaryStatus == NOTEPADS_MARKDOWN_CORE_OK && primaryHresult == 0)
            {
                primaryStatus = status;
            }
        }
        void PrimaryHr(int32_t hr) noexcept // State mutex held
        {
            if (hr < 0 && primaryHresult == 0 && primaryStatus == NOTEPADS_MARKDOWN_CORE_OK)
            {
                primaryHresult = hr;
            }
        }
    };

    namespace
    {
        void CancelState(std::shared_ptr<CoreControllerState> const &state, bool stop) noexcept
        {
            std::shared_ptr<Request> active;
            std::shared_ptr<Request> pending;
            std::unique_lock stateLock(state->mutex);
            if (stop)
            {
                state->closed = true;
            }
            active = state->active;
            pending = state->pending;
            std::unique_lock preparingLock(state->preparingCancelMutex);
            bool cancellationWon = false;
            if (state->preparing)
            {
                state->preparingIntent = true;
                cancellationWon = true;
            }
            auto preparingCancel = state->preparingCancel;
            stateLock.unlock();
            if (preparingCancel != nullptr)
            {
                state->ledger->Cleanup(notepads_markdown_core_cancel_request(preparingCancel));
            }
            preparingLock.unlock();
            if (active)
            {
                cancellationWon = active->Cancel() || cancellationWon;
            }
            if (pending)
            {
                cancellationWon = pending->Cancel() || cancellationWon;
            }
            if (cancellationWon)
            {
                std::lock_guard guard(state->mutex);
                state->canceled = true;
            }
            state->Signal();
        }

        void UiBody(std::weak_ptr<CoreControllerState> const &weak) noexcept
        {
            auto state = weak.lock();
            if (!state)
            {
                return;
            }
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            state->Hook(CoreTestPoint::UiBeforeEntry);
#endif
            std::shared_ptr<Request> request;
            uint64_t generation{};
            notepads_markdown_core_session *session{};
            {
                std::lock_guard guard(state->mutex);
                generation = state->wakeGeneration;
                state->wakeEntered = true;
                if (state->wake == CoreWakePhase::Posting)
                {
                    ++state->postingEntries;
                }
                if (!state->closed && state->active && state->active->serial == state->wakeSerial &&
                    state->active->phase == RequestPhase::AwaitUI && state->active->borrow == Borrow::Awaiting)
                {
                    request = state->active;
                    request->borrow = Borrow::Entered;
                    state->installEntered = true;
                    session = state->session;
                }
                else
                {
                    state->wakeAck = true;
                }
            }
            if (!request)
            {
                state->Signal();
                return;
            }
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            state->Hook(CoreTestPoint::UiBeforeClaim);
#endif
            notepads_markdown_core_model *model{};
            auto error = Pod<notepads_markdown_core_error>();
            int32_t status = NOTEPADS_MARKDOWN_CORE_CANCELLED;
            if (request->Claim())
            {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                state->Hook(CoreTestPoint::UiAfterClaim);
#endif
                status = notepads_markdown_core_job_install(session, request->job, &model, &error);
            }
            notepads_markdown_core_model *old{};
            {
                std::lock_guard guard(state->mutex);
                if (status == NOTEPADS_MARKDOWN_CORE_OK)
                {
                    old = std::exchange(state->installed, model);
                    state->lastInstalled = request->serial;
                    ++state->installs;
                }
                else
                {
                    state->Primary(status);
                }
                request->status = status;
                request->error = error;
            }
            if (old != nullptr)
            {
                auto cleanup = notepads_markdown_core_model_destroy(old);
                state->ledger->Cleanup(cleanup);
                if (cleanup != NOTEPADS_MARKDOWN_CORE_OK)
                {
                    {
                        std::lock_guard guard(state->mutex);
                        state->coreCleanupFailed = true;
                    }
                    CancelState(state, true);
                }
            }
            {
                std::lock_guard guard(state->mutex);
                request->phase = RequestPhase::Decided;
                request->borrow = Borrow::Acked;
                state->installEntered = false;
                if (state->wakeGeneration == generation)
                {
                    state->wakeAck = true;
                }
            }
            state->Signal();
            // Deliberately outside the acknowledged Core borrow. A platform action
            // may still be Started here; the driver must observe its real terminal.
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            state->Hook(CoreTestPoint::UiAfterAck);
#endif
        }

        void RejectAwaiting(CoreControllerState &state, Request &request, int32_t status) noexcept
        {
            if (request.phase == RequestPhase::AwaitUI && request.borrow == Borrow::Awaiting)
            {
                request.borrow = Borrow::RejectedBeforeEntry;
                request.phase = RequestPhase::Decided;
                request.status = status;
                ++state.rejectedBeforeEntry;
                state.Primary(status);
            }
        }

        void ObserveWake(std::shared_ptr<CoreControllerState> const &state) noexcept
        {
            winrt::Windows::Foundation::IAsyncAction action{nullptr};
            uint64_t generation{};
            {
                std::lock_guard guard(state->mutex);
                if (state->closed || state->wake != CoreWakePhase::InFlight)
                {
                    return;
                }
                action = state->uiAction;
                generation = state->wakeGeneration;
            }
            int32_t hr{};
            bool terminal = false;
            bool statusObserved = false;
            try
            {
                terminal = action.Status() != winrt::Windows::Foundation::AsyncStatus::Started;
                statusObserved = true;
                if (!terminal)
                {
                    std::lock_guard guard(state->mutex);
                    if (state->wakeAck)
                    {
                        ++state->ackWhileStarted;
                    }
                    return;
                }
                action.GetResults();
            }
            catch (...)
            {
                hr = CurrentHresult();
                // A Status failure does not prove terminal. Stop, retaining the
                // platform tail rather than recycling a possibly live wake slot.
                if (!statusObserved)
                {
                    {
                        std::lock_guard guard(state->mutex);
                        state->PrimaryHr(hr);
                    }
                    CancelState(state, true);
                    return;
                }
            }
            if (terminal)
            {
                bool closeFailed = false;
                try
                {
                    action.Close();
                }
                catch (...)
                {
                    state->ledger->CleanupHr(CurrentHresult());
                    closeFailed = true;
                }
                bool dispatchFailed = hr < 0 || closeFailed;
                {
                    std::lock_guard guard(state->mutex);
                    if (state->wakeGeneration == generation)
                    {
                        state->PrimaryHr(hr);
                        if (!state->wakeEntered)
                        {
                            dispatchFailed = true;
                            if (state->active && state->active->serial == state->wakeSerial)
                            {
                                RejectAwaiting(*state, *state->active, NOTEPADS_MARKDOWN_CORE_INVALID_STATE);
                            }
                            state->PrimaryHr(hr == 0 ? E_UNEXPECTED : hr);
                        }
                        state->uiAction = nullptr;
                        state->wake = CoreWakePhase::Idle;
                    }
                }
                if (dispatchFailed)
                {
                    // No new wake may reuse a handler after an exceptional
                    // platform dispatch whose retained late invocation is unknown.
                    CancelState(state, true);
                }
            }
        }

        void PostWake(std::shared_ptr<CoreControllerState> const &state) noexcept
        {
            uint64_t generation{};
            {
                std::lock_guard guard(state->mutex);
                if (state->closed || state->wake != CoreWakePhase::Idle || !state->active ||
                    state->active->phase != RequestPhase::AwaitUI || state->active->borrow != Borrow::Awaiting)
                {
                    return;
                }
                if (state->wakeGeneration == std::numeric_limits<uint64_t>::max())
                {
                    state->Primary(NOTEPADS_MARKDOWN_CORE_ARITHMETIC_OVERFLOW);
                    state->closed = true;
                    state->Signal();
                    return;
                }
                generation = ++state->wakeGeneration;
                state->wakeSerial = state->active->serial;
                state->wakeEntered = false;
                state->wakeAck = false;
                state->wake = CoreWakePhase::Posting;
            }
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            state->Hook(CoreTestPoint::DriverBeforePost);
#endif
            {
                std::lock_guard guard(state->mutex);
                if (state->closed)
                {
                    state->wake = CoreWakePhase::Idle;
                    return;
                }
            }
            try
            {
                auto action = state->dispatcher.RunAsync(winrt::Windows::UI::Core::CoreDispatcherPriority::Normal,
                                                         state->uiHandler);
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                state->Hook(CoreTestPoint::DriverAfterPost);
#endif
                std::lock_guard guard(state->mutex);
                state->uiAction = std::move(action);
                state->wake = CoreWakePhase::InFlight;
            }
            catch (...)
            {
                auto hr = CurrentHresult();
                std::shared_ptr<Request> rejected;
                {
                    std::lock_guard guard(state->mutex);
                    state->PrimaryHr(hr);
                    if (state->wakeGeneration == generation && state->active &&
                        state->active->serial == state->wakeSerial)
                    {
                        rejected = state->active;
                        RejectAwaiting(*state, *state->active, NOTEPADS_MARKDOWN_CORE_INVALID_STATE);
                    }
                    state->wake = CoreWakePhase::Idle;
                }
                if (rejected)
                {
                    // A post may throw after its handler has already Entered.
                    // Compete with Claim through the same gate, then wait its ack.
                    rejected->Cancel();
                }
                CancelState(state, true);
            }
            state->Signal();
        }

        void Driver(std::shared_ptr<CoreControllerState> const &state) noexcept
        {
            {
                std::lock_guard guard(state->mutex);
                ++state->driverStarts;
            }
            for (;;)
            {
                ObserveWake(state);
                std::shared_ptr<Request> cleanup;
                std::shared_ptr<Request> run;
                bool stopReady = false;
                {
                    std::lock_guard guard(state->mutex);
                    if (state->active && state->active->phase == RequestPhase::AwaitUI &&
                        (state->closed || state->active->canceled.load(std::memory_order_acquire)))
                    {
                        RejectAwaiting(*state, *state->active, NOTEPADS_MARKDOWN_CORE_CANCELLED);
                    }
                    if (state->active && state->active->phase == RequestPhase::Decided &&
                        state->active->borrow != Borrow::Entered)
                    {
                        cleanup = state->active;
                    }
                    else if (state->closed)
                    {
                        if (state->submitBorrows == 0 && !state->installEntered && !state->active)
                        {
                            if (state->pending)
                            {
                                cleanup = std::move(state->pending);
                            }
                            else
                            {
                                stopReady = true;
                            }
                        }
                    }
                    else if (!state->active && state->pending)
                    {
                        state->active = std::move(state->pending);
                        run = state->active;
                        run->phase = RequestPhase::Running;
                    }
                }
                if (cleanup)
                {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                    state->Hook(CoreTestPoint::RequestBeforeCleanup);
#endif
                    cleanup->Cancel();
                    auto clean = cleanup->Cleanup();
                    {
                        std::lock_guard guard(state->mutex);
                        if (state->active == cleanup)
                        {
                            // Keep active visible until run, UI borrow and actual
                            // Rust handle destruction have all completed.
                            state->active.reset();
                        }
                        ++state->retired;
                        state->coreCleanupFailed = state->coreCleanupFailed || !clean;
                    }
                    cleanup.reset();
                    if (!clean)
                    {
                        CancelState(state, true);
                    }
                    continue;
                }
                if (run)
                {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                    state->Hook(CoreTestPoint::DriverBeforeRun);
#endif
                    auto error = Pod<notepads_markdown_core_error>();
                    auto status = notepads_markdown_core_job_run(run->job, &error);
                    {
                        std::lock_guard guard(state->mutex);
                        ++state->runs;
                        run->status = status;
                        run->error = error;
                        if (status == NOTEPADS_MARKDOWN_CORE_OK)
                        {
                            run->phase = RequestPhase::AwaitUI;
                        }
                        else
                        {
                            run->phase = RequestPhase::Decided;
                            run->borrow = Borrow::RejectedBeforeEntry;
                            state->Primary(status);
                        }
                    }
                    continue;
                }
                if (stopReady)
                {
                    notepads_markdown_core_session *session{};
                    notepads_markdown_core_model *model{};
                    winrt::Windows::Foundation::IAsyncAction action{nullptr};
                    {
                        std::lock_guard guard(state->mutex);
                        session = std::exchange(state->session, nullptr);
                        model = std::exchange(state->installed, nullptr);
                        action = std::move(state->uiAction);
                        state->wake = action ? CoreWakePhase::Tail : CoreWakePhase::Idle;
                    }
                    bool clean = true;
                    if (model != nullptr)
                    {
                        auto status = notepads_markdown_core_model_destroy(model);
                        state->ledger->Cleanup(status);
                        clean = clean && status == NOTEPADS_MARKDOWN_CORE_OK;
                    }
                    if (session != nullptr)
                    {
                        auto status = notepads_markdown_core_session_destroy(session);
                        state->ledger->Cleanup(status);
                        clean = clean && status == NOTEPADS_MARKDOWN_CORE_OK;
                    }
                    if (action)
                    {
                        try
                        {
                            action.Cancel();
                        }
                        catch (...)
                        {
                            state->ledger->CleanupHr(CurrentHresult());
                        }
                        try
                        {
                            action.Close();
                        }
                        catch (...)
                        {
                            state->ledger->CleanupHr(CurrentHresult());
                        }
                    }
                    {
                        std::lock_guard guard(state->mutex);
                        state->uiHandler = nullptr;
                        state->coreDrained = clean && !state->coreCleanupFailed;
                    }
                    state->Signal();
                    return;
                }
                PostWake(state);
                bool waitTimed = false;
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                bool waitForSubmit = false;
#endif
                {
                    std::lock_guard guard(state->mutex);
                    waitTimed = state->wake == CoreWakePhase::InFlight;
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                    waitForSubmit = state->closed && state->submitBorrows != 0;
#endif
                }
                // The permanent timeout is necessary even after a body ack: the
                // platform may still report Started and signal no further event.
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                if (!waitTimed)
                {
                    state->Hook(CoreTestPoint::DriverBeforeIdleWait);
                    if (waitForSubmit)
                    {
                        state->Hook(CoreTestPoint::DriverBeforeSubmitWait);
                    }
                }
#endif
                auto waited = ::WaitForSingleObjectEx(state->changed.get(), waitTimed ? 10 : INFINITE, FALSE);
                if (waited == WAIT_FAILED)
                {
                    {
                        std::lock_guard guard(state->mutex);
                        state->PrimaryHr(HRESULT_FROM_WIN32(::GetLastError()));
                    }
                    CancelState(state, true);
                }
            }
        }

        template <typename Prepare>
        CoreSubmitResult Submit(std::shared_ptr<CoreControllerState> const &state, Prepare prepare,
                                CoreCompletionTicket *completion = nullptr) noexcept
        {
            if (completion)
                *completion = {};
            CoreSubmitResult result;
            result.error = Pod<notepads_markdown_core_error>();
            result.sourceState = Pod<notepads_markdown_core_submit_result>();
            try
            {
                if (!state->dispatcher.HasThreadAccess())
                {
                    result.status = NOTEPADS_MARKDOWN_CORE_INVALID_STATE;
                    result.error.status = result.status;
                    return result;
                }
            }
            catch (...)
            {
                std::lock_guard guard(state->mutex);
                state->PrimaryHr(CurrentHresult());
                result.status = NOTEPADS_MARKDOWN_CORE_INVALID_STATE;
                result.error.status = result.status;
                return result;
            }
            std::shared_ptr<Request> oldPending;
            std::shared_ptr<Request> active;
            {
                std::unique_lock guard(state->mutex);
                if (state->closed || state->submitBorrows != 0 || state->installEntered)
                {
                    result.status = NOTEPADS_MARKDOWN_CORE_INVALID_STATE;
                    result.error.status = result.status;
                    return result;
                }
                if (state->serial == std::numeric_limits<uint64_t>::max())
                {
                    state->Primary(NOTEPADS_MARKDOWN_CORE_ARITHMETIC_OVERFLOW);
                    state->closed = true;
                    state->Signal();
                    result.status = NOTEPADS_MARKDOWN_CORE_ARITHMETIC_OVERFLOW;
                    result.error.status = result.status;
                    guard.unlock();
                    CancelState(state, true);
                    return result;
                }
                result.serial = ++state->serial;
                ++state->submitBorrows;
                state->preparing = true;
                std::lock_guard preparingGuard(state->preparingCancelMutex);
                state->preparingIntent = false;
                oldPending = std::move(state->pending);
                active = state->active;
            }
            // This entire scope, including old-pending destruction, owns the
            // Submit borrow. Stop cannot take its session or pending ownership.
            struct EndBorrow
            {
                std::shared_ptr<CoreControllerState> const &state;
                ~EndBorrow()
                {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                    state->Hook(CoreTestPoint::SubmitBeforeEndBorrow);
#endif
                    std::lock_guard guard(state->mutex);
                    state->preparing = false;
                    --state->submitBorrows;
                    state->Signal();
                }
            } end{state};
            if (active)
            {
                active->Cancel();
            }
            state->Signal();
            if (oldPending)
            {
                oldPending->Cancel();
                auto clean = oldPending->Cleanup();
                oldPending.reset();
                std::lock_guard guard(state->mutex);
                ++state->retired;
                if (!clean)
                {
                    state->coreCleanupFailed = true;
                    state->closed = true;
                    result.status = state->ledger->cleanupStatus.load();
                    if (result.status == NOTEPADS_MARKDOWN_CORE_OK)
                    {
                        result.status = NOTEPADS_MARKDOWN_CORE_INVALID_STATE;
                    }
                    result.error.status = result.status;
                    return result;
                }
            }
            bool initiallyCanceled{};
            {
                std::lock_guard guard(state->preparingCancelMutex);
                initiallyCanceled = state->preparingIntent;
            }
            notepads_markdown_core_job *job{};
            notepads_markdown_core_cancel *cancel{};
            result.status =
                notepads_markdown_core_session_start_request(state->session, initiallyCanceled ? 1 : 0, &job, &cancel, &result.error);
            if (result.status != NOTEPADS_MARKDOWN_CORE_OK)
            {
                std::lock_guard guard(state->mutex);
                state->Primary(result.status);
                return result;
            }
            {
                std::lock_guard guard(state->preparingCancelMutex);
                state->preparingCancel = cancel;
                if (state->preparingIntent)
                {
                    state->ledger->Cleanup(notepads_markdown_core_cancel_request(cancel));
                }
            }
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            state->Hook(CoreTestPoint::PreparingAfterStart);
#endif
            std::shared_ptr<Request> request;
            try
            {
                request =
                    std::allocate_shared<Request>(HeapAllocator<Request>{job, state->ledger, AllocationOwner::Request},
                                                  result.serial, job, cancel, state->ledger);
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                if (state->hooks)
                {
                    state->hooks->ObserveRequest(request);
                }
#endif
                if (completion)
                {
                    auto record = std::allocate_shared<CoreCompletionRecord>(
                        HeapAllocator<CoreCompletionRecord>{job, state->ledger, AllocationOwner::Completion});
                    record->serial = result.serial;
                    record->request = request;
                    record->cancelRequest = [](std::shared_ptr<void> const &value) noexcept
                    { std::static_pointer_cast<Request>(value)->Cancel(); };
                    request->completion = record;
                    *completion = CoreCompletionTicket(std::move(record));
                }
            }
            catch (AllocationFailure const &failure)
            {
                result.status = failure.status;
                result.error = failure.error;
            }
            catch (...)
            {
                result.status = NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT;
                result.error.status = result.status;
            }
            if (request && result.status == NOTEPADS_MARKDOWN_CORE_OK)
            {
                result.status = prepare(job, result.sourceState, result.error);
            }
            bool published = false;
            {
                std::lock_guard guard(state->mutex);
                std::lock_guard preparingGuard(state->preparingCancelMutex);
                state->preparingCancel = nullptr;
                state->preparing = false;
                if (request)
                {
                    // Preparing cannot Claim. This is the sole, atomic transfer
                    // from fixed early intent to the request's publication gate.
                    if (state->preparingIntent || state->closed)
                    {
                        request->publication = Publication::Canceled;
                        request->canceled.store(true, std::memory_order_release);
                    }
                }
                if (result.status == NOTEPADS_MARKDOWN_CORE_OK && state->closed)
                {
                    // S may already be accepted, but a closed native controller
                    // cannot return a surviving publication ticket to its caller.
                    result.status = NOTEPADS_MARKDOWN_CORE_CANCELLED;
                    result.error = Pod<notepads_markdown_core_error>();
                    result.error.status = result.status;
                    result.sourceState.has_ready = 0;
                    result.sourceState.ready = {};
                }
                else if (result.status == NOTEPADS_MARKDOWN_CORE_OK && result.sourceState.has_ready == 0)
                {
                    result.status = NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT;
                    result.error = Pod<notepads_markdown_core_error>();
                    result.error.status = result.status;
                }
                if (result.sourceState.state_valid != 0)
                {
                    state->sourceState = result.sourceState;
                }
                state->Primary(result.status);
                if (request && result.status == NOTEPADS_MARKDOWN_CORE_OK && result.sourceState.has_ready != 0 && !state->closed)
                {
                    state->pending = request;
                    published = true;
                }
            }
            if (!published)
            {
                if (request)
                {
                    request->status = result.status;
                    request->error = result.error;
                    request->Cancel();
                    auto clean = request->Cleanup();
                    std::lock_guard guard(state->mutex);
                    state->coreCleanupFailed = state->coreCleanupFailed || !clean;
                    state->closed = state->closed || !clean;
                    ++state->retired;
                }
                else
                {
                    // Owner construction failed. The fixed Preparing borrow still
                    // prevents teardown and no cancel registration remains.
                    auto jobCleanup = notepads_markdown_core_job_destroy(job);
                    auto cancelCleanup = notepads_markdown_core_cancel_destroy(cancel);
                    state->ledger->Cleanup(jobCleanup);
                    state->ledger->Cleanup(cancelCleanup);
                    if (jobCleanup != NOTEPADS_MARKDOWN_CORE_OK || cancelCleanup != NOTEPADS_MARKDOWN_CORE_OK)
                    {
                        std::lock_guard guard(state->mutex);
                        state->coreCleanupFailed = true;
                        state->closed = true;
                    }
                }
            }
            state->Signal();
            return result;
        }
    }

#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
    CoreControllerTestHooks::CoreControllerTestHooks()
    {
        for (auto &gate : _gates)
        {
            gate.reached = Event(true);
            gate.release = Event(true);
        }
    }
    void CoreControllerTestHooks::Arm(CoreTestPoint point, uint32_t maxHoldMilliseconds) noexcept
    {
        auto &gate = _gates[static_cast<size_t>(point)];
        ::ResetEvent(gate.reached.get());
        ::ResetEvent(gate.release.get());
        gate.timedOut.store(false, std::memory_order_release);
        gate.maxHoldMilliseconds.store(maxHoldMilliseconds, std::memory_order_release);
        gate.armed.store(true, std::memory_order_release);
    }
    void CoreControllerTestHooks::Release(CoreTestPoint point) noexcept
    {
        auto &gate = _gates[static_cast<size_t>(point)];
        gate.armed.store(false, std::memory_order_release);
        ::SetEvent(gate.release.get());
    }
    void CoreControllerTestHooks::ReleaseAll() noexcept
    {
        for (size_t index = 0; index < _gates.size(); ++index)
        {
            Release(static_cast<CoreTestPoint>(index));
        }
    }
    bool CoreControllerTestHooks::WaitReached(CoreTestPoint point, uint32_t milliseconds) const noexcept
    {
        return ::WaitForSingleObjectEx(_gates[static_cast<size_t>(point)].reached.get(), milliseconds, FALSE) ==
               WAIT_OBJECT_0;
    }
    void CoreControllerTestHooks::Reach(CoreTestPoint point) noexcept
    {
        auto &gate = _gates[static_cast<size_t>(point)];
        if (gate.armed.exchange(false, std::memory_order_acq_rel))
        {
            ::SetEvent(gate.reached.get());
            auto waited = ::WaitForSingleObjectEx(gate.release.get(),
                                                  gate.maxHoldMilliseconds.load(std::memory_order_acquire), FALSE);
            if (waited != WAIT_OBJECT_0)
            {
                gate.timedOut.store(true, std::memory_order_release);
            }
        }
    }
    bool CoreControllerTestHooks::AnyTimedOut() const noexcept
    {
        for (auto const &gate : _gates)
        {
            if (gate.timedOut.load(std::memory_order_acquire))
            {
                return true;
            }
        }
        return false;
    }
    void CoreControllerTestHooks::RetainRequestWeak(bool enabled) noexcept
    {
        std::lock_guard guard(_weakMutex);
        _retainWeak = enabled;
    }
    void CoreControllerTestHooks::ReleaseRequestWeak() noexcept
    {
        std::lock_guard guard(_weakMutex);
        _requestWeak.reset();
        _retainWeak = false;
    }
    bool CoreControllerTestHooks::RetainedRequestExpired() const noexcept
    {
        std::lock_guard guard(_weakMutex);
        return _requestWeak.expired();
    }
    void CoreControllerTestHooks::ObserveRequest(std::shared_ptr<void> const &request) noexcept
    {
        std::lock_guard guard(_weakMutex);
        if (_retainWeak)
        {
            _requestWeak = request;
        }
    }

#endif
    CoreControllerLifetime::~CoreControllerLifetime()
    {
        if (_ledger)
        {
            --_ledger->observationTickets;
        }
    }
    CoreControllerLifetime::CoreControllerLifetime(CoreControllerLifetime const &other) noexcept
        : _ledger(other._ledger)
    {
        if (_ledger)
        {
            ++_ledger->observationTickets;
        }
    }
    CoreControllerLifetime &CoreControllerLifetime::operator=(CoreControllerLifetime const &other) noexcept
    {
        if (this != &other)
        {
            CoreControllerLifetime copy(other);
            _ledger.swap(copy._ledger);
        }
        return *this;
    }
    CoreControllerLifetime::CoreControllerLifetime(CoreControllerLifetime &&other) noexcept
        : _ledger(std::move(other._ledger))
    {
    }
    CoreControllerLifetime &CoreControllerLifetime::operator=(CoreControllerLifetime &&other) noexcept
    {
        if (this != &other)
        {
            if (_ledger)
            {
                --_ledger->observationTickets;
            }
            _ledger = std::move(other._ledger);
        }
        return *this;
    }
    CoreLifetimeSnapshot CoreControllerLifetime::Snapshot() const noexcept
    {
        if (_ledger)
        {
            return _ledger->Snapshot();
        }
        CoreLifetimeSnapshot empty;
        empty.fullyReleased = true;
        return empty;
    }

    CoreModelPin::CoreModelPin(notepads_markdown_core_model *model, std::shared_ptr<CoreControllerLedger> ledger) noexcept
        : _model(model), _ledger(std::move(ledger))
    {
        if (_model != nullptr)
        {
            ++_ledger->modelPins;
        }
    }
    CoreModelPin::~CoreModelPin()
    {
        Reset();
    }
    CoreModelPin::CoreModelPin(CoreModelPin &&other) noexcept
        : _model(std::exchange(other._model, nullptr)), _ledger(std::move(other._ledger))
    {
    }
    CoreModelPin &CoreModelPin::operator=(CoreModelPin &&other) noexcept
    {
        if (this != &other)
        {
            Reset();
            _model = std::exchange(other._model, nullptr);
            _ledger = std::move(other._ledger);
        }
        return *this;
    }
    CoreModelPin CoreModelPin::Clone() const
    {
        return CloneModel(_model, _ledger);
    }
    CoreModelPin CoreModelPin::CloneModel(notepads_markdown_core_model *source, std::shared_ptr<CoreControllerLedger> const &ledger)
    {
        auto error = Pod<notepads_markdown_core_error>();
        notepads_markdown_core_cancel *cancel{};
        notepads_markdown_core_model *model{};
        auto status = notepads_markdown_core_model_cancel_create(source, &cancel, &error);
        if (status == NOTEPADS_MARKDOWN_CORE_OK)
            status = notepads_markdown_core_model_clone(source, cancel, &model, &error);
        if (cancel)
            ledger->Cleanup(notepads_markdown_core_cancel_destroy(cancel));
        if (status != NOTEPADS_MARKDOWN_CORE_OK)
            throw winrt::hresult_error(E_FAIL, L"Cannot retain the installed Core model.");
        return CoreModelPin(model, ledger);
    }
    void CoreModelPin::Reset() noexcept
    {
        if (_model != nullptr)
        {
            _ledger->Cleanup(notepads_markdown_core_model_destroy(std::exchange(_model, nullptr)));
            --_ledger->modelPins;
        }
        _ledger.reset();
    }

    std::unique_ptr<CoreController> CoreController::Create(winrt::Windows::UI::Core::CoreDispatcher const &dispatcher,
                                                           uint32_t profile
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                                                           , std::shared_ptr<CoreControllerTestHooks> hooks
#endif
    )
    {
        return CreateInternal(dispatcher, profile,
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                              std::move(hooks),
#endif
                              false);
    }

    std::unique_ptr<CoreController> CoreController::CreatePreview(
        winrt::Windows::UI::Core::CoreDispatcher const &dispatcher
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        , std::shared_ptr<CoreControllerTestHooks> hooks
#endif
    )
    {
        return CreateInternal(dispatcher, NOTEPADS_MARKDOWN_CORE_G3,
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                              std::move(hooks),
#endif
                              true);
    }

    std::unique_ptr<CoreController> CoreController::CreateEmbeddedPreview(
        winrt::Windows::UI::Core::CoreDispatcher const &dispatcher, CoreModelPin const &parent)
    {
        if (!parent)
            throw winrt::hresult_invalid_argument();
        return CreateInternal(dispatcher, NOTEPADS_MARKDOWN_CORE_G3,
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                              {},
#endif
                              true, parent.Get());
    }

    std::unique_ptr<CoreController> CoreController::CreateInternal(
        winrt::Windows::UI::Core::CoreDispatcher const &dispatcher, uint32_t profile,
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        std::shared_ptr<CoreControllerTestHooks> hooks,
#endif
        bool preview, notepads_markdown_core_model const *parent)
    {
        if (!dispatcher || !dispatcher.HasThreadAccess())
        {
            throw winrt::hresult_wrong_thread();
        }
        if (notepads_markdown_core_abi_version() != NOTEPADS_MARKDOWN_CORE_ABI_VERSION)
        {
            throw winrt::hresult_error(E_NOINTERFACE, L"Core ABI mismatch.");
        }
        // Bootstrap is empty control initialization: no source/input is borrowed.
        // Its independent Preparing job only funds known fixed native storage.
        // Every update later starts a different original job before its own owner.
        struct Initialization
        {
            notepads_markdown_core_session *session{};
            notepads_markdown_core_job *job{};
            notepads_markdown_core_cancel *cancel{};
            std::shared_ptr<CoreControllerLedger> ledger;
            int32_t RetireRequest() noexcept
            {
                int32_t first = NOTEPADS_MARKDOWN_CORE_OK;
                if (job != nullptr)
                {
                    auto status = notepads_markdown_core_job_destroy(std::exchange(job, nullptr));
                    if (ledger)
                    {
                        ledger->Cleanup(status);
                    }
                    if (status != NOTEPADS_MARKDOWN_CORE_OK)
                    {
                        first = status;
                    }
                }
                if (cancel != nullptr)
                {
                    auto status = notepads_markdown_core_cancel_destroy(std::exchange(cancel, nullptr));
                    if (ledger)
                    {
                        ledger->Cleanup(status);
                    }
                    if (first == NOTEPADS_MARKDOWN_CORE_OK)
                    {
                        first = status;
                    }
                }
                return first;
            }
            ~Initialization()
            {
                RetireRequest();
                if (session != nullptr)
                {
                    auto status = notepads_markdown_core_session_destroy(session);
                    if (ledger)
                    {
                        ledger->Cleanup(status);
                    }
                }
            }
        } initialization;
        auto error = Pod<notepads_markdown_core_error>();
        auto status = parent ? notepads_markdown_core_model_cancel_create(parent, &initialization.cancel, &error)
                             : preview ? notepads_markdown_preview_cancel_create(&initialization.cancel, &error)
                                       : notepads_markdown_core_cancel_create(nullptr, &initialization.cancel, &error);
        if (status == NOTEPADS_MARKDOWN_CORE_OK)
        {
            status = notepads_markdown_core_session_create(initialization.cancel, profile, 0, &initialization.session, &error);
        }
        auto bootstrapCleanup = initialization.RetireRequest();
        if (status != NOTEPADS_MARKDOWN_CORE_OK || bootstrapCleanup != NOTEPADS_MARKDOWN_CORE_OK)
        {
            throw winrt::hresult_error(E_FAIL, L"Core controller empty-session initialization failed.");
        }
        status = notepads_markdown_core_session_start_request(initialization.session, 0, &initialization.job, &initialization.cancel,
                                               &error);
        if (status != NOTEPADS_MARKDOWN_CORE_OK)
        {
            throw winrt::hresult_error(E_FAIL, L"Core controller fixed-storage admission failed.");
        }
        auto initJob = initialization.job;
        auto ledger = std::allocate_shared<CoreControllerLedger>(
            HeapAllocator<CoreControllerLedger>{initJob, {}, AllocationOwner::Observation});
        initialization.ledger = ledger;
        auto state = std::allocate_shared<CoreControllerState>(
            HeapAllocator<CoreControllerState>{initJob, ledger, AllocationOwner::State}, ledger);
        state->session = std::exchange(initialization.session, nullptr);
        state->dispatcher = dispatcher;
        state->uiThreadId = ::GetCurrentThreadId();
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        state->hooks = std::move(hooks);
#endif
        auto uiPin =
            std::allocate_shared<DelegatePin>(HeapAllocator<DelegatePin>{initJob, ledger, AllocationOwner::UiDelegate});
        state->uiHandler = winrt::Windows::UI::Core::DispatchedHandler{
            [weak = std::weak_ptr(state), pin = std::move(uiPin)]() noexcept { UiBody(weak); }};
        auto driverPin = std::allocate_shared<DelegatePin>(
            HeapAllocator<DelegatePin>{initJob, ledger, AllocationOwner::DriverDelegate});
        winrt::Windows::System::Threading::WorkItemHandler driver{
            [state, pin = std::move(driverPin)](winrt::Windows::Foundation::IAsyncAction const &) noexcept
            { Driver(state); }};
        auto memory = HeapAllocator<CoreController>{initJob, ledger, AllocationOwner::ControlHandle}.allocate(1);
        auto owner = std::unique_ptr<CoreController>(::new (memory) CoreController());
        owner->_state = state;
        // All known allocations and failure cleanup are established before the
        // sole platform driver starts. Retire the init job before RunAsync; its
        // independent leases remain live until each actual native final free.
        auto initCleanup = initialization.RetireRequest();
        if (initCleanup != NOTEPADS_MARKDOWN_CORE_OK || ledger->cleanupStatus.load() != NOTEPADS_MARKDOWN_CORE_OK ||
            ledger->cleanupHresult.load() != 0)
        {
            throw winrt::hresult_error(E_FAIL, L"Core controller fixed-storage initialization cleanup failed.");
        }
        // No request can enter before this succeeds. State deliberately has no
        // driver action; only this independent owner retains the returned action.
        auto action = winrt::Windows::System::Threading::ThreadPool::RunAsync(driver);
        owner->_driver = std::move(action);
        return owner;
    }
    CoreController::~CoreController()
    {
        Stop();
        // Releasing the action is not an observation of its eventual completion.
        _driver = nullptr;
        _state.reset();
    }
    void CoreController::operator delete(void *memory) noexcept
    {
        if (memory != nullptr)
        {
            FreeAllocation(memory);
        }
    }
    CoreSubmitResult CoreController::SubmitUtf16(std::span<uint16_t const> input,
                                                 notepads_markdown_core_snapshot const &descriptor, uint32_t extensions) noexcept
    {
        return Submit(_state,
                      [&](notepads_markdown_core_job *job, notepads_markdown_core_submit_result &result, notepads_markdown_core_error &error)
                      {
                          return notepads_markdown_core_job_replace_utf16(_state->session, job, input.data(), input.size(),
                                                           &descriptor, extensions, &result, &error);
                      });
    }
    CoreSubmitResult CoreController::SubmitUtf8(std::span<uint8_t const> input,
                                                              notepads_markdown_core_snapshot const &descriptor,
                                                CoreCompletionTicket *completion, uint32_t extensions) noexcept
    {
        // Source preparation captures the complete configuration in the same
        // Rust job that enters our existing active/latest-pending queue.
        return Submit(
            _state,
            [&](notepads_markdown_core_job *job, notepads_markdown_core_submit_result &result, notepads_markdown_core_error &error)
            {
                return notepads_markdown_core_job_replace_utf8(_state->session, job, input.data(), input.size(),
                                                                &descriptor, extensions, &result, &error);
            },
            completion);
    }
    CoreSubmitResult CoreController::SubmitEdits(notepads_markdown_core_edit_batch const &descriptor,
                                                 std::span<notepads_markdown_core_edit_row const> rows,
                                                 std::span<uint8_t const> replacements,
                                                 CoreCompletionTicket *completion) noexcept
    {
        if (completion)
            *completion = {};
        if (descriptor.count != rows.size())
        {
            CoreSubmitResult result;
            result.status = NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT;
            result.error = Pod<notepads_markdown_core_error>();
            result.error.status = result.status;
            result.sourceState = Pod<notepads_markdown_core_submit_result>();
            return result;
        }
        return Submit(_state,
                      [&](notepads_markdown_core_job *job, notepads_markdown_core_submit_result &result, notepads_markdown_core_error &error)
                      {
                          return notepads_markdown_core_job_apply(_state->session, job, &descriptor, rows.data(), replacements.data(),
                                                   replacements.size(), &result, &error);
                      }, completion);
    }
    CoreSubmitResult CoreController::CaptureCurrent() noexcept
    {
        return Submit(_state, [&](notepads_markdown_core_job *job, notepads_markdown_core_submit_result &result, notepads_markdown_core_error &error)
                      { return notepads_markdown_core_job_capture_current(_state->session, job, &result, &error); });
    }
    void CoreController::RequestCancel() noexcept
    {
        if (_state)
        {
            CancelState(_state, false);
        }
    }
    void CoreController::Stop() noexcept
    {
        if (_state)
        {
            CancelState(_state, true);
        }
    }
    CoreDrainResult CoreController::Drain() noexcept
    {
        CoreDrainResult result;
        if (!_state)
        {
            return result;
        }
        if (::GetCurrentThreadId() == _state->uiThreadId)
        {
            result.primaryStatus = NOTEPADS_MARKDOWN_CORE_INVALID_STATE;
            return result;
        }
        result.offUi = true;
        if (_drainEntered.exchange(true))
        {
            result.primaryStatus = NOTEPADS_MARKDOWN_CORE_INVALID_STATE;
            return result;
        }
        Stop();
        bool terminal = false;
        while (!terminal)
        {
            try
            {
                terminal = !_driver || _driver.Status() != winrt::Windows::Foundation::AsyncStatus::Started;
            }
            catch (...)
            {
                std::lock_guard guard(_state->mutex);
                _state->PrimaryHr(CurrentHresult());
                break;
            }
            if (!terminal)
            {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                _state->Hook(CoreTestPoint::DrainBeforeWait);
#endif
                ::WaitForSingleObjectEx(_state->drainChanged.get(), 10, FALSE);
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                _state->Hook(CoreTestPoint::DrainAfterWait);
#endif
            }
        }
        if (terminal && _driver)
        {
            try
            {
                _driver.GetResults();
            }
            catch (...)
            {
                std::lock_guard guard(_state->mutex);
                _state->PrimaryHr(CurrentHresult());
            }
            try
            {
                _driver.Close();
            }
            catch (...)
            {
                _state->ledger->CleanupHr(CurrentHresult());
            }
            _driver = nullptr;
            std::lock_guard guard(_state->mutex);
            _state->driverExited = true;
        }
        auto snapshot = Snapshot();
        result.coreDrained = snapshot.coreDrained;
        result.driverExited = snapshot.driverExited;
        result.canceled = snapshot.canceled;
        result.primaryStatus = snapshot.primaryStatus;
        result.cleanupStatus = snapshot.cleanupStatus;
        result.primaryHresult = snapshot.primaryHresult;
        result.cleanupHresult = snapshot.cleanupHresult;
        return result;
    }
    CoreControllerSnapshot CoreController::Snapshot() const noexcept
    {
        CoreControllerSnapshot value;
        if (!_state)
        {
            return value;
        }
        std::lock_guard guard(_state->mutex);
        value.closed = _state->closed;
        value.preparing = _state->preparing;
        value.submitEntered = _state->submitBorrows != 0;
        value.installEntered = _state->installEntered;
        value.active = _state->active ? 1 : 0;
        value.pending = _state->pending ? 1 : 0;
        value.activeSerial = _state->active ? _state->active->serial : 0;
        value.pendingSerial = _state->pending ? _state->pending->serial : 0;
        value.lastSubmitted = _state->serial;
        value.lastInstalled = _state->lastInstalled;
        value.retired = _state->retired;
        value.runCount = _state->runs;
        value.installedCount = _state->installs;
        value.driverStartCount = _state->driverStarts;
        value.wakePhase = _state->wake;
        value.wakeGeneration = _state->wakeGeneration;
        value.wakeSerial = _state->wakeSerial;
        value.wakeEntered = _state->wakeEntered;
        value.wakeAck = _state->wakeAck;
        value.postingEntries = _state->postingEntries;
        value.ackWhileStarted = _state->ackWhileStarted;
        value.rejectedBeforeEntry = _state->rejectedBeforeEntry;
        value.coreDrained = _state->coreDrained;
        value.driverExited = _state->driverExited;
        value.canceled = _state->canceled;
        value.primaryStatus = _state->primaryStatus;
        value.primaryHresult = _state->primaryHresult;
        value.cleanupStatus = _state->ledger->cleanupStatus.load();
        value.cleanupHresult = _state->ledger->cleanupHresult.load();
        value.sourceState = _state->sourceState;
        value.lifetime = _state->ledger->Snapshot();
        return value;
    }
    CoreModelPin CoreController::PinInstalled()
    {
        if (!_state->dispatcher.HasThreadAccess())
        {
            throw winrt::hresult_wrong_thread();
        }
        notepads_markdown_core_model *installed{};
        {
            std::lock_guard guard(_state->mutex);
            if (_state->closed || _state->submitBorrows != 0 || _state->installEntered || !_state->installed)
            {
                throw winrt::hresult_illegal_method_call();
            }
            ++_state->submitBorrows;
            installed = _state->installed;
        }
        struct ReleaseBorrow
        {
            CoreControllerState &state;
            ~ReleaseBorrow()
            {
                std::lock_guard guard(state.mutex);
                --state.submitBorrows;
                state.Signal();
            }
        } release{*_state};
        return CoreModelPin::CloneModel(installed, _state->ledger);
    }
    CoreControllerLifetime CoreController::Lifetime() const noexcept
    {
        CoreControllerLifetime value;
        if (_state)
        {
            value._ledger = _state->ledger;
            ++value._ledger->observationTickets;
        }
        return value;
    }
}
