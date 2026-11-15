// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Resources/Diagrams/NativeDiagramPresentation.h"
#include <cstring>
#include <new>

namespace Notepads::Markdown::Native
{
    bool NativeDiagramKey::operator==(NativeDiagramKey const &other) const noexcept
    {
        return std::memcmp(&identity, &other.identity, sizeof(identity)) == 0 &&
               sourceGeneration == other.sourceGeneration && textGeneration == other.textGeneration &&
               attempt == other.attempt && factoryEpoch == other.factoryEpoch && fontEpoch == other.fontEpoch &&
               styleEpoch == other.styleEpoch && dpiEpoch == other.dpiEpoch &&
               targetWidthEpoch == other.targetWidthEpoch && deviceEpoch == other.deviceEpoch &&
               targetWidth == other.targetWidth && dpi == other.dpi && object == other.object &&
               extensions == other.extensions;
    }
    DiagramPresentationHandle::DiagramPresentationHandle(DiagramPresentationHandle const &other) noexcept
        : _value(other._value)
    {
        if (_value)
            _value->AddRef();
    }
    DiagramPresentationHandle::DiagramPresentationHandle(DiagramPresentationHandle &&other) noexcept
        : _value(std::exchange(other._value, nullptr))
    {
    }
    DiagramPresentationHandle &DiagramPresentationHandle::operator=(DiagramPresentationHandle const &other) noexcept
    {
        auto copy = other;
        std::swap(_value, copy._value);
        return *this;
    }
    DiagramPresentationHandle &DiagramPresentationHandle::operator=(DiagramPresentationHandle &&other) noexcept
    {
        if (this != &other)
        {
            Reset();
            _value = std::exchange(other._value, nullptr);
        }
        return *this;
    }
    DiagramPresentationHandle::~DiagramPresentationHandle()
    {
        Reset();
    }
    NativeDiagramPresentation *DiagramPresentationHandle::operator->() const noexcept
    {
        return _value;
    }
    DiagramPresentationHandle::operator bool() const noexcept
    {
        return _value != nullptr;
    }
    void DiagramPresentationHandle::Reset() noexcept
    {
        if (auto value = std::exchange(_value, nullptr))
            value->Release();
    }
    DiagramPresentationHandle DiagramPresentationHandle::Adopt(NativeDiagramPresentation *value) noexcept
    {
        DiagramPresentationHandle result;
        result._value = value;
        return result;
    }

    NativeDiagramPresentation::NativeDiagramPresentation(NativeDiagramKey const &key, NativeDiagramState state,
                                                         int32_t status, uint8_t const *bytes, uint64_t count,
                                                         double width, double height, Destroy destroy,
                                                         void *owner) noexcept
        : _key(key), _state(state), _status(status), _bytes(bytes), _count(count), _width(width), _height(height),
          _destroy(destroy), _owner(owner)
    {
    }
    NativeDiagramKey const &NativeDiagramPresentation::Key() const noexcept
    {
        return _key;
    }
    NativeDiagramState NativeDiagramPresentation::State() const noexcept
    {
        return _state;
    }
    int32_t NativeDiagramPresentation::Status() const noexcept
    {
        return _status;
    }
    uint8_t const *NativeDiagramPresentation::Bytes() const noexcept
    {
        return _bytes;
    }
    uint64_t NativeDiagramPresentation::ByteCount() const noexcept
    {
        return _count;
    }
    double NativeDiagramPresentation::Width() const noexcept
    {
        return _width;
    }
    double NativeDiagramPresentation::Height() const noexcept
    {
        return _height;
    }
    DiagramPresentationHandle const &NativeDiagramPresentation::Next() const noexcept
    {
        return _next;
    }
    void *NativeDiagramPresentation::Owner() const noexcept
    {
        return _owner;
    }
    void NativeDiagramPresentation::AddRef() noexcept
    {
        _references.fetch_add(1, std::memory_order_relaxed);
    }
    void NativeDiagramPresentation::Release() noexcept
    {
        if (_references.fetch_sub(1, std::memory_order_acq_rel) == 1)
            _destroy(this);
    }
    bool NativeDiagramPresentation::LinkBefore(DiagramPresentationHandle const &head) noexcept
    {
        auto node = head ? head.operator->() : nullptr;
        for (uint32_t count = 0; node; ++count)
        {
            if (node == this || count >= NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT - 1)
                return false;
            node = node->Next() ? node->Next().operator->() : nullptr;
        }
        bool expected{};
        if (!_linked.compare_exchange_strong(expected, true, std::memory_order_acq_rel))
            return false;
        _next = head;
        return true;
    }
    NativeDiagramPresentation const *NativeDiagramPresentation::Find(DiagramPresentationHandle const &head,
                                                                     NativeDiagramKey const &key) noexcept
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
    NativeDiagramState NativeDiagramPresentation::Classify(int32_t status) noexcept
    {
        switch (status)
        {
        case NOTEPADS_MARKDOWN_CORE_OK:
            return NativeDiagramState::Ready;
        case NOTEPADS_MARKDOWN_CORE_CANCELLED:
        case NOTEPADS_MARKDOWN_CORE_STALE_RESULT:
            return NativeDiagramState::Cancelled;
        case NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY:
        case NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT:
        case NOTEPADS_MARKDOWN_CORE_DEADLINE:
            return NativeDiagramState::Limited;
        case NOTEPADS_MARKDOWN_CORE_UNSUPPORTED:
            return NativeDiagramState::Unsupported;
        case NOTEPADS_MARKDOWN_DIAGRAM_STACK_UNAVAILABLE:
            return NativeDiagramState::Unavailable;
        case NOTEPADS_MARKDOWN_DIAGRAM_FONT_UNAVAILABLE:
            return NativeDiagramState::FontUnavailable;
        default:
            return NativeDiagramState::Failed;
        }
    }

