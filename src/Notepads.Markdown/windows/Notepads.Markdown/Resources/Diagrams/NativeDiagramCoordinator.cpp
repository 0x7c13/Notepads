// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include <algorithm>
#include <iterator>
#include "Resources/Diagrams/NativeDiagramCoordinator.h"
#include <cstring>
#include <new>

namespace Notepads::Markdown::Native
{
    namespace
    {
        SRWLOCK registryLock = SRWLOCK_INIT;
        NativeDiagramCoordinator *coordinators{};
        std::atomic_flag notifying = ATOMIC_FLAG_INIT;
        uint64_t wakeEpoch{};
        notepads_markdown_core_error Error() noexcept
        {
            return {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(notepads_markdown_core_error)};
        }
    }
    DiagramCoordinatorHandle::DiagramCoordinatorHandle(NativeDiagramCoordinator *value) noexcept : _value(value)
    {
    }
    DiagramCoordinatorHandle::DiagramCoordinatorHandle(DiagramCoordinatorHandle const &other) noexcept
        : _value(other._value)
    {
        if (_value)
            _value->AddRef();
    }
    DiagramCoordinatorHandle::DiagramCoordinatorHandle(DiagramCoordinatorHandle &&other) noexcept
        : _value(std::exchange(other._value, nullptr))
    {
    }
    DiagramCoordinatorHandle &DiagramCoordinatorHandle::operator=(DiagramCoordinatorHandle const &other) noexcept
    {
        auto copy = other;
        std::swap(_value, copy._value);
        return *this;
    }
    DiagramCoordinatorHandle &DiagramCoordinatorHandle::operator=(DiagramCoordinatorHandle &&other) noexcept
    {
        if (this != &other)
        {
            Reset();
            _value = std::exchange(other._value, nullptr);
        }
        return *this;
    }
    DiagramCoordinatorHandle::~DiagramCoordinatorHandle()
    {
        Reset();
    }
    NativeDiagramCoordinator *DiagramCoordinatorHandle::operator->() const noexcept
    {
        return _value;
    }
    DiagramCoordinatorHandle::operator bool() const noexcept
    {
        return _value != nullptr;
    }
    void DiagramCoordinatorHandle::Reset() noexcept
    {
        if (auto value = std::exchange(_value, nullptr))
            value->Release();
    }

