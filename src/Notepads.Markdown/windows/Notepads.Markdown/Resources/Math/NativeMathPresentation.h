// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#include <mutex>
#include "Resources/Math/NativeMathMemory.h"
#include "Resources/Math/NativeMathGeometry.h"
#include "Resources/DerivedDeliveryReceipt.h"
#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.UI.Core.h>

namespace Notepads::Markdown::Native
{
    struct NativeMathKey
    {
        notepads_markdown_core_identity identity{};
        uint64_t sourceGeneration{};
        uint64_t textGeneration{};
        uint64_t attempt{};
        uint64_t factoryEpoch{};
        uint64_t engine{};
        uint64_t fonts{};
        uint64_t style{};
        uint32_t object{};
        uint32_t syntax{};
        bool operator==(NativeMathKey const &other) const noexcept;
    };

    enum class NativeMathState : uint8_t
    {
        None,
        Pending,
        Ready,
        Unsupported,
        Failed,
        Limited,
        Unavailable,
        Cancelled
    };

    class NativeMathPresentation;
    class MathPresentationHandle final
    {
      public:
        MathPresentationHandle() noexcept = default;
        MathPresentationHandle(MathPresentationHandle const &other) noexcept;
        MathPresentationHandle(MathPresentationHandle &&other) noexcept;
        MathPresentationHandle &operator=(MathPresentationHandle const &other) noexcept;
        MathPresentationHandle &operator=(MathPresentationHandle &&other) noexcept;
        ~MathPresentationHandle();
        NativeMathPresentation *operator->() const noexcept;
        explicit operator bool() const noexcept;
        void Reset() noexcept;
        static MathPresentationHandle Adopt(NativeMathPresentation *value) noexcept;

      private:
        NativeMathPresentation *_value{};
    };

    // Immutable after the UI links it once, before publishing to any Frame.
    // Ready storage and transport occupy two of the prepaid sixteen arena slots.
    // Failed storage and transport are embedded in their independently funded Node.
    class NativeMathPresentation final
    {
      public:
        using Destroy = void (*)(NativeMathPresentation *) noexcept;
        NativeMathPresentation(NativeMathKey const &key, NativeMathState state, int32_t status, Destroy destroy,
                               void *owner) noexcept;
        ~NativeMathPresentation() = default;
        NativeMathKey const &Key() const noexcept;
        NativeMathState State() const noexcept;
        int32_t Status() const noexcept;
        std::shared_ptr<NativeMathDrawing> const &Drawing() const noexcept;
        MathMemoryHandle const &Memory() const noexcept;
        MathPresentationHandle const &Next() const noexcept;
        bool LinkBefore(MathPresentationHandle const &head) noexcept;
        bool Linked() const noexcept { return _linked.load(std::memory_order_acquire); }
        static NativeMathPresentation const *Find(MathPresentationHandle const &head,
                                                  NativeMathKey const &key) noexcept;
        static MathPresentationHandle CreateReady(NativeMathKey const &key, MathMemoryHandle memory,
                                                  std::shared_ptr<NativeMathDrawing> drawing);
        static NativeMathState Classify(int32_t status) noexcept;
        void *Owner() const noexcept;
        void AddRef() noexcept;
        void Release() noexcept;

      private:
        std::atomic<uint32_t> _references{1};
        std::atomic<bool> _linked{};
        Destroy _destroy{};
        void *_owner{};
        NativeMathKey _key;
        NativeMathState _state;
        int32_t _status{};
        MathMemoryHandle _memory;
        std::shared_ptr<NativeMathDrawing> _drawing;
        MathPresentationHandle _next;
    };

    struct NativeMathDeliverySink
    {
        winrt::weak_ref<winrt::Windows::Foundation::IInspectable> target;
        winrt::Windows::UI::Core::CoreDispatcher dispatcher{nullptr};
        // Private control implementation only. Resolve occurs solely in UI Invoke.
        // Check loaded/source/opt-in/attempt/factory epoch before adopting payload.
        void (*invoke)(winrt::Windows::Foundation::IInspectable const &, MathPresentationHandle const &) noexcept {};
    };

    class NativeMathTransport final
    {
      public:
        // Returns after dispatch registration, never waits for UI. On failure
        // all placement storage is freed and payload ownership is returned.
        static HRESULT Post(NativeMathDeliverySink const &sink, MathPresentationHandle payload,
                            DerivedDeliveryReceipt receipt = {}) noexcept;
        // Failure Node supplies prepaid space and an independent intrusive owner.
        // releaseStorage runs only after both real COM delegate refs are gone.
        static HRESULT PostEmbedded(NativeMathDeliverySink const &sink, MathPresentationHandle payload, void *storage,
                                    size_t bytes, void *owner, void (*releaseStorage)(void *) noexcept) noexcept;
        static constexpr size_t EmbeddedBytes = 768;
    };
}
