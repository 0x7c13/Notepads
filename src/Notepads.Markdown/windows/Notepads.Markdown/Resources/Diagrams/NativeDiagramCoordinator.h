// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include <array>
#include "Resources/Diagrams/NativeDiagramWorker.h"
#include "Resources/Math/NativeMathCoordinator.h"
#include "Core/NativeFailure.h"

namespace Notepads::Markdown::Native
{
    struct DiagramMemoryFailure : NativeFailure
    {
        notepads_markdown_core_error diagnostic;
        DiagramMemoryFailure(int32_t status, notepads_markdown_core_error const &error) noexcept
            : NativeFailure(status), diagnostic(error)
        {
        }
    };
    class NativeDiagramCoordinator;
    class DiagramCoordinatorHandle final
    {
      public:
        DiagramCoordinatorHandle() noexcept = default;
        DiagramCoordinatorHandle(DiagramCoordinatorHandle const &other) noexcept;
        DiagramCoordinatorHandle(DiagramCoordinatorHandle &&other) noexcept;
        DiagramCoordinatorHandle &operator=(DiagramCoordinatorHandle const &other) noexcept;
        DiagramCoordinatorHandle &operator=(DiagramCoordinatorHandle &&other) noexcept;
        ~DiagramCoordinatorHandle();
        NativeDiagramCoordinator *operator->() const noexcept;
        explicit operator bool() const noexcept;
        void Reset() noexcept;

      private:
        friend class NativeDiagramCoordinator;
        explicit DiagramCoordinatorHandle(NativeDiagramCoordinator *value) noexcept;
        NativeDiagramCoordinator *_value{};
    };

    class NativeDiagramCoordinator final
    {
      public:
        static DiagramCoordinatorHandle Create(MathCoordinatorHandle const &sharedControl, notepads_markdown_core_model const *model,
                                               NativeDiagramDeliverySink const &sink);
        void BeginSource(uint64_t attempt) noexcept;
        NativeDiagramState Request(notepads_markdown_core_model const *model, NativeDiagramKey const &key, uint32_t ordinal,
                                   notepads_markdown_diagram_font_capability const &fonts, notepads_markdown_diagram_style const &style,
                                   notepads_markdown_core_error *error, uint32_t residentLimit = NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT) noexcept;
        void CancelAll() noexcept;
        std::array<NativeDiagramState, NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT>
        AttemptStates(uint64_t attempt) const noexcept;
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        NativeDiagramState AttemptState(uint64_t attempt, uint32_t ordinal) const noexcept;
#endif
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        uint32_t PendingCountForDiagnostics() const noexcept;
#endif
        void AddRef() noexcept;
        void Release() noexcept;
        static void NotifyResourcesAvailable() noexcept;

      private:
        NativeDiagramCoordinator(MathCoordinatorHandle const &sharedControl, notepads_markdown_diagram_coordinator_funding *funding,
                                 NativeDiagramDeliverySink const &sink) noexcept;
        ~NativeDiagramCoordinator() = default;
        static void Completed(void *context, notepads_markdown_diagram_job *job, NativeDiagramWorkerResult const &result) noexcept;
        static bool Admit(void *context, NativeDiagramWorker::PreparedNode *node) noexcept;
        void Finished(NativeDiagramWorker::PreparedNode *node, NativeDiagramWorkerResult const &result) noexcept;
        void Remove(NativeDiagramWorker::PreparedNode *node, NativeDiagramState state) noexcept;
        static void SettleDelivery(void *owner, uint64_t attempt, uint32_t ordinal, bool accepted) noexcept;
        void Wake() noexcept;
        std::atomic<uint32_t> _references{1};
        MathCoordinatorHandle _shared;
        notepads_markdown_diagram_coordinator_funding *_funding{};
        mutable SRWLOCK _lock = SRWLOCK_INIT;
        NativeDiagramWorker::PreparedNode *_head{};
        NativeDiagramCoordinator *_runtimeNext{};
        NativeDiagramDeliverySink _sink;
        uint64_t _attempt{};
        uint8_t _attempts[NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT]{};
        NativeDiagramKey _policyKey;
        bool _hasPolicyKey{};
        bool _quarantined{};
    };
    static_assert(sizeof(NativeDiagramCoordinator) <= NOTEPADS_MARKDOWN_DIAGRAM_NATIVE_COORDINATOR_BYTES);
    static_assert(NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT * sizeof(void *) <= 1024);
}
