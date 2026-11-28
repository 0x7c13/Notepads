// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Rendering/NativeDocumentRenderer.h"
#include <winrt/Notepads.Markdown.h>
#include <mutex>
#include <utility>

namespace winrt::Notepads::Markdown::implementation
{
    struct PreviewUpdate
    {
        ::Notepads::Markdown::Native::ModuleLifetime module;
        guid document{};
        uint64_t revision{};
        uint64_t generation{};
        uint64_t sourceGeneration{};
        // Internal reload/Refresh receipts may redraw, but cannot reopen the
        // edit channel after a lifecycle or input synchronization failure.
        bool fullSynchronization{};
        bool incrementalEdit{};
        bool waitForEmbeddedViewport{};
        bool surfaceRecovery{};
        winrt::handle completed{::CreateEventExW(nullptr, nullptr, CREATE_EVENT_MANUAL_RESET, EVENT_ALL_ACCESS)};
        // Pending -> publishing or completing -> done. Publication and Cancel compete once.
        std::atomic<uint32_t> state{};
        std::atomic<uint64_t> layoutGeneration{};
        Markdown::DocumentUpdateStatus status{Markdown::DocumentUpdateStatus::Failed};
        int32_t error{};
        bool published{};
        uint64_t blocks{};
        std::mutex cancelMutex;
        std::shared_ptr<::Notepads::Markdown::Native::PreviewCancellation> layoutCancel;
        std::shared_ptr<::Notepads::Markdown::Native::PreviewCancellation> sourceCancel;
        PreviewUpdate()
        {
            check_bool(static_cast<bool>(completed));
        }
        bool Pending() const noexcept
        {
            return state.load(std::memory_order_acquire) == 0;
        }
        bool AllowsViewportFrame() const noexcept
        {
            auto current = state.load(std::memory_order_acquire);
            // status is published by the release store of the terminal state.
            return current == 0 || current == 1 ||
                   (current == 3 && status == Markdown::DocumentUpdateStatus::Ready);
        }
        bool Complete(Markdown::DocumentUpdateStatus result, int32_t code = NOTEPADS_MARKDOWN_CORE_OK) noexcept
        {
            uint32_t expected{};
            if (!state.compare_exchange_strong(expected, 2, std::memory_order_acq_rel))
                return false;
            status = result;
            error = code;
            state.store(3, std::memory_order_release);
            ::SetEvent(completed.get());
            return true;
        }
        void CompleteViewportWait() noexcept
        {
            if (!waitForEmbeddedViewport)
                Complete(Markdown::DocumentUpdateStatus::NotLoaded);
        }
        bool Claim() noexcept
        {
            uint32_t expected{};
            return state.compare_exchange_strong(expected, 1, std::memory_order_acq_rel);
        }
        void Published(bool success, uint64_t count) noexcept
        {
            status = success ? Markdown::DocumentUpdateStatus::Ready : Markdown::DocumentUpdateStatus::Failed;
            error = success ? NOTEPADS_MARKDOWN_CORE_OK : NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT;
            published = success;
            blocks = success ? count : 0;
            state.store(3, std::memory_order_release);
            ::SetEvent(completed.get());
        }
        void Cancel() noexcept
        {
            if (!Complete(Markdown::DocumentUpdateStatus::Canceled, NOTEPADS_MARKDOWN_CORE_CANCELLED))
                return;
            ::Notepads::Markdown::Native::CoreCompletionTicket parse;
            std::shared_ptr<::Notepads::Markdown::Native::PreviewCancellation> flag, source;
            {
                std::lock_guard guard(cancelMutex);
                parse = _parse;
                flag = layoutCancel;
                source = sourceCancel;
            }
            parse.Cancel();
            if (flag)
                flag->Cancel();
            if (source)
                source->Cancel();
        }
        void BindParse(::Notepads::Markdown::Native::CoreCompletionTicket ticket)
        {
            ::Notepads::Markdown::Native::CoreCompletionTicket previous;
            {
                std::lock_guard guard(cancelMutex);
                previous = std::exchange(_parse, ticket);
            }
            // A deferred source can bind after its public operation was canceled.
            // Cancel the exact new request without holding the publication lock.
            if (!Pending())
                ticket.Cancel();
        }
        void BindSourceCancel(std::shared_ptr<::Notepads::Markdown::Native::PreviewCancellation> flag)
        {
            {
                std::lock_guard guard(cancelMutex);
                sourceCancel = flag;
            }
            if (!Pending())
                flag->Cancel();
        }
        void BindCancel(std::shared_ptr<::Notepads::Markdown::Native::PreviewCancellation> flag)
        {
            {
                std::lock_guard guard(cancelMutex);
                layoutCancel = flag;
            }
            if (!Pending())
                flag->Cancel();
        }

      private:
        ::Notepads::Markdown::Native::CoreCompletionTicket _parse; // cancelMutex
    };
}
