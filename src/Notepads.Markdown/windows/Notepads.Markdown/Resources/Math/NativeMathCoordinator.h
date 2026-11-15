// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include <array>
#include "Resources/Math/NativeMathPresentation.h"
#include "Resources/Math/NativeMathWorker.h"

namespace Notepads::Markdown::Native
{
    class NativeMathCoordinator;
    class MathCoordinatorHandle final
    {
      public:
        MathCoordinatorHandle() noexcept = default;
        MathCoordinatorHandle(MathCoordinatorHandle const &other) noexcept;
        MathCoordinatorHandle(MathCoordinatorHandle &&other) noexcept;
        MathCoordinatorHandle &operator=(MathCoordinatorHandle const &other) noexcept;
        MathCoordinatorHandle &operator=(MathCoordinatorHandle &&other) noexcept;
        ~MathCoordinatorHandle();
        NativeMathCoordinator *operator->() const noexcept;
        explicit operator bool() const noexcept;
        void Reset() noexcept;

      private:
        friend class NativeMathCoordinator;
        explicit MathCoordinatorHandle(NativeMathCoordinator *value) noexcept;
        NativeMathCoordinator *_value{};
    };

    // All known coordinator capacity is inside the funded <=256-byte header.
    // No terminal/Ready strong head is held here: the control/Frames own it.
    class NativeMathCoordinator final
    {
      public:
        static MathCoordinatorHandle Create(NativeMathDeliverySink const &sink);
        void BeginSource(uint64_t attempt) noexcept;
        NativeMathState Request(notepads_markdown_core_model const *model, NativeMathKey const &key, uint32_t ordinal,
                                ID2D1Factory *actualMultiThreadedFactory, notepads_markdown_core_error *error,
                                uint32_t residentLimit = NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT) noexcept;
        void CancelAll() noexcept;
        std::array<NativeMathState, NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT>
        AttemptStates(uint64_t attempt) const noexcept;
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        NativeMathState AttemptState(uint64_t attempt, uint32_t ordinal) const noexcept;
#endif
        int32_t Usage(notepads_markdown_math_usage *usage, notepads_markdown_core_error *error) noexcept;
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        uint32_t PendingCountForDiagnostics() const noexcept;
#endif
        // Borrowed while this coordinator handle lives. Diagram clones the
        // same Derived authority; no independent per-engine quota is created.
        notepads_markdown_math_control *SharedControl() const noexcept
        {
            return _control;
        }
        void AddRef() noexcept;
        void Release() noexcept;
        // Shared Derived capacity entry: call once per actual policy-capacity
        // return, outside its own locks. Wakes both diagram and math queues;
        // the <=100ms timers remain the fallback.
        static void NotifyResourcesAvailable() noexcept;

      private:
        friend class NativeMathWorker;
        friend class NativeMathRuntime;
        NativeMathCoordinator(notepads_markdown_math_control *control, NativeMathDeliverySink const &sink) noexcept;
        ~NativeMathCoordinator() = default;
        static void Completed(void *context, notepads_markdown_math_job *job, NativeMathWorkerResult const &result) noexcept;
        static bool Admit(void *context, NativeMathWorker::PreparedNode *node) noexcept;
        void Finished(NativeMathWorker::PreparedNode *node, notepads_markdown_math_job *job,
                      NativeMathWorkerResult const &result) noexcept;
        void Wake() noexcept;
        void Remove(NativeMathWorker::PreparedNode *node, NativeMathState state) noexcept;
        static void SettleDelivery(void *owner, uint64_t attempt, uint32_t ordinal, bool accepted) noexcept;
        std::atomic<uint32_t> _references{1};
        ModuleLifetime _module;
        notepads_markdown_math_control *_control{};
        mutable SRWLOCK _lock = SRWLOCK_INIT;
        NativeMathWorker::PreparedNode *_head{};
        NativeMathCoordinator *_runtimeNext{};
        NativeMathDeliverySink _sink;
        uint64_t _attempt{};
        uint8_t _attempts[NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT]{};
    };
    static_assert(sizeof(NativeMathCoordinator) <= NOTEPADS_MARKDOWN_MATH_NATIVE_POLICY_HEADER_BYTES);
    static_assert(NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT * sizeof(void *) <= 1024);
}
