// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "notepads/markdown/core.h"
#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.UI.Core.h>
#include <array>
#include <atomic>
#include <memory>
#include <mutex>
#include <span>

namespace Notepads::Markdown::Native
{
    struct CoreControllerState;
    struct CoreControllerLedger;
    struct CoreCompletionRecord;

    // An exact request completion; cancellation never targets a successor.
    class CoreCompletionTicket
    {
      public:
        CoreCompletionTicket() noexcept = default;
        explicit CoreCompletionTicket(std::shared_ptr<CoreCompletionRecord> record) noexcept;
        HANDLE EventHandle() const noexcept;
        uint64_t Serial() const noexcept;
        bool Done() const noexcept;
        int32_t Status() const noexcept;
        notepads_markdown_core_error Error() const noexcept;
        void Cancel() const noexcept;
        explicit operator bool() const noexcept
        {
            return static_cast<bool>(_record);
        }

      private:
        std::shared_ptr<CoreCompletionRecord> _record;
    };

    // These are native owner observations, not Rust quota or physical-memory measurements.
    struct CoreLifetimeSnapshot
    {
        uint32_t states{};
        uint32_t stateControlBlocks{};
        uint32_t driverDelegates{};
        uint32_t uiDelegates{};
        uint32_t requestObjects{};
        uint32_t requestControlBlocks{};
        uint32_t completionControlBlocks{};
        uint32_t modelPins{};
        uint32_t controlHandles{};
        // Observation storage/tickets keep their own init Rust lease alive. They
        // are excluded from fullyReleased and cannot observe their own last free.
        uint32_t observationTickets{};
        uint32_t observationAllocationBlocks{};
        bool fullyReleased{};
        int32_t cleanupStatus{};
        int32_t cleanupHresult{};
    };

    class CoreControllerLifetime
    {
      public:
        CoreControllerLifetime() noexcept = default;
        ~CoreControllerLifetime();
        CoreControllerLifetime(CoreControllerLifetime const &other) noexcept;
        CoreControllerLifetime &operator=(CoreControllerLifetime const &other) noexcept;
        CoreControllerLifetime(CoreControllerLifetime &&other) noexcept;
        CoreControllerLifetime &operator=(CoreControllerLifetime &&other) noexcept;
        CoreLifetimeSnapshot Snapshot() const noexcept;

      private:
        friend class CoreController;
        std::shared_ptr<CoreControllerLedger> _ledger;
    };

    enum class CoreWakePhase : uint8_t
    {
        Idle,
        Posting,
        InFlight,
        Tail
    };
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
    enum class CoreTestPoint : uint8_t
    {
        PreparingAfterStart,
        DriverBeforeRun,
        DriverBeforePost,
        DriverAfterPost,
        UiBeforeEntry,
        UiBeforeClaim,
        UiAfterClaim,
        UiAfterAck,
        DriverBeforeIdleWait,
        DriverBeforeSubmitWait,
        SubmitBeforeEndBorrow,
        DrainBeforeWait,
        DrainAfterWait,
        RequestBeforeCleanup,
        Count
    };

    // Optional, preallocated correctness gates. Production scheduling never depends on them.
    class CoreControllerTestHooks
    {
      public:
        CoreControllerTestHooks();
        void Arm(CoreTestPoint point, uint32_t maxHoldMilliseconds = 0xFFFFFFFFu) noexcept;
        void Release(CoreTestPoint point) noexcept;
        void ReleaseAll() noexcept;
        bool WaitReached(CoreTestPoint point, uint32_t timeoutMilliseconds) const noexcept;
        void Reach(CoreTestPoint point) noexcept;
        bool AnyTimedOut() const noexcept;
        void RetainRequestWeak(bool enabled) noexcept;
        void ReleaseRequestWeak() noexcept;
        bool RetainedRequestExpired() const noexcept;
        void ObserveRequest(std::shared_ptr<void> const &request) noexcept;

      private:
        struct Gate
        {
            std::atomic<bool> armed{};
            std::atomic<bool> timedOut{};
            std::atomic<uint32_t> maxHoldMilliseconds{0xFFFFFFFFu};
            winrt::handle reached;
            winrt::handle release;
        };
        std::array<Gate, static_cast<size_t>(CoreTestPoint::Count)> _gates;
        mutable std::mutex _weakMutex;
        bool _retainWeak{};
        std::weak_ptr<void> _requestWeak;
    };

#endif
    struct CoreSubmitResult
    {
        uint64_t serial{};
        int32_t status{NOTEPADS_MARKDOWN_CORE_OK};
        notepads_markdown_core_error error{};
        notepads_markdown_core_submit_result sourceState{};
    };

