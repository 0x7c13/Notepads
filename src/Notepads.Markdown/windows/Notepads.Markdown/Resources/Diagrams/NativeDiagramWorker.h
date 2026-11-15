// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Resources/Diagrams/NativeDiagramArtifact.h"

namespace Notepads::Markdown::Native
{
    struct NativeDiagramStackObservation
    {
        uint64_t lowerLimit{}, upperLimit{}, allocationBase{}, allocationBytes{};
        uint64_t committedBytes{}, guardBytes{}, marker{}, usableLower{}, headroom{}, entrySlack{};
        DWORD threadId{}, queryError{};
        bool measured{}, withinStackBudget{}, threadSignaled{};
    };
    struct NativeDiagramWorkerResult
    {
        int32_t status{NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
        notepads_markdown_core_error error{NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(notepads_markdown_core_error)};
        NativeDiagramStackObservation stack;
        HRESULT apartment{E_UNEXPECTED};
        DWORD tailWaitResult{WAIT_OBJECT_0};
    };
    struct NativeDiagramStartResult
    {
        int32_t status{NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
        DWORD win32Error{};
    };
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
    struct NativeDiagramWorkerTestHooks
    {
        HANDLE tailReached{}, tailPermit{};
        DWORD simulatedCreateThreadError{};
        bool forceInsufficientHeadroom{};
    };
#endif

    class NativeDiagramWorker final
    {
      public:
        struct PreparedNode;
        using Admission = bool (*)(void *, PreparedNode *) noexcept;
        using Completion = void (*)(void *, notepads_markdown_diagram_job *, NativeDiagramWorkerResult const &) noexcept;
        static int32_t Prepare(notepads_markdown_math_control *control, notepads_markdown_core_model const *model, NativeDiagramKey const &key,
                               uint32_t ordinal, notepads_markdown_diagram_font_capability const &fonts, notepads_markdown_diagram_style const &style,
                               void *owner, Completion completion, PreparedNode **output, notepads_markdown_core_error *error,
                               Admission admission = nullptr) noexcept;
        static NativeDiagramStartResult StartPrepared(PreparedNode *node) noexcept;
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        static NativeDiagramStartResult StartPreparedForTesting(PreparedNode *node,
                                                                NativeDiagramWorkerTestHooks const &hooks) noexcept;
#endif
        static void RetainPrepared(PreparedNode *node) noexcept;
        static void ReleasePrepared(PreparedNode *node) noexcept;
        static void CancelPrepared(PreparedNode *node) noexcept;
        static void WakePrepared(PreparedNode *node) noexcept;
        static void DestroyQueued(PreparedNode *node) noexcept;
        static PreparedNode *NextPrepared(PreparedNode *node) noexcept;
        static void SetNextPrepared(PreparedNode *node, PreparedNode *next) noexcept;
        static bool IsQueued(PreparedNode *node) noexcept;
        static NativeDiagramKey const &PreparedKey(PreparedNode *node) noexcept;
        static uint32_t PreparedOrdinal(PreparedNode *node) noexcept;
        static void *PreparedOwner(PreparedNode *node) noexcept;
        static void ClearPreparedOwner(PreparedNode *node) noexcept;
        static DiagramPresentationHandle BuildPresentationAfterThreadExit(PreparedNode *node, int32_t &status) noexcept;
        static HRESULT Post(PreparedNode *node, NativeDiagramDeliverySink const &sink,
                            DiagramPresentationHandle payload, DerivedDeliveryReceipt receipt = {}) noexcept;
        static void *AcquireStreamStorage(PreparedNode *node, size_t size, size_t alignment, uint32_t *slot) noexcept;
        static void ReleaseStreamStorage(PreparedNode *node, uint32_t slot) noexcept;
        static uint32_t LiveNodes() noexcept;
        static uint32_t ReaperWaitFailures() noexcept;
        static uint32_t CallbackPinFailures() noexcept;
        static uint32_t NodeFreeFailures() noexcept;
        static uint32_t DeferredRetirements() noexcept;
    };
}
