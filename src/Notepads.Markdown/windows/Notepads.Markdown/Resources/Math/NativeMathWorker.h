// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Resources/Math/NativeMathPresentation.h"
#include "notepads/markdown/math.h"
#include "Core/ModuleLifetime.h"
#include <windows.h>

namespace Notepads::Markdown::Native
{
    // The module has one destructible process authority. It deliberately does not
    // pin ModuleLifetime: controls, running nodes and retained payloads do that.
    class NativeMathRuntime final
    {
      public:
        static NativeMathRuntime &Shared() noexcept;
        int32_t CreateControl(notepads_markdown_math_control **output, notepads_markdown_core_error *error) noexcept;
        int32_t Usage(notepads_markdown_math_usage *output, notepads_markdown_core_error *error) noexcept;
        NativeMathRuntime(NativeMathRuntime const &) = delete;
        NativeMathRuntime &operator=(NativeMathRuntime const &) = delete;

      private:
        friend class NativeMathCoordinator;
        NativeMathRuntime() noexcept = default;
        ~NativeMathRuntime();
        SRWLOCK _lock = SRWLOCK_INIT;
        notepads_markdown_math_runtime *_runtime{};
        class NativeMathCoordinator *_waiters{};
        uint64_t _wakeEpoch{};
        std::atomic_flag _notifying = ATOMIC_FLAG_INIT;
    };

    class NativeMathControl final
    {
      public:
        NativeMathControl() noexcept = default;
        ~NativeMathControl();
        NativeMathControl(NativeMathControl const &) = delete;
        NativeMathControl &operator=(NativeMathControl const &) = delete;
        int32_t Initialize(notepads_markdown_core_error *error) noexcept;
        int32_t Usage(notepads_markdown_math_usage *output, notepads_markdown_core_error *error) noexcept;
        notepads_markdown_math_control *Get() const noexcept
        {
            return _control;
        }

      private:
        ModuleLifetime _module;
        notepads_markdown_math_control *_control{};
    };

    static_assert(sizeof(NativeMathRuntime) <= NOTEPADS_MARKDOWN_MATH_NATIVE_POLICY_HEADER_BYTES);
    static_assert(sizeof(NativeMathControl) <= NOTEPADS_MARKDOWN_MATH_NATIVE_POLICY_HEADER_BYTES);

    struct NativeMathStackObservation
    {
        uint64_t lowerLimit{};
        uint64_t upperLimit{};
        uint64_t allocationBase{};
        uint64_t allocationBytes{};
        uint64_t committedBytes{}; // Includes guard pages, unlike headroom.
        uint64_t guardBytes{};
        uint64_t marker{};
        uint64_t usableLower{};
        uint64_t headroom{}; // Contiguous committed RW, less entry slack.
        uint64_t entrySlack{};
        DWORD threadId{};
        DWORD queryError{};
        bool measured{};
        bool withinStackBudget{};
        bool threadSignaled{};
    };

    struct NativeMathWorkerResult
    {
        int32_t status{NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
        notepads_markdown_core_error error{NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(notepads_markdown_core_error)};
        NativeMathStackObservation stack;
        DWORD tailWaitResult{WAIT_OBJECT_0};
    };

    struct NativeMathStartResult
    {
        int32_t status{NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
        DWORD win32Error{};
    };

    // Private fixed test seam, never an arbitrary worker observer/callback.
    // The two manual-reset events must outlive real worker retirement. A gate
    // blocks only the post-run tail and has a bounded failure rescue timeout.
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
    struct NativeMathWorkerTestHooks
    {
        HANDLE tailReached{};
        HANDLE tailPermit{};
        DWORD simulatedCreateThreadError{};
        // A diagnostic may only reduce the measured guarantee; it cannot
        // substitute a profile or increase the real committed headroom.
        bool forceInsufficientHeadroom{};
    };
#endif

    class NativeMathWorker final
    {
      public:
        struct PreparedNode;
        using Admission = bool (*)(void *owner, PreparedNode *node) noexcept;
        // Called on a background reaper only after the actual thread is signaled.
        // job is borrowed and valid throughout; retain it to keep a payload.
        // userdata belongs to the private coordinator until this call returns.
        // Native finalization must count the prepaid fixed 4096-byte node while
        // it is live. Completion must not call application/UI code directly.
        using Completion = void (*)(void *userdata, notepads_markdown_math_job *job, NativeMathWorkerResult const &result) noexcept;
        // Caller owns an activated job and serializes Start for that job. Start
        // consumes a one-shot claim before any allocation and takes another reference.
        // Failure leaves the caller's grant unchanged, but this job cannot restart;
        // release it and prepare a fresh funded job.
        static NativeMathStartResult Start(notepads_markdown_math_job *job, Completion completion, void *userdata) noexcept;
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        static NativeMathStartResult StartForTesting(notepads_markdown_math_job *job, Completion completion, void *userdata,
                                                     NativeMathWorkerTestHooks const &hooks) noexcept;
#endif
        // Prepare owns exactly the original FFI context reference; native node
        // references do not retain that context. The complete 4096 bytes are
        // paid by its queued MetadataGrant until real node/context destruction.
        static int32_t Prepare(notepads_markdown_math_control *control, notepads_markdown_core_model const *model, NativeMathKey const &key,
                               uint32_t ordinal, ID2D1Factory *factory, void *owner, Completion completion,
                               PreparedNode **output, notepads_markdown_core_error *error, Admission admission = nullptr) noexcept;
        static NativeMathStartResult StartPrepared(PreparedNode *node) noexcept;
        static void RetainPrepared(PreparedNode *node) noexcept;
        static void ReleasePrepared(PreparedNode *node) noexcept;
        static void CancelPrepared(PreparedNode *node) noexcept;
        static void WakePrepared(PreparedNode *node) noexcept;
        // Only for a prepared node never submitted to StartPrepared.
        static void DestroyQueued(PreparedNode *node) noexcept;
        static PreparedNode *NextPrepared(PreparedNode *node) noexcept;
        static bool IsQueued(PreparedNode *node) noexcept;
        static void SetNextPrepared(PreparedNode *node, PreparedNode *next) noexcept;
        static NativeMathKey const &PreparedKey(PreparedNode *node) noexcept;
        static uint32_t PreparedOrdinal(PreparedNode *node) noexcept;
        static ID2D1Factory *PreparedFactory(PreparedNode *node) noexcept;
        static void *PreparedOwner(PreparedNode *node) noexcept;
        static void ClearPreparedOwner(PreparedNode *node) noexcept;
        static MathPresentationHandle FailurePresentation(PreparedNode *node, int32_t status) noexcept;
        static HRESULT PostFailure(PreparedNode *node, NativeMathDeliverySink const &sink,
                                   MathPresentationHandle payload) noexcept;
        // Observation only; no cancellation/join/finalization authority.
        static uint32_t LiveNodes() noexcept;
        static uint32_t ReaperWaitFailures() noexcept;
        static uint32_t CallbackPinFailures() noexcept;
        static uint32_t NodeFreeFailures() noexcept;
    };
}