    namespace
    {
        using Dispatch = winrt::Windows::UI::Core::DispatchedHandler;
        using Completed = winrt::Windows::Foundation::AsyncOperationCompletedHandler<bool>;
        using Operation = winrt::Windows::Foundation::IAsyncOperation<bool>;
        struct Transport final : winrt::impl::abi_t<Dispatch>, winrt::impl::abi_t<Completed>
        {
            std::atomic<uint32_t> references{1};
            std::atomic<bool> settled{};
            ModuleLifetime module;
            NativeDiagramDeliverySink sink;
            DiagramPresentationHandle payload;
            DerivedDeliveryReceipt receipt;
            void *storageOwner{};
            void (*releaseStorage)(void *) noexcept {};
            Transport(NativeDiagramDeliverySink const &value, DiagramPresentationHandle item, void *owner,
                      void (*release)(void *) noexcept, DerivedDeliveryReceipt delivery) noexcept
                : sink(value), payload(std::move(item)), receipt(std::move(delivery)), storageOwner(owner), releaseStorage(release)
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
                    this->~Transport();
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
        static_assert(sizeof(Transport) <= NativeDiagramTransport::EmbeddedBytes);
    }
    HRESULT NativeDiagramTransport::PostEmbedded(NativeDiagramDeliverySink const &sink,
                                                 DiagramPresentationHandle payload, void *storage, size_t bytes,
                                                 void *owner, void (*releaseStorage)(void *) noexcept,
                                                 DerivedDeliveryReceipt receipt) noexcept
    {
        if (!storage || bytes < sizeof(Transport) || !releaseStorage || !payload)
        {
            if (releaseStorage)
                releaseStorage(owner);
            return E_INVALIDARG;
        }
        auto record = new (storage) Transport(sink, std::move(payload), owner, releaseStorage, std::move(receipt));
        HRESULT result = S_OK;
        try
        {
            if (!record->sink.dispatcher || !record->sink.invoke)
                winrt::throw_hresult(E_INVALIDARG);
            Dispatch handler{nullptr};
            Completed completed{nullptr};
            winrt::copy_from_abi(handler, static_cast<winrt::impl::abi_t<Dispatch> *>(record));
            winrt::copy_from_abi(completed, static_cast<winrt::impl::abi_t<Completed> *>(record));
            auto operation =
                record->sink.dispatcher.TryRunAsync(winrt::Windows::UI::Core::CoreDispatcherPriority::Normal, handler);
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
