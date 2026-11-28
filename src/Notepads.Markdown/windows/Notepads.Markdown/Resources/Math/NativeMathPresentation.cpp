// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Resources/Math/NativeMathPresentation.h"
#include <cstring>
#include <new>

namespace Notepads::Markdown::Native
{
    bool NativeMathKey::operator==(NativeMathKey const &other) const noexcept
    {
        return std::memcmp(&identity, &other.identity, sizeof(identity)) == 0 &&
               sourceGeneration == other.sourceGeneration && textGeneration == other.textGeneration &&
               attempt == other.attempt && factoryEpoch == other.factoryEpoch && engine == other.engine &&
               fonts == other.fonts && style == other.style && object == other.object && syntax == other.syntax;
    }
    MathPresentationHandle::MathPresentationHandle(MathPresentationHandle const &other) noexcept : _value(other._value)
    {
        if (_value)
            _value->AddRef();
    }
    MathPresentationHandle::MathPresentationHandle(MathPresentationHandle &&other) noexcept
        : _value(std::exchange(other._value, nullptr))
    {
    }
    MathPresentationHandle &MathPresentationHandle::operator=(MathPresentationHandle const &other) noexcept
    {
        auto copy = other;
        std::swap(_value, copy._value);
        return *this;
    }
    MathPresentationHandle &MathPresentationHandle::operator=(MathPresentationHandle &&other) noexcept
    {
        if (this != &other)
        {
            Reset();
            _value = std::exchange(other._value, nullptr);
        }
        return *this;
    }
    MathPresentationHandle::~MathPresentationHandle()
    {
        Reset();
    }
    NativeMathPresentation *MathPresentationHandle::operator->() const noexcept
    {
        return _value;
    }
    MathPresentationHandle::operator bool() const noexcept
    {
        return _value != nullptr;
    }
    void MathPresentationHandle::Reset() noexcept
    {
        if (auto value = std::exchange(_value, nullptr))
            value->Release();
    }
    MathPresentationHandle MathPresentationHandle::Adopt(NativeMathPresentation *value) noexcept
    {
        MathPresentationHandle result;
        result._value = value;
        return result;
    }
    NativeMathPresentation::NativeMathPresentation(NativeMathKey const &key, NativeMathState state, int32_t status,
                                                   Destroy destroy, void *owner) noexcept
        : _destroy(destroy), _owner(owner), _key(key), _state(state), _status(status)
    {
    }
    NativeMathKey const &NativeMathPresentation::Key() const noexcept
    {
        return _key;
    }
    NativeMathState NativeMathPresentation::State() const noexcept
    {
        return _state;
    }
    int32_t NativeMathPresentation::Status() const noexcept
    {
        return _status;
    }
    std::shared_ptr<NativeMathDrawing> const &NativeMathPresentation::Drawing() const noexcept
    {
        return _drawing;
    }
    MathMemoryHandle const &NativeMathPresentation::Memory() const noexcept
    {
        return _memory;
    }
    MathPresentationHandle const &NativeMathPresentation::Next() const noexcept
    {
        return _next;
    }
    void *NativeMathPresentation::Owner() const noexcept
    {
        return _owner;
    }
    void NativeMathPresentation::AddRef() noexcept
    {
        _references.fetch_add(1, std::memory_order_relaxed);
    }
    void NativeMathPresentation::Release() noexcept
    {
        if (_references.fetch_sub(1, std::memory_order_acq_rel) == 1)
            _destroy(this);
    }
    bool NativeMathPresentation::LinkBefore(MathPresentationHandle const &head) noexcept
    {
        auto node = head ? head.operator->() : nullptr;
        uint32_t count{};
        while (node)
        {
            if (node == this || ++count >= NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT)
                return false;
            node = node->Next() ? node->Next().operator->() : nullptr;
        }
        bool expected{};
        if (!_linked.compare_exchange_strong(expected, true, std::memory_order_acq_rel))
            return false;
        _next = head;
        return true;
    }
    NativeMathPresentation const *NativeMathPresentation::Find(MathPresentationHandle const &head,
                                                               NativeMathKey const &key) noexcept
    {
        auto node = head ? head.operator->() : nullptr;
        for (uint32_t count = 0; node && count < NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT; ++count)
        {
            if (node->Key() == key)
                return node;
            node = node->Next() ? node->Next().operator->() : nullptr;
        }
        return nullptr;
    }
    MathPresentationHandle NativeMathPresentation::CreateReady(NativeMathKey const &key, MathMemoryHandle memory,
                                                               std::shared_ptr<NativeMathDrawing> drawing)
    {
        if (!memory || !memory->IsFinalized() || !drawing)
            throw MathMemoryFailure{NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
        auto storage = memory->AllocateInline(sizeof(NativeMathPresentation), alignof(NativeMathPresentation));
        auto value = new (storage) NativeMathPresentation(
            key, NativeMathState::Ready, NOTEPADS_MARKDOWN_CORE_OK,
            [](NativeMathPresentation *self) noexcept
            {
                ModuleLifetime tail;
                self->~NativeMathPresentation();
                (void)NativeMathMemory::Free(self);
            },
            nullptr);
        value->_memory = std::move(memory);
        value->_drawing = std::move(drawing);
        return MathPresentationHandle::Adopt(value);
    }
    NativeMathState NativeMathPresentation::Classify(int32_t status) noexcept
    {
        switch (status)
        {
        case NOTEPADS_MARKDOWN_CORE_OK:
            return NativeMathState::Ready;
        case NOTEPADS_MARKDOWN_CORE_CANCELLED:
        case NOTEPADS_MARKDOWN_CORE_STALE_RESULT:
            return NativeMathState::Cancelled;
        case NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT:
        case NOTEPADS_MARKDOWN_CORE_DEADLINE:
        case NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY:
            return NativeMathState::Limited;
        case NOTEPADS_MARKDOWN_CORE_UNSUPPORTED:
        case NOTEPADS_MARKDOWN_MATH_MISSING_FONT:
        case NOTEPADS_MARKDOWN_MATH_MISSING_GLYPH:
            return NativeMathState::Unsupported;
        case NOTEPADS_MARKDOWN_MATH_STACK_UNAVAILABLE:
            return NativeMathState::Unavailable;
        default:
            return NativeMathState::Failed;
        }
    }

    namespace
    {
        using Dispatch = winrt::Windows::UI::Core::DispatchedHandler;
        using Completed = winrt::Windows::Foundation::AsyncOperationCompletedHandler<bool>;
        using Operation = winrt::Windows::Foundation::IAsyncOperation<bool>;
        // Both COM interfaces share one fixed allocation and one refcount. The
        // operation owns delegates; this object deliberately never owns it.
        struct Transport final : winrt::impl::abi_t<Dispatch>, winrt::impl::abi_t<Completed>
        {
            std::atomic<uint32_t> references{1};
            std::atomic<bool> settled{};
            ModuleLifetime module;
            NativeMathDeliverySink sink;
            MathPresentationHandle payload;
            MathMemoryHandle memory;
            DerivedDeliveryReceipt receipt;
            void *storageOwner{};
            void (*releaseStorage)(void *) noexcept {};
            Transport(NativeMathDeliverySink const &value, MathPresentationHandle item, MathMemoryHandle owner,
                      void *external, void (*release)(void *) noexcept, DerivedDeliveryReceipt delivery = {}) noexcept
                : sink(value), payload(std::move(item)), memory(std::move(owner)), receipt(std::move(delivery)),
                  storageOwner(external), releaseStorage(release)
            {
            }
            int32_t __stdcall QueryInterface(winrt::guid const &id, void **output) noexcept override
            {
                if (!output)
                    return E_POINTER;
                *output = nullptr;
                if (id == winrt::guid_of<Dispatch>() || id == winrt::guid_of<winrt::Windows::Foundation::IUnknown>() ||
                    id == winrt::guid_of<IAgileObject>())
                    *output = static_cast<winrt::impl::abi_t<Dispatch> *>(this);
                else if (id == winrt::guid_of<Completed>())
                    *output = static_cast<winrt::impl::abi_t<Completed> *>(this);
                else
                    return E_NOINTERFACE;
                AddRef();
                return S_OK;
            }
            uint32_t __stdcall AddRef() noexcept override
            {
                return references.fetch_add(1) + 1;
            }
            uint32_t __stdcall Release() noexcept override
            {
                auto left = references.fetch_sub(1, std::memory_order_acq_rel) - 1;
                if (!left)
                {
                    ModuleLifetime tail;
                    auto owner = storageOwner;
                    auto release = releaseStorage;
                    bool arena = static_cast<bool>(memory);
                    this->~Transport();
                    if (arena)
                        (void)NativeMathMemory::Free(this);
                    else if (release)
                        release(owner);
                }
                return left;
            }
            void Drop() noexcept
            {
                if (settled.exchange(true, std::memory_order_acq_rel))
                    return;
                receipt.Complete(false);
                payload.Reset();
                sink = {};
            }
            int32_t __stdcall Invoke() noexcept override
            {
                if (settled.exchange(true, std::memory_order_acq_rel))
                    return S_OK;
                auto item = std::move(payload);
                try
                {
                    // This is the only weak resolution and the only UI call.
                    if (auto target = sink.target.get(); target && sink.invoke)
                        sink.invoke(target, item);
                }
                catch (...)
                {
                }
                receipt.Complete(item && item->Linked());
                sink = {};
                return S_OK;
            }
            int32_t __stdcall Invoke(void *operation, int32_t) noexcept override
            {
                // GetResults observes false/errors; either way an uninvoked
                // request closes here. A running Invoke won the settle first.
                try
                {
                    Operation current{nullptr};
                    winrt::copy_from_abi(current, operation);
                    (void)current.GetResults();
                }
                catch (...)
                {
                }
                Drop();
                return S_OK;
            }
        };
        static_assert(sizeof(Transport) <= NativeMathTransport::EmbeddedBytes);

        HRESULT Send(Transport *record) noexcept
        {
            HRESULT result = S_OK;
            try
            {
                if (!record->sink.dispatcher || !record->sink.invoke)
                    winrt::throw_hresult(E_INVALIDARG);
                auto dispatcher = record->sink.dispatcher;
                // Initial local reference protects synchronous completion and
                // Invoke reentry, including exceptions during registration.
                Dispatch handler{nullptr};
                Completed completed{nullptr};
                winrt::copy_from_abi(handler, static_cast<winrt::impl::abi_t<Dispatch> *>(record));
                winrt::copy_from_abi(completed, static_cast<winrt::impl::abi_t<Completed> *>(record));
                auto operation =
                    dispatcher.TryRunAsync(winrt::Windows::UI::Core::CoreDispatcherPriority::Normal, handler);
                operation.Completed(completed);
            }
            catch (...)
            {
                result = winrt::to_hresult();
                record->Drop();
            }
            record->Release();
            return result;
        }
    }
    HRESULT NativeMathTransport::Post(NativeMathDeliverySink const &sink, MathPresentationHandle payload,
                                      DerivedDeliveryReceipt receipt) noexcept
    {
        try
        {
            if (!payload || !payload->Memory())
                return E_INVALIDARG;
            auto memory = payload->Memory();
            auto storage = memory->AllocateInline(sizeof(Transport), alignof(Transport));
            return Send(new (storage) Transport(sink, std::move(payload), std::move(memory), nullptr, nullptr,
                                                std::move(receipt)));
        }
        catch (...)
        {
            return winrt::to_hresult();
        }
    }
    HRESULT NativeMathTransport::PostEmbedded(NativeMathDeliverySink const &sink, MathPresentationHandle payload,
                                              void *storage, size_t bytes, void *owner,
                                              void (*releaseStorage)(void *) noexcept) noexcept
    {
        if (!storage || bytes < sizeof(Transport) || !releaseStorage || !payload)
        {
            if (releaseStorage)
                releaseStorage(owner);
            return E_INVALIDARG;
        }
        return Send(new (storage) Transport(sink, std::move(payload), {}, owner, releaseStorage));
    }
}
