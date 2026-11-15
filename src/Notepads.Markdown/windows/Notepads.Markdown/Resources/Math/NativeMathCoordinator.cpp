// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include <algorithm>
#include <iterator>
#include "Resources/Math/NativeMathCoordinator.h"
#include "Resources/Diagrams/NativeDiagramCoordinator.h"
#include "Resources/Math/NativeMathArtifact.h"
#include <cstring>
#include <new>

namespace Notepads::Markdown::Native
{
    namespace
    {
        notepads_markdown_core_error Error() noexcept
        {
            return {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(notepads_markdown_core_error)};
        }
    }
    MathCoordinatorHandle::MathCoordinatorHandle(NativeMathCoordinator *value) noexcept : _value(value)
    {
    }
    MathCoordinatorHandle::MathCoordinatorHandle(MathCoordinatorHandle const &other) noexcept : _value(other._value)
    {
        if (_value)
            _value->AddRef();
    }
    MathCoordinatorHandle::MathCoordinatorHandle(MathCoordinatorHandle &&other) noexcept
        : _value(std::exchange(other._value, nullptr))
    {
    }
    MathCoordinatorHandle &MathCoordinatorHandle::operator=(MathCoordinatorHandle const &other) noexcept
    {
        auto copy = other;
        std::swap(_value, copy._value);
        return *this;
    }
    MathCoordinatorHandle &MathCoordinatorHandle::operator=(MathCoordinatorHandle &&other) noexcept
    {
        if (this != &other)
        {
            Reset();
            _value = std::exchange(other._value, nullptr);
        }
        return *this;
    }
    MathCoordinatorHandle::~MathCoordinatorHandle()
    {
        Reset();
    }
    NativeMathCoordinator *MathCoordinatorHandle::operator->() const noexcept
    {
        return _value;
    }
    MathCoordinatorHandle::operator bool() const noexcept
    {
        return _value != nullptr;
    }
    void MathCoordinatorHandle::Reset() noexcept
    {
        if (auto value = std::exchange(_value, nullptr))
            value->Release();
    }
    NativeMathCoordinator::NativeMathCoordinator(notepads_markdown_math_control *control, NativeMathDeliverySink const &sink) noexcept
        : _control(control), _sink(sink)
    {
    }
    MathCoordinatorHandle NativeMathCoordinator::Create(NativeMathDeliverySink const &sink)
    {
        notepads_markdown_math_control *control{};
        auto error = Error();
        auto status = NativeMathRuntime::Shared().CreateControl(&control, &error);
        if (status != NOTEPADS_MARKDOWN_CORE_OK)
            throw MathMemoryFailure{status, error};
        auto storage = HeapAlloc(GetProcessHeap(), 0, sizeof(NativeMathCoordinator));
        if (!storage)
        {
            (void)notepads_markdown_math_control_release(control, &error);
            NotifyResourcesAvailable();
            throw MathMemoryFailure{NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY};
        }
        auto result = new (storage) NativeMathCoordinator(control, sink);
        auto &runtime = NativeMathRuntime::Shared();
        AcquireSRWLockExclusive(&runtime._lock);
        result->_runtimeNext = runtime._waiters;
        runtime._waiters = result;
        ReleaseSRWLockExclusive(&runtime._lock);
        return MathCoordinatorHandle(result);
    }
    void NativeMathCoordinator::AddRef() noexcept
    {
        _references.fetch_add(1, std::memory_order_relaxed);
    }
    void NativeMathCoordinator::Release() noexcept
    {
        // Serialize the last decrement with the registry's non-owning lookup.
        auto &runtime = NativeMathRuntime::Shared();
        AcquireSRWLockExclusive(&runtime._lock);
        if (_references.fetch_sub(1, std::memory_order_acq_rel) != 1)
        {
            ReleaseSRWLockExclusive(&runtime._lock);
            return;
        }
        auto link = &runtime._waiters;
        while (*link && *link != this)
            link = &(*link)->_runtimeNext;
        if (*link)
            *link = _runtimeNext;
        ReleaseSRWLockExclusive(&runtime._lock);
        ModuleLifetime tail;
        auto control = _control;
        this->~NativeMathCoordinator();
        if (!HeapFree(GetProcessHeap(), 0, this))
        {
            ++winrt::get_module_lock(); // Preserve the real policy header lease.
            return;
        }
        auto error = Error();
        (void)notepads_markdown_math_control_release(control, &error);
        NotifyResourcesAvailable();
    }
    void NativeMathCoordinator::BeginSource(uint64_t attempt) noexcept
    {
        CancelAll();
        AcquireSRWLockExclusive(&_lock);
        _attempt = attempt;
        std::memset(_attempts, 0, sizeof(_attempts));
        ReleaseSRWLockExclusive(&_lock);
    }
    std::array<NativeMathState, NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT>
    NativeMathCoordinator::AttemptStates(uint64_t attempt) const noexcept
    {
        std::array<NativeMathState, NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT> states{};
        AcquireSRWLockShared(&_lock);
        if (attempt == _attempt)
            std::transform(std::begin(_attempts), std::end(_attempts), states.begin(),
                [](uint8_t state) { return static_cast<NativeMathState>(state); });
        else
            states.fill(NativeMathState::Limited);
        ReleaseSRWLockShared(&_lock);
        return states;
    }

#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
    NativeMathState NativeMathCoordinator::AttemptState(uint64_t attempt, uint32_t ordinal) const noexcept
    {
        AcquireSRWLockShared(&_lock);
        auto state = attempt == _attempt && ordinal < NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT
                         ? static_cast<NativeMathState>(_attempts[ordinal])
                         : NativeMathState::Limited;
        ReleaseSRWLockShared(&_lock);
        return state;
    }
#endif
    NativeMathState NativeMathCoordinator::Request(notepads_markdown_core_model const *model, NativeMathKey const &key,
                                                   uint32_t ordinal, ID2D1Factory *factory,
                                                   notepads_markdown_core_error *error, uint32_t residentLimit) noexcept
    {
        if (!error)
            return NativeMathState::Failed;
        if (ordinal >= NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT)
        {
            error->status = NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT;
            return NativeMathState::Limited;
        }
        AcquireSRWLockExclusive(&_lock);
        if (key.attempt != _attempt)
        {
            ReleaseSRWLockExclusive(&_lock);
            error->status = NOTEPADS_MARKDOWN_CORE_STALE_RESULT;
            return NativeMathState::Cancelled;
        }
        auto previous = static_cast<NativeMathState>(_attempts[ordinal]);
        if (previous != NativeMathState::None)
        {
            ReleaseSRWLockExclusive(&_lock);
            return previous;
        }
        // The view apportions the existing shared resident pool between enabled
        // families before either eager binding pass. Memoize denied ordinals so
        // they are terminal Limited, never an unrequested Pending placeholder.
        if (ordinal >= residentLimit)
        {
            _attempts[ordinal] = static_cast<uint8_t>(NativeMathState::Limited);
            ReleaseSRWLockExclusive(&_lock);
            error->status = NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT;
            return NativeMathState::Limited;
        }
        _attempts[ordinal] = static_cast<uint8_t>(NativeMathState::Pending);
        AddRef(); // Transferred to the Node after successful prepare.
        ReleaseSRWLockExclusive(&_lock);
        NativeMathWorker::PreparedNode *node{};
        auto status =
            NativeMathWorker::Prepare(_control, model, key, ordinal, factory, this, Completed, &node, error, Admit);
        if (status != NOTEPADS_MARKDOWN_CORE_OK)
        {
            auto state = NativeMathPresentation::Classify(status);
            AcquireSRWLockExclusive(&_lock);
            if (_attempt == key.attempt)
                _attempts[ordinal] = static_cast<uint8_t>(state);
            ReleaseSRWLockExclusive(&_lock);
            Release();
            return state;
        }
        AcquireSRWLockExclusive(&_lock);
        bool stale = _attempt != key.attempt;
        if (!_head)
            _head = node;
        else
        {
            auto tail = _head;
            while (NativeMathWorker::NextPrepared(tail))
                tail = NativeMathWorker::NextPrepared(tail);
            NativeMathWorker::SetNextPrepared(tail, node);
        }
        // Original native Node reference becomes FIFO list ownership.
        NativeMathWorker::RetainPrepared(node); // Protect Start/Cancel from an immediate completion.
        ReleaseSRWLockExclusive(&_lock);
        // Register first, then retry admission immediately. Resource changes in
        // the registration window are also caught by the <=100ms periodic timer.
        (void)NativeMathWorker::StartPrepared(node);
        if (stale)
            NativeMathWorker::CancelPrepared(node);
        NativeMathWorker::ReleasePrepared(node);
        return NativeMathState::Pending;
    }
    void NativeMathCoordinator::CancelAll() noexcept
    {
        NativeMathWorker::PreparedNode *nodes[NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT]{};
        uint32_t count{};
        AcquireSRWLockShared(&_lock);
        for (auto node = _head; node && count < NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT;
             node = NativeMathWorker::NextPrepared(node))
        {
            NativeMathWorker::RetainPrepared(node);
            nodes[count++] = node;
        }
        ReleaseSRWLockShared(&_lock);
        for (uint32_t i = 0; i < count; ++i)
        {
            NativeMathWorker::CancelPrepared(nodes[i]);
            NativeMathWorker::ReleasePrepared(nodes[i]);
        }
    }
    void NativeMathCoordinator::Wake() noexcept
    {
        NativeMathWorker::PreparedNode *nodes[NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT]{};
        uint32_t count{};
        AcquireSRWLockShared(&_lock);
        for (auto node = _head; node && count < NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT;
             node = NativeMathWorker::NextPrepared(node))
        {
            NativeMathWorker::RetainPrepared(node);
            nodes[count++] = node;
        }
        ReleaseSRWLockShared(&_lock);
        for (uint32_t i = 0; i < count; ++i)
        {
            NativeMathWorker::WakePrepared(nodes[i]);
            NativeMathWorker::ReleasePrepared(nodes[i]);
        }
    }
    void NativeMathCoordinator::NotifyResourcesAvailable() noexcept
    {
        NativeDiagramCoordinator::NotifyResourcesAvailable();
        // No allocation and no COM/FFI while the runtime registry is locked.
        // Policy headers bound registered controls by their real metadata pool;
        // process resident bound caps the useful waiter snapshot at 256.
        NativeMathCoordinator *controls[256]{};
        uint32_t count{};
        auto &runtime = NativeMathRuntime::Shared();
        AcquireSRWLockExclusive(&runtime._lock);
        ++runtime._wakeEpoch;
        if (runtime._notifying.test_and_set(std::memory_order_acquire))
        {
            ReleaseSRWLockExclusive(&runtime._lock);
            return;
        }
        for (;;)
        {
            auto epoch = runtime._wakeEpoch;
            count = 0;
            for (auto control = runtime._waiters; control && count < 256; control = control->_runtimeNext)
            {
                AcquireSRWLockShared(&control->_lock);
                if (control->_head)
                {
                    control->AddRef();
                    controls[count++] = control;
                }
                ReleaseSRWLockShared(&control->_lock);
            }
            ReleaseSRWLockExclusive(&runtime._lock);
            for (uint32_t i = 0; i < count; ++i)
            {
                controls[i]->Wake();
                controls[i]->Release();
            }
            AcquireSRWLockExclusive(&runtime._lock);
            if (epoch == runtime._wakeEpoch)
            {
                runtime._notifying.clear(std::memory_order_release);
                ReleaseSRWLockExclusive(&runtime._lock);
                return;
            }
        }
    }
    bool NativeMathCoordinator::Admit(void *context, NativeMathWorker::PreparedNode *node) noexcept
    {
        auto self = static_cast<NativeMathCoordinator *>(context);
        AcquireSRWLockShared(&self->_lock);
        auto first = self->_head;
        while (first && !NativeMathWorker::IsQueued(first))
            first = NativeMathWorker::NextPrepared(first);
        auto allowed = first == node;
        ReleaseSRWLockShared(&self->_lock);
        return allowed;
    }
    void NativeMathCoordinator::Completed(void *context, notepads_markdown_math_job *job,
                                          NativeMathWorkerResult const &result) noexcept
    {
        auto node = static_cast<NativeMathWorker::PreparedNode *>(context);
        auto owner = static_cast<NativeMathCoordinator *>(NativeMathWorker::PreparedOwner(node));
        owner->Finished(node, job, result);
    }
    void NativeMathCoordinator::Remove(NativeMathWorker::PreparedNode *node, NativeMathState state) noexcept
    {
        AcquireSRWLockExclusive(&_lock);
        auto current = _head;
        NativeMathWorker::PreparedNode *previous{};
        while (current && current != node)
        {
            previous = current;
            current = NativeMathWorker::NextPrepared(current);
        }
        if (current)
        {
            auto next = NativeMathWorker::NextPrepared(current);
            if (previous)
                NativeMathWorker::SetNextPrepared(previous, next);
            else
                _head = next;
            NativeMathWorker::SetNextPrepared(current, nullptr);
        }
        auto const &key = NativeMathWorker::PreparedKey(node);
        auto ordinal = NativeMathWorker::PreparedOrdinal(node);
        if (_attempt == key.attempt && ordinal < NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT)
            _attempts[ordinal] = static_cast<uint8_t>(state);
        ReleaseSRWLockExclusive(&_lock);
        NativeMathWorker::ClearPreparedOwner(node);
        NativeMathWorker::ReleasePrepared(node); // Remove list owner, not Rust ref.
    }
    void NativeMathCoordinator::Finished(NativeMathWorker::PreparedNode *node, notepads_markdown_math_job *job,
                                         NativeMathWorkerResult const &result) noexcept
    {
        auto status = result.status;
        MathPresentationHandle payload;
        if (status == NOTEPADS_MARKDOWN_CORE_OK && result.stack.threadSignaled)
        {
            // All builder temporaries and failure owners are gone before the
            // retire ref gate below. A failed real free deliberately leaves >1.
            auto built = NativeMathArtifact::BuildAfterThreadExit(job, NativeMathWorker::PreparedFactory(node));
            status = built.status;
            if (status == NOTEPADS_MARKDOWN_CORE_OK)
            {
                auto const &key = NativeMathWorker::PreparedKey(node);
                if (std::memcmp(&key.identity, &built.summary.identity, sizeof(key.identity)) != 0 ||
                    key.object != built.summary.object)
                    status = NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE;
                else
                {
                    try
                    {
                        payload =
                            NativeMathPresentation::CreateReady(key, std::move(built.memory), std::move(built.drawing));
                    }
                    catch (MathMemoryFailure const &failure)
                    {
                        status = failure.status;
                    }
                    catch (...)
                    {
                        status = NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY;
                    }
                }
            }
        }
        if (!payload)
        {
            if (status == NOTEPADS_MARKDOWN_CORE_OK)
                status = NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT;
            auto state = notepads_markdown_math_job_state(job);
            if (state == NOTEPADS_MARKDOWN_MATH_QUEUED || state == NOTEPADS_MARKDOWN_MATH_ACTIVATED || state == NOTEPADS_MARKDOWN_MATH_COMPUTED ||
                state == NOTEPADS_MARKDOWN_MATH_FINISH_FAILED)
            {
                // Rust additionally checks its exact references==1 BEFORE any
                // mutation. Extra memory/prefix/weak/free-failure refs retain64.
                auto error = Error();
                (void)notepads_markdown_math_job_retire_metadata(job, &error);
            }
            payload = NativeMathWorker::FailurePresentation(node, status);
        }
        auto finalState = payload->State();
        if (finalState == NativeMathState::Ready)
        {
            auto attempt = NativeMathWorker::PreparedKey(node).attempt;
            auto ordinal = NativeMathWorker::PreparedOrdinal(node);
            AddRef(); // Receipt pins only the coordinator, never a Ready artifact.
            DerivedDeliveryReceipt receipt(this, attempt, ordinal, &SettleDelivery, [](void *owner) noexcept
                { static_cast<NativeMathCoordinator *>(owner)->Release(); });
            // Retire before dispatch: UI Invoke may run before Post returns.
            // The ordinal stays Pending until linking or dropped dispatch settles.
            Remove(node, NativeMathState::Pending);
            (void)NativeMathTransport::Post(_sink, payload, std::move(receipt));
        }
        else
        {
            Remove(node, finalState);
            (void)NativeMathWorker::PostFailure(node, _sink, payload);
        }
        NotifyResourcesAvailable();
        Release(); // Last Node->coordinator owner; never touches this afterwards.
    }
    void NativeMathCoordinator::SettleDelivery(void *owner, uint64_t attempt, uint32_t ordinal, bool accepted) noexcept
    {
        auto self = static_cast<NativeMathCoordinator *>(owner);
        AcquireSRWLockExclusive(&self->_lock);
        if (self->_attempt == attempt && ordinal < NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT &&
            self->_attempts[ordinal] == static_cast<uint8_t>(NativeMathState::Pending))
            self->_attempts[ordinal] = static_cast<uint8_t>(accepted ? NativeMathState::Ready : NativeMathState::Unavailable);
        ReleaseSRWLockExclusive(&self->_lock);
    }
    int32_t NativeMathCoordinator::Usage(notepads_markdown_math_usage *usage, notepads_markdown_core_error *error) noexcept
    {
        return notepads_markdown_math_control_usage(_control, usage, error);
    }
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
    uint32_t NativeMathCoordinator::PendingCountForDiagnostics() const noexcept
    {
        uint32_t count{};
        AcquireSRWLockShared(&_lock);
        auto node = _head;
        while (node && count < NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT)
        {
            ++count;
            node = NativeMathWorker::NextPrepared(node);
        }
        bool complete = !node;
        ReleaseSRWLockShared(&_lock);
        return complete ? count : UINT32_MAX;
    }
#endif
}