    NativeDiagramCoordinator::NativeDiagramCoordinator(MathCoordinatorHandle const &sharedControl,
                                                       notepads_markdown_diagram_coordinator_funding *funding,
                                                       NativeDiagramDeliverySink const &sink) noexcept
        : _shared(sharedControl), _funding(funding), _sink(sink)
    {
        ++winrt::get_module_lock();
    }
    DiagramCoordinatorHandle NativeDiagramCoordinator::Create(MathCoordinatorHandle const &sharedControl,
                                                              notepads_markdown_core_model const *model,
                                                              NativeDiagramDeliverySink const &sink)
    {
        auto error = Error();
        if (!sharedControl || !model)
            throw DiagramMemoryFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT, error};
        notepads_markdown_diagram_coordinator_funding *funding{};
        auto status = notepads_markdown_diagram_coordinator_funding_prepare(sharedControl->SharedControl(), model, &funding, &error);
        if (status != NOTEPADS_MARKDOWN_CORE_OK)
            throw DiagramMemoryFailure{status, error};
        auto storage = HeapAlloc(GetProcessHeap(), 0, NOTEPADS_MARKDOWN_DIAGRAM_NATIVE_COORDINATOR_BYTES);
        if (!storage)
        {
            (void)notepads_markdown_diagram_coordinator_funding_release(funding, &error);
            error.status = NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY;
            throw DiagramMemoryFailure{error.status, error};
        }
        auto result = new (storage) NativeDiagramCoordinator(sharedControl, funding, sink);
        AcquireSRWLockExclusive(&registryLock);
        result->_runtimeNext = coordinators;
        coordinators = result;
        ReleaseSRWLockExclusive(&registryLock);
        return DiagramCoordinatorHandle(result);
    }
    void NativeDiagramCoordinator::AddRef() noexcept
    {
        _references.fetch_add(1, std::memory_order_relaxed);
    }
    void NativeDiagramCoordinator::Release() noexcept
    {
        AcquireSRWLockExclusive(&registryLock);
        if (_references.fetch_sub(1, std::memory_order_acq_rel) != 1)
        {
            ReleaseSRWLockExclusive(&registryLock);
            return;
        }
        auto link = &coordinators;
        while (*link && *link != this)
            link = &(*link)->_runtimeNext;
        if (*link)
            *link = _runtimeNext;
        ReleaseSRWLockExclusive(&registryLock);
        ModuleLifetime tail;
        auto funding = _funding;
        // Move the only nontrivial owners out before releasing raw storage.
        // On a HeapFree failure the original C++ lifetime/identity is untouched
        // and the shared policy owner can be restored, without resurrection.
        auto shared = std::move(_shared);
        auto sink = std::move(_sink);
        sink = {};
        if (!HeapFree(GetProcessHeap(), 0, this))
        {
            _shared = std::move(shared);
            _references.store(1, std::memory_order_release);
            _quarantined = true;
            AcquireSRWLockExclusive(&registryLock);
            _runtimeNext = coordinators;
            coordinators = this;
            ReleaseSRWLockExclusive(&registryLock);
            return; // Funding + original module pin remain in the live header.
        }
        auto error = Error();
        (void)notepads_markdown_diagram_coordinator_funding_release(funding, &error);
        --winrt::get_module_lock();
        NativeMathCoordinator::NotifyResourcesAvailable();
    }
    void NativeDiagramCoordinator::BeginSource(uint64_t attempt) noexcept
    {
        CancelAll();
        AcquireSRWLockExclusive(&_lock);
        _attempt = attempt;
        _hasPolicyKey = false;
        std::memset(_attempts, 0, sizeof(_attempts));
        ReleaseSRWLockExclusive(&_lock);
    }
    std::array<NativeDiagramState, NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT>
    NativeDiagramCoordinator::AttemptStates(uint64_t attempt) const noexcept
    {
        std::array<NativeDiagramState, NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT> states{};
        AcquireSRWLockShared(&_lock);
        if (attempt == _attempt)
            std::transform(std::begin(_attempts), std::end(_attempts), states.begin(),
                [](uint8_t state) { return static_cast<NativeDiagramState>(state); });
        else
            states.fill(NativeDiagramState::Limited);
        ReleaseSRWLockShared(&_lock);
        return states;
    }

#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
    NativeDiagramState NativeDiagramCoordinator::AttemptState(uint64_t attempt, uint32_t ordinal) const noexcept
    {
        AcquireSRWLockShared(&_lock);
        auto state = attempt == _attempt && ordinal < NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT
                         ? static_cast<NativeDiagramState>(_attempts[ordinal])
                         : NativeDiagramState::Limited;
        ReleaseSRWLockShared(&_lock);
        return state;
    }
