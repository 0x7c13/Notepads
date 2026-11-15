// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "notepads/markdown/core.h"
#include "Core/ModuleLifetime.h"
#include <array>
#include <algorithm>
#include <exception>
#include <string>
#include <vector>

namespace Notepads::Markdown::Native { class CoreController; }

namespace Notepads::Markdown::Native::CoreValidation
{
    struct CheckResult
    {
        std::wstring name;
        bool passed;
        int32_t status;
        std::wstring detail;
    };

    struct SuiteResult
    {
        uint32_t abiVersion;
        std::vector<CheckResult> checks;
    };

    enum class FaultKind
    {
        Assertion,
        UnexpectedStatus,
        UnexpectedException,
        CleanupStatus,
        CleanupInvariant
    };

    struct FaultRecord
    {
        bool present{};
        FaultKind kind{};
        int32_t status{};
        wchar_t const *operation{};
    };

    // One request owns the queued/running responsibility before the first await.
    // Independent query/control flags and the registered controller scope have distinct
    // cancellation ownership; controller updates always go through its publication gate.
    class Context
    {
    public:
        Context() noexcept;
        ~Context();
        Context(Context const &) = delete;
        Context &operator=(Context const &) = delete;
        void RequestCancel() noexcept;
        void PublishController(CoreController* controller) noexcept;
        void WithdrawController(CoreController* controller) noexcept;
        bool CancellationRequested() const noexcept;
        bool IsPublishedOrigin(notepads_markdown_core_cancel const *token) const noexcept;
        void RecordPrimary(FaultKind kind, int32_t status, wchar_t const *operation) noexcept;
        void RecordException(std::exception_ptr exception, wchar_t const *operation) noexcept;
        void Arbitrate() const;
        void Publish(notepads_markdown_core_cancel *token);
        void Withdraw(notepads_markdown_core_cancel *token) noexcept;
        void RecordCleanup(int32_t status, wchar_t const *operation) noexcept;
        void RecordCleanupInvariant(wchar_t const *operation) noexcept;
        FaultRecord PrimaryFailure() const noexcept;
        FaultRecord Failure() const noexcept;

    private:
        friend struct ContextTestAccess;

        void RecordCleanupLocked(FaultKind kind, int32_t status, wchar_t const *operation) noexcept;

        template <auto Request> void RequestCancelUsing() noexcept
        {
            _requestCancelled.store(true, std::memory_order_release);
            std::lock_guard guard(_mutex);
            for (auto token : _tokens)
            {
                if (token != nullptr)
                {
                    auto status = Request(token);
                    if (status != NOTEPADS_MARKDOWN_CORE_OK)
                    {
                        RecordCleanupLocked(FaultKind::CleanupStatus, status, L"concurrent cancel_request");
                    }
                }
            }
        }

        template <auto Request> void PublishUsing(notepads_markdown_core_cancel *token)
        {
            std::lock_guard guard(_mutex);
            auto slot = std::find(_tokens.begin(), _tokens.end(), nullptr);
            if (slot == _tokens.end())
            {
                throw winrt::hresult_error(E_UNEXPECTED, L"Fixed suite cancellation slots exhausted.");
            }
            *slot = token;
            if (_requestCancelled.load(std::memory_order_acquire))
            {
                auto status = Request(token);
                if (status != NOTEPADS_MARKDOWN_CORE_OK)
                {
                    RecordCleanupLocked(FaultKind::CleanupStatus, status, L"publish canceled token");
                }
            }
        }

        ModuleLifetime _module;
        std::atomic<bool> _requestCancelled{};
        mutable std::mutex _mutex;
        std::array<notepads_markdown_core_cancel *, 8> _tokens{};
        std::mutex _controllerMutex;
        CoreController* _controller{};
        FaultRecord _primary;
        FaultRecord _cleanup;
    };

    uint32_t LiveWorkers() noexcept;
    uint32_t LiveHandles() noexcept;
    SuiteResult Run(Context &context);
}
