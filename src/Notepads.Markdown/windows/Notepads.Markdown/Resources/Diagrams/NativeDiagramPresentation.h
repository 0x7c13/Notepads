// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "notepads/markdown/diagram.h"
#include "Core/ModuleLifetime.h"
#include "Resources/DerivedDeliveryReceipt.h"
#include <atomic>
#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.UI.Core.h>

namespace Notepads::Markdown::Native
{
    struct NativeDiagramKey
    {
        notepads_markdown_core_identity identity{};
        uint64_t sourceGeneration{};
        uint64_t textGeneration{};
        uint64_t attempt{};
        uint64_t factoryEpoch{};
        uint64_t fontEpoch{};
        uint64_t styleEpoch{};
        uint64_t dpiEpoch{};
        uint64_t targetWidthEpoch{};
        uint64_t deviceEpoch{};
        double targetWidth{};
        double dpi{};
        uint32_t object{};
        uint32_t extensions{NOTEPADS_MARKDOWN_CORE_EXTENSIONS_MERMAID};
        bool operator==(NativeDiagramKey const &other) const noexcept;
    };

    enum class NativeDiagramState : uint8_t
    {
        None,
        Pending,
        Ready,
        Unsupported,
        Failed,
        Limited,
        Unavailable,
        Cancelled,
        FontUnavailable
    };

    class NativeDiagramPresentation;
    class DiagramPresentationHandle final
    {
      public:
        DiagramPresentationHandle() noexcept = default;
        DiagramPresentationHandle(DiagramPresentationHandle const &other) noexcept;
        DiagramPresentationHandle(DiagramPresentationHandle &&other) noexcept;
        DiagramPresentationHandle &operator=(DiagramPresentationHandle const &other) noexcept;
        DiagramPresentationHandle &operator=(DiagramPresentationHandle &&other) noexcept;
        ~DiagramPresentationHandle();
        NativeDiagramPresentation *operator->() const noexcept;
        explicit operator bool() const noexcept;
        void Reset() noexcept;
        static DiagramPresentationHandle Adopt(NativeDiagramPresentation *value) noexcept;

      private:
        NativeDiagramPresentation *_value{};
    };

    // Both Ready and typed failures occupy the prepaid native Node. Every
    // Frame/transport/stream retains that same actual owner, without a second
    // shared_ptr allocation or a second retained quota.
    class NativeDiagramPresentation final
    {
      public:
        using Destroy = void (*)(NativeDiagramPresentation *) noexcept;
        NativeDiagramPresentation(NativeDiagramKey const &key, NativeDiagramState state, int32_t status,
                                  uint8_t const *bytes, uint64_t count, double width, double height, Destroy destroy,
                                  void *owner) noexcept;
        NativeDiagramKey const &Key() const noexcept;
        NativeDiagramState State() const noexcept;
        int32_t Status() const noexcept;
        uint8_t const *Bytes() const noexcept;
        uint64_t ByteCount() const noexcept;
        double Width() const noexcept;
        double Height() const noexcept;
        DiagramPresentationHandle const &Next() const noexcept;
        bool LinkBefore(DiagramPresentationHandle const &head) noexcept;
        bool Linked() const noexcept { return _linked.load(std::memory_order_acquire); }
        static NativeDiagramPresentation const *Find(DiagramPresentationHandle const &head,
                                                     NativeDiagramKey const &key) noexcept;
        static NativeDiagramState Classify(int32_t status) noexcept;
        void *Owner() const noexcept;
        void AddRef() noexcept;
        void Release() noexcept;

      private:
        std::atomic<uint32_t> _references{1};
        std::atomic<bool> _linked{};
        NativeDiagramKey _key;
        NativeDiagramState _state;
        int32_t _status{};
        uint8_t const *_bytes{};
        uint64_t _count{};
        double _width{}, _height{};
        Destroy _destroy{};
        void *_owner{};
        DiagramPresentationHandle _next;
    };

    struct NativeDiagramDeliverySink
    {
        winrt::weak_ref<winrt::Windows::Foundation::IInspectable> target;
        winrt::Windows::UI::Core::CoreDispatcher dispatcher{nullptr};
        void (*invoke)(winrt::Windows::Foundation::IInspectable const &, DiagramPresentationHandle const &) noexcept {};
    };

    class NativeDiagramTransport final
    {
      public:
        static constexpr size_t EmbeddedBytes = 768;
        static HRESULT PostEmbedded(NativeDiagramDeliverySink const &sink, DiagramPresentationHandle payload,
                                    void *storage, size_t bytes, void *owner,
                                    void (*releaseStorage)(void *) noexcept, DerivedDeliveryReceipt receipt = {}) noexcept;
    };
}