#endif
    NativeDiagramState NativeDiagramCoordinator::Request(notepads_markdown_core_model const *model, NativeDiagramKey const &key,
                                                         uint32_t ordinal, notepads_markdown_diagram_font_capability const &fonts,
                                                         notepads_markdown_diagram_style const &style, notepads_markdown_core_error *error,
                                                         uint32_t residentLimit) noexcept
    {
        if (!error)
            return NativeDiagramState::Failed;
        if (ordinal >= NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT)
        {
            error->status = NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT;
            return NativeDiagramState::Limited;
        }
        auto policyKey = key;
        policyKey.object = 0;
        AcquireSRWLockExclusive(&_lock);
        if (_quarantined || key.attempt != _attempt || (_hasPolicyKey && !(_policyKey == policyKey)))
        {
            ReleaseSRWLockExclusive(&_lock);
            error->status = NOTEPADS_MARKDOWN_CORE_STALE_RESULT;
            return NativeDiagramState::Cancelled;
        }
        _policyKey = policyKey;
        _hasPolicyKey = true;
        auto previous = static_cast<NativeDiagramState>(_attempts[ordinal]);
        if (previous != NativeDiagramState::None)
        {
            ReleaseSRWLockExclusive(&_lock);
            return previous;
        }
        if (ordinal >= residentLimit)
        {
            _attempts[ordinal] = static_cast<uint8_t>(NativeDiagramState::Limited);
            ReleaseSRWLockExclusive(&_lock);
            error->status = NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT;
            return NativeDiagramState::Limited;
        }
        _attempts[ordinal] = static_cast<uint8_t>(NativeDiagramState::Pending);
        AddRef(); // Successful Prepare transfers this owner to its Node.
        ReleaseSRWLockExclusive(&_lock);
        NativeDiagramWorker::PreparedNode *node{};
        auto status = NativeDiagramWorker::Prepare(_shared->SharedControl(), model, key, ordinal, fonts, style, this,
                                                   Completed, &node, error, Admit);
        if (status != NOTEPADS_MARKDOWN_CORE_OK)
        {
            auto state = NativeDiagramPresentation::Classify(status);
            AcquireSRWLockExclusive(&_lock);
            if (_attempt == key.attempt)
                _attempts[ordinal] = static_cast<uint8_t>(state);
            ReleaseSRWLockExclusive(&_lock);
            Release();
            return state;
        }
        AcquireSRWLockExclusive(&_lock);
        auto stale = _attempt != key.attempt;
        if (!_head)
            _head = node;
        else
        {
            auto tail = _head;
            while (NativeDiagramWorker::NextPrepared(tail))
                tail = NativeDiagramWorker::NextPrepared(tail);
            NativeDiagramWorker::SetNextPrepared(tail, node);
        }
        NativeDiagramWorker::RetainPrepared(node);
        ReleaseSRWLockExclusive(&_lock);
        (void)NativeDiagramWorker::StartPrepared(node);
        if (stale)
            NativeDiagramWorker::CancelPrepared(node);
        NativeDiagramWorker::ReleasePrepared(node);
        return NativeDiagramState::Pending;
    }
    void NativeDiagramCoordinator::CancelAll() noexcept
    {
        NativeDiagramWorker::PreparedNode *nodes[NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT]{};
        uint32_t count{};
        AcquireSRWLockShared(&_lock);
        for (auto node = _head; node && count < NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT;
             node = NativeDiagramWorker::NextPrepared(node))
        {
            NativeDiagramWorker::RetainPrepared(node);
            nodes[count++] = node;
        }
        ReleaseSRWLockShared(&_lock);
        for (uint32_t i = 0; i < count; ++i)
        {
            NativeDiagramWorker::CancelPrepared(nodes[i]);
            NativeDiagramWorker::ReleasePrepared(nodes[i]);
        }
    }
    void NativeDiagramCoordinator::Wake() noexcept
    {
        NativeDiagramWorker::PreparedNode *nodes[NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT]{};
        uint32_t count{};
        AcquireSRWLockShared(&_lock);
        for (auto node = _head; node && count < NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT;
             node = NativeDiagramWorker::NextPrepared(node))
        {
            NativeDiagramWorker::RetainPrepared(node);
            nodes[count++] = node;
        }
        ReleaseSRWLockShared(&_lock);
        for (uint32_t i = 0; i < count; ++i)
        {
            NativeDiagramWorker::WakePrepared(nodes[i]);
            NativeDiagramWorker::ReleasePrepared(nodes[i]);
        }
    }
    void NativeDiagramCoordinator::NotifyResourcesAvailable() noexcept
    {
        NativeDiagramCoordinator *snapshot[256]{};
        AcquireSRWLockExclusive(&registryLock);
        ++wakeEpoch;
        if (notifying.test_and_set(std::memory_order_acquire))
        {
            ReleaseSRWLockExclusive(&registryLock);
            return;
        }
        for (;;)
        {
            auto epoch = wakeEpoch;
            uint32_t count{};
            for (auto owner = coordinators; owner && count < 256; owner = owner->_runtimeNext)
            {
                AcquireSRWLockShared(&owner->_lock);
                if (owner->_head && !owner->_quarantined)
                {
                    owner->AddRef();
                    snapshot[count++] = owner;
                }
                ReleaseSRWLockShared(&owner->_lock);
            }
            ReleaseSRWLockExclusive(&registryLock);
            for (uint32_t i = 0; i < count; ++i)
            {
                snapshot[i]->Wake();
                snapshot[i]->Release();
            }
            AcquireSRWLockExclusive(&registryLock);
            if (epoch == wakeEpoch)
            {
                notifying.clear(std::memory_order_release);
                ReleaseSRWLockExclusive(&registryLock);
                return;
            }
        }
    }
    bool NativeDiagramCoordinator::Admit(void *context, NativeDiagramWorker::PreparedNode *node) noexcept
    {
        auto self = static_cast<NativeDiagramCoordinator *>(context);
        AcquireSRWLockShared(&self->_lock);
        auto first = self->_head;
        while (first && !NativeDiagramWorker::IsQueued(first))
            first = NativeDiagramWorker::NextPrepared(first);
        auto result = first == node;
        ReleaseSRWLockShared(&self->_lock);
        return result;
    }
    void NativeDiagramCoordinator::Completed(void *context, notepads_markdown_diagram_job *,
                                             NativeDiagramWorkerResult const &result) noexcept
    {
        auto node = static_cast<NativeDiagramWorker::PreparedNode *>(context);
        auto self = static_cast<NativeDiagramCoordinator *>(NativeDiagramWorker::PreparedOwner(node));
        self->Finished(node, result);
    }
    void NativeDiagramCoordinator::Remove(NativeDiagramWorker::PreparedNode *node, NativeDiagramState state) noexcept
    {
        AcquireSRWLockExclusive(&_lock);
        auto current = _head;
        NativeDiagramWorker::PreparedNode *previous{};
        while (current && current != node)
        {
            previous = current;
            current = NativeDiagramWorker::NextPrepared(current);
        }
        if (current)
        {
            auto next = NativeDiagramWorker::NextPrepared(current);
            if (previous)
                NativeDiagramWorker::SetNextPrepared(previous, next);
            else
                _head = next;
            NativeDiagramWorker::SetNextPrepared(current, nullptr);
        }
        auto ordinal = NativeDiagramWorker::PreparedOrdinal(node);
        if (_attempt == NativeDiagramWorker::PreparedKey(node).attempt && ordinal < NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT)
            _attempts[ordinal] = static_cast<uint8_t>(state);
        ReleaseSRWLockExclusive(&_lock);
        NativeDiagramWorker::ClearPreparedOwner(node);
        NativeDiagramWorker::ReleasePrepared(node);
    }
    void NativeDiagramCoordinator::Finished(NativeDiagramWorker::PreparedNode *node,
                                            NativeDiagramWorkerResult const &result) noexcept
    {
        auto status = result.status;
        auto payload = NativeDiagramWorker::BuildPresentationAfterThreadExit(node, status);
        auto state = payload->State();
        if (state == NativeDiagramState::Ready)
        {
            auto attempt = NativeDiagramWorker::PreparedKey(node).attempt;
            auto ordinal = NativeDiagramWorker::PreparedOrdinal(node);
            AddRef();
            DerivedDeliveryReceipt receipt(this, attempt, ordinal, &SettleDelivery, [](void *owner) noexcept
                { static_cast<NativeDiagramCoordinator *>(owner)->Release(); });
            Remove(node, NativeDiagramState::Pending);
            (void)NativeDiagramWorker::Post(node, _sink, std::move(payload), std::move(receipt));
        }
        else
        {
            auto attempt = NativeDiagramWorker::PreparedKey(node).attempt;
            auto ordinal = NativeDiagramWorker::PreparedOrdinal(node);
            Remove(node, state);
            auto posted = NativeDiagramWorker::Post(node, _sink, std::move(payload));
            if (FAILED(posted))
            {
                AcquireSRWLockExclusive(&_lock);
                if (_attempt == attempt && ordinal < NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT)
                    _attempts[ordinal] = static_cast<uint8_t>(NativeDiagramState::Unavailable);
                ReleaseSRWLockExclusive(&_lock);
            }
        }
        NativeMathCoordinator::NotifyResourcesAvailable();
        Release();
    }
    void NativeDiagramCoordinator::SettleDelivery(void *owner, uint64_t attempt, uint32_t ordinal, bool accepted) noexcept
    {
        auto self = static_cast<NativeDiagramCoordinator *>(owner);
        AcquireSRWLockExclusive(&self->_lock);
        if (self->_attempt == attempt && ordinal < NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT &&
            self->_attempts[ordinal] == static_cast<uint8_t>(NativeDiagramState::Pending))
            self->_attempts[ordinal] = static_cast<uint8_t>(accepted ? NativeDiagramState::Ready : NativeDiagramState::Unavailable);
        ReleaseSRWLockExclusive(&self->_lock);
    }
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
    uint32_t NativeDiagramCoordinator::PendingCountForDiagnostics() const noexcept
    {
        uint32_t count{};
        AcquireSRWLockShared(&_lock);
        auto node = _head;
        while (node && count < NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT)
        {
            ++count;
            node = NativeDiagramWorker::NextPrepared(node);
        }
        bool complete = !node;
        ReleaseSRWLockShared(&_lock);
        return complete ? count : UINT32_MAX;
    }
#endif
}
