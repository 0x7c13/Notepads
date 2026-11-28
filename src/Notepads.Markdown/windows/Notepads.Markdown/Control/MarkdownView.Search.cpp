// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Control/MarkdownView.h"
#include "Control/MarkdownFindResult.h"
#include "Reading/NativeReadingSearch.h"

namespace winrt::Notepads::Markdown::implementation
{
    namespace
    {
        namespace Native = ::Notepads::Markdown::Native;

        struct SearchContext
        {
            // Last member to retire: paid query/Main and the module's own code
            // must remain valid through every worker/delegate destruction.
            Native::ModuleLifetime module;
            // Permit retires after paid payloads; weak slots cannot admit a
            // replacement while their old query/Main are still being freed.
            std::shared_ptr<Native::ReadingSearchOperation> operation;
            std::shared_ptr<TextSnapshot const> snapshot;
            Native::ReadingString query;
            weak_ref<MarkdownView> owner;
            Windows::UI::Core::CoreDispatcher dispatcher{nullptr};
            uint32_t start;
            uint32_t length;
            bool backward;
            bool ignoreCase;

            SearchContext(std::shared_ptr<TextSnapshot const> source,
                          std::shared_ptr<Native::ReadingSearchOperation> permit,
                          weak_ref<MarkdownView> control, Windows::UI::Core::CoreDispatcher queue,
                          uint32_t first, uint32_t count, bool reverse, bool folded)
                : operation(std::move(permit)), snapshot(std::move(source)),
                  query(Native::ReadingAllocator<wchar_t>{snapshot->main->Memory()}),
                  owner(std::move(control)), dispatcher(std::move(queue)),
                  start(first), length(count), backward(reverse), ignoreCase(folded)
            {
            }
        };

        struct SearchCompletion
        {
            // A dispatcher can retain its delegate after a caller stops waiting.
            Native::ModuleLifetime module;
            Markdown::MarkdownFindResult value{nullptr};
            std::atomic<int32_t> status{E_UNEXPECTED};
        };

        void SearchCheckpoint(void *value)
        {
            static_cast<Native::ReadingSearchOperation *>(value)->Check();
        }

        Windows::Foundation::IAsyncOperation<Markdown::MarkdownFindResult> FindTextCoreAsync(
            std::shared_ptr<SearchContext> argument)
        {
            // Completed IAsyncOperation objects may retain their coroutine frame.
            // No resource-owning argument may remain there after body locals die.
            auto context = std::move(argument);
            auto cancellation = co_await get_cancellation_token();
            cancellation.callback([weak = std::weak_ptr{context->operation}]
                                  {
                                      if (auto operation = weak.lock())
                                          operation->Cancel();
                                  });
            struct CallbackGuard
            {
                decltype(cancellation) token;
                ~CallbackGuard() noexcept
                {
                    token.callback(nullptr);
                }
            } guard{cancellation};

            // All valid requests, including an empty range, publish asynchronously.
            // The UI can therefore deterministically cancel/invalidate queued work.
            co_await resume_background();
            context->operation->Check();
            auto match = Native::FindReadingText(
                context->snapshot->View(), {context->query.data(), context->query.size()},
                context->start, context->length, context->backward, context->ignoreCase,
                SearchCheckpoint, context->operation.get());
            context->operation->Check();
            auto completed = std::make_shared<SearchCompletion>();
            auto dispatched = co_await resume_agile(context->dispatcher.TryRunAsync(
                Windows::UI::Core::CoreDispatcherPriority::Normal,
                [pending = context, completed, match]() mutable
                {
                    // Empty this stored capture once invoked. A completed dispatch
                    // operation retaining its delegate cannot retain the old Main.
                    auto job = std::move(pending);
                    try
                    {
                        job->operation->Check();
                        auto control = job->owner.get();
                        if (!control || !control->ReadingAvailable())
                            throw hresult_error(UIA_E_ELEMENTNOTAVAILABLE);
                        control->CheckLiveRange(*job->snapshot);
                        completed->value = make<implementation::MarkdownFindResult>(
                            job->snapshot->document, job->snapshot->revision, job->snapshot->token,
                            match.found, match.start, match.length);
                        completed->status.store(S_OK, std::memory_order_release);
                    }
                    catch (...)
                    {
                        completed->status.store(static_cast<int32_t>(to_hresult()), std::memory_order_release);
                    }
                }));
            context->operation->Check();
            if (cancellation())
                throw hresult_canceled();
            if (!dispatched)
                throw hresult_error(UIA_E_ELEMENTNOTAVAILABLE);
            check_hresult(completed->status.load(std::memory_order_acquire));
            co_return completed->value;
        }
    }

    uint64_t MarkdownView::DisplayedTextLength() const
    {
        CheckUiThread();
        if (_documentAccepted && _automationSnapshot && _automationSnapshot->main)
        {
            auto bounds = _automationSnapshot->Bounds();
            return bounds.end - bounds.start;
        }
        return 0;
    }

    Windows::Foundation::IAsyncOperation<Markdown::MarkdownFindResult> MarkdownView::FindTextAsync(
        guid document, uint64_t revision, uint64_t textGeneration, hstring const &query,
        uint64_t start, uint64_t length, bool backward, bool ignoreCase)
    {
        // This front method is deliberately not a coroutine: validate before
        // copying any caller-owned string, and never borrow it across an await.
        CheckUiThread();
        if (!ReadingAvailable())
            throw hresult_error(UIA_E_ELEMENTNOTAVAILABLE);
        auto snapshot = _automationSnapshot;
        if (snapshot->document != document || snapshot->revision != revision || snapshot->token != textGeneration)
            throw hresult_invalid_argument();
        auto bounds = snapshot->Bounds();
        if (start < bounds.start || start > bounds.end || length > bounds.end - start ||
            query.size() > Native::ReadingSearchMaximumQueryLength ||
            !Native::IsValidReadingSearchQuery({query.c_str(), query.size()}))
            throw hresult_invalid_argument();
        snapshot->main->ValidateRange(static_cast<uint32_t>(start), static_cast<uint32_t>(length));
        auto permit = _readingSearch.Acquire();
        auto context = std::make_shared<SearchContext>(snapshot, std::move(permit), get_weak(), Dispatcher(),
                                                       static_cast<uint32_t>(start), static_cast<uint32_t>(length),
                                                       backward, ignoreCase);
        context->query.assign(query.c_str(), query.size());
        // Bounded, weak diagnostic observations of the actual production context.
        // Register before starting it; failure destroys payloads before the permit.
        for (auto &observation : _readingSearchContexts)
        {
            if (observation.expired())
            {
                observation = context;
                return FindTextCoreAsync(std::move(context));
            }
        }
        throw hresult_error(E_UNEXPECTED);
    }

    uint32_t MarkdownView::SearchContextCount() const
    {
        CheckUiThread();
        uint32_t count{};
        for (auto const &observation : _readingSearchContexts)
            if (!observation.expired())
                ++count;
        return count;
    }

    uint32_t MarkdownView::SearchOperationCount() const
    {
        CheckUiThread();
        return _readingSearch.ActiveCount();
    }

}