    struct CoreControllerSnapshot
    {
        bool closed{};
        bool preparing{};
        bool submitEntered{};
        bool installEntered{};
        uint32_t active{};
        uint32_t pending{};
        uint64_t activeSerial{};
        uint64_t pendingSerial{};
        uint64_t lastSubmitted{};
        uint64_t lastInstalled{};
        uint64_t retired{};
        uint64_t runCount{};
        uint64_t installedCount{};
        uint32_t driverStartCount{};
        CoreWakePhase wakePhase{};
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
        int32_t primaryStatus{};
        int32_t cleanupStatus{};
        int32_t primaryHresult{};
        int32_t cleanupHresult{};
        notepads_markdown_core_submit_result sourceState{};
        CoreLifetimeSnapshot lifetime{};
    };

    struct CoreDrainResult
    {
        bool offUi{};
        bool coreDrained{};
        bool driverExited{};
        bool canceled{};
        int32_t primaryStatus{};
        int32_t cleanupStatus{};
        int32_t primaryHresult{};
        int32_t cleanupHresult{};
    };

    class CoreModelPin
    {
      public:
        CoreModelPin() noexcept = default;
        ~CoreModelPin();
        CoreModelPin(CoreModelPin const &) = delete;
        CoreModelPin &operator=(CoreModelPin const &) = delete;
        CoreModelPin(CoreModelPin &&other) noexcept;
        CoreModelPin &operator=(CoreModelPin &&other) noexcept;
        notepads_markdown_core_model *Get() const noexcept
        {
            return _model;
        }
        explicit operator bool() const noexcept
        {
            return _model != nullptr;
        }
        void Reset() noexcept;
        CoreModelPin Clone() const;

      private:
        friend class CoreController;
        static CoreModelPin CloneModel(notepads_markdown_core_model *model, std::shared_ptr<CoreControllerLedger> const &ledger);
        CoreModelPin(notepads_markdown_core_model *model, std::shared_ptr<CoreControllerLedger> ledger) noexcept;
        notepads_markdown_core_model *_model{};
        std::shared_ptr<CoreControllerLedger> _ledger;
    };

    // Native-only owner. Create/Submit/PinInstalled require the supplied UI dispatcher.
    // Stop and RequestCancel may be called concurrently. Drain is exclusive and off UI.
    // Destruction is a Stop fallback; only explicit Drain observes the driver terminal state.
    class CoreController
    {
      public:
        static std::unique_ptr<CoreController> Create(winrt::Windows::UI::Core::CoreDispatcher const &dispatcher,
                                                      uint32_t explicitProfile
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                                                      , std::shared_ptr<CoreControllerTestHooks> hooks = {}
#endif
        );
        static std::unique_ptr<CoreController> CreatePreview(winrt::Windows::UI::Core::CoreDispatcher const &dispatcher
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                                                             , std::shared_ptr<CoreControllerTestHooks> hooks = {}
#endif
        );
        static std::unique_ptr<CoreController> CreateEmbeddedPreview(
            winrt::Windows::UI::Core::CoreDispatcher const &dispatcher, CoreModelPin const &parent);
        ~CoreController();
        void operator delete(void *memory) noexcept;
        CoreController(CoreController const &) = delete;
        CoreController &operator=(CoreController const &) = delete;
        CoreSubmitResult SubmitUtf8(std::span<uint8_t const> input, notepads_markdown_core_snapshot const &descriptor,
                                    CoreCompletionTicket *completion = nullptr, uint32_t extensions = 0) noexcept;
        CoreSubmitResult SubmitUtf16(std::span<uint16_t const> input, notepads_markdown_core_snapshot const &descriptor,
                                     uint32_t extensions = 0) noexcept;
        CoreSubmitResult SubmitEdits(notepads_markdown_core_edit_batch const &descriptor, std::span<notepads_markdown_core_edit_row const> rows,
                                     std::span<uint8_t const> replacements,
                                     CoreCompletionTicket *completion = nullptr) noexcept;
        CoreSubmitResult CaptureCurrent() noexcept;
        void RequestCancel() noexcept;
        void Stop() noexcept;
        CoreDrainResult Drain() noexcept;
        CoreControllerSnapshot Snapshot() const noexcept;
        CoreModelPin PinInstalled();
        CoreControllerLifetime Lifetime() const noexcept;

      private:
        static std::unique_ptr<CoreController> CreateInternal(
            winrt::Windows::UI::Core::CoreDispatcher const &dispatcher, uint32_t explicitProfile,
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            std::shared_ptr<CoreControllerTestHooks> hooks,
#endif
            bool preview, notepads_markdown_core_model const *parent = nullptr);
        CoreController() noexcept = default;
        std::shared_ptr<CoreControllerState> _state;
        winrt::Windows::Foundation::IAsyncAction _driver{nullptr};
        std::atomic<bool> _drainEntered{};
    };
}
