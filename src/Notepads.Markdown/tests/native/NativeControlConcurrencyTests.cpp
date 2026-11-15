// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "NativeControlConcurrencyTests.h"
#include "Control/PreviewUpdate.h"
#include "Control/MarkdownTheme.h"
#include <array>
#include <atomic>
#include <objidl.h>
#include <chrono>
#include <thread>

namespace Notepads::Markdown::Native::ControlConcurrencyTests
{
    namespace
    {
        void Require(bool value)
        {
            if (!value)
                throw winrt::hresult_error(E_FAIL, L"Control concurrency regression failed.");
        }

        void CheckThemeConcurrency()
        {
            using Color = winrt::Windows::UI::Color;
            auto theme = winrt::make<winrt::Notepads::Markdown::implementation::MarkdownTheme>();
            auto colors = [&]
            {
                return std::array{theme.Foreground(), theme.Background(), theme.LinkForeground(),
                                  theme.SelectionForeground(), theme.SelectionBackground(), theme.BorderColor(),
                                  theme.PanelBackground(), theme.FocusColor(), theme.CodeCommentForeground(),
                                  theme.CodeStringForeground(), theme.CodeNumberForeground(),
                                  theme.CodeKeywordForeground(), theme.CodeTypeForeground(), theme.CodeFunctionForeground()};
            };
            auto sameColor = [](Color const &left, Color const &right) noexcept
            {
                return left.A == right.A && left.R == right.R && left.G == right.G && left.B == right.B;
            };
            constexpr std::array<Color, 14> expectedDefaults{{
                {255, 30, 30, 30}, {255, 255, 255, 255}, {255, 0, 90, 158},
                {255, 255, 255, 255}, {255, 0, 90, 158}, {255, 205, 211, 220},
                {255, 244, 245, 247}, {255, 0, 90, 158}, {255, 90, 112, 81},
                {255, 163, 21, 21}, {255, 9, 134, 88}, {255, 0, 0, 180},
                {255, 38, 127, 153}, {255, 121, 94, 38}}};
            auto defaults = colors();
            Require(theme.Generation() == 1 && theme.FontFamily() == L"Segoe UI" &&
                    theme.CodeFontFamily() == L"Consolas" && theme.FontSize() == 18 &&
                    theme.LineHeight() == 27 && theme.Padding() == 16);
            for (size_t index = 0; index < defaults.size(); ++index)
                Require(sameColor(defaults[index], expectedDefaults[index]));
            Require(static_cast<bool>(theme.try_as<::IAgileObject>()));

            constexpr uint32_t iterations = 20000;
            winrt::hstring first{std::wstring(1024, L'a')}, second{std::wstring(1024, L'b')};
            winrt::handle start{CreateEventW(nullptr, TRUE, FALSE, nullptr)};
            winrt::check_bool(static_cast<bool>(start));
            std::atomic<bool> valid{true};
            auto run = [&](auto action)
            {
                bool initialized{};
                try
                {
                    winrt::init_apartment(winrt::apartment_type::multi_threaded);
                    initialized = true;
                    if (WaitForSingleObjectEx(start.get(), 10000, FALSE) != WAIT_OBJECT_0)
                        throw winrt::hresult_error(E_FAIL);
                    action();
                }
                catch (...)
                {
                    valid.store(false, std::memory_order_relaxed);
                }
                if (initialized)
                    winrt::uninit_apartment();
            };
            auto write = [&](uint32_t ordinal)
            {
                run([&]
                {
                    for (uint32_t iteration = 0; iteration < iterations; ++iteration)
                    {
                        theme.Generation(iteration + 1);
                        theme.FontFamily(ordinal ? first : second);
                        theme.CodeFontFamily(ordinal ? second : first);
                        theme.FontSize(18 + ordinal);
                        theme.LineHeight(27 + ordinal);
                        theme.Padding(16 + ordinal);
                        Color color{255, static_cast<uint8_t>(ordinal), 1, 2};
                        theme.Foreground(color);
                        theme.Background(color);
                        theme.LinkForeground(color);
                        theme.SelectionForeground(color);
                        theme.SelectionBackground(color);
                        theme.BorderColor(color);
                        theme.PanelBackground(color);
                        theme.FocusColor(color);
                        theme.CodeCommentForeground(color);
                        theme.CodeStringForeground(color);
                        theme.CodeNumberForeground(color);
                        theme.CodeKeywordForeground(color);
                        theme.CodeTypeForeground(color);
                        theme.CodeFunctionForeground(color);
                    }
                });
            };
            auto read = [&]
            {
                run([&]
                {
                    for (uint32_t iteration = 0; iteration < iterations; ++iteration)
                    {
                        auto font = theme.FontFamily(), code = theme.CodeFontFamily();
                        auto generation = theme.Generation();
                        auto fontSize = theme.FontSize(), lineHeight = theme.LineHeight(), padding = theme.Padding();
                        if ((font != first && font != second && font != L"Segoe UI") ||
                            (code != first && code != second && code != L"Consolas") ||
                            generation == 0 || generation > iterations ||
                            (fontSize != 18 && fontSize != 19) || (lineHeight != 27 && lineHeight != 28) ||
                            (padding != 16 && padding != 17))
                            valid.store(false, std::memory_order_relaxed);
                        auto observed = colors();
                        for (size_t index = 0; index < observed.size(); ++index)
                        {
                            auto const &color = observed[index];
                            bool written = color.A == 255 && color.R <= 1 && color.G == 1 && color.B == 2;
                            if (!written && !sameColor(color, expectedDefaults[index]))
                                valid.store(false, std::memory_order_relaxed);
                        }
                    }
                });
            };
            // Join every real thread even if a later thread allocation or SetEvent fails.
            // ReleaseStart is declared after workers, so unwinding opens the gate first.
            std::array<std::jthread, 4> workers;
            struct ReleaseStart
            {
                HANDLE event;
                ~ReleaseStart() { SetEvent(event); }
            } releaseStart{start.get()};
            workers[0] = std::jthread(write, 0u);
            workers[1] = std::jthread(write, 1u);
            workers[2] = std::jthread(read);
            workers[3] = std::jthread(read);
            winrt::check_bool(SetEvent(start.get()));
            for (auto &worker : workers)
                worker.join();
            Require(valid.load(std::memory_order_relaxed));
        }

        template <typename Predicate> bool Until(Predicate predicate)
        {
            auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(5);
            while (!predicate())
            {
                if (std::chrono::steady_clock::now() >= deadline)
                    return false;
                std::this_thread::sleep_for(std::chrono::milliseconds(1));
            }
            return true;
        }

        winrt::Windows::Foundation::IAsyncAction CheckParseBinding(
            winrt::Windows::UI::Core::CoreDispatcher dispatcher,
            std::shared_ptr<CoreValidation::Context> context, bool cancelFirst,
            std::exception_ptr &failure)
        {
            auto hooks = std::make_shared<CoreControllerTestHooks>();
            hooks->Arm(CoreTestPoint::DriverBeforeRun, 10000);
            auto controller = CoreController::CreatePreview(dispatcher, hooks);
            auto lifetime = controller->Lifetime();
            auto receipt = std::make_shared<winrt::Notepads::Markdown::implementation::PreviewUpdate>();
            CoreCompletionTicket ticket, successor;
            bool registered{};
            try
            {
                context->PublishController(controller.get());
                registered = true;
                notepads_markdown_core_snapshot descriptor{};
                descriptor.header = {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, static_cast<uint32_t>(sizeof(descriptor))};
                descriptor.source.document[0] = 0xD3;
                descriptor.source.document[1] = cancelFirst ? 1 : 2;
                descriptor.source.revision = 1;
                descriptor.profile = NOTEPADS_MARKDOWN_CORE_G3;
                constexpr uint8_t input[]{'#', ' ', 'A'};
                auto submitted = controller->SubmitUtf8(input, descriptor, &ticket);
                Require(submitted.status == NOTEPADS_MARKDOWN_CORE_OK && static_cast<bool>(ticket));
                co_await winrt::resume_background();
                Require(hooks->WaitReached(CoreTestPoint::DriverBeforeRun, 5000));
                // Cancellation is on MTA; binding remains on the real UI thread.
                if (cancelFirst)
                    receipt->Cancel();
                co_await winrt::resume_foreground(dispatcher);
                receipt->BindParse(ticket);
                co_await winrt::resume_background();
                if (!cancelFirst)
                    receipt->Cancel();
                hooks->Release(CoreTestPoint::DriverBeforeRun);
                Require(Until([&] { context->Arbitrate(); return ticket.Done(); }));
                Require(ticket.Status() == NOTEPADS_MARKDOWN_CORE_CANCELLED &&
                        controller->Snapshot().lastInstalled == 0 &&
                        receipt->status == winrt::Notepads::Markdown::DocumentUpdateStatus::Canceled);

                co_await winrt::resume_foreground(dispatcher);
                descriptor.source.revision = 2;
                submitted = controller->SubmitUtf8(input, descriptor, &successor);
                Require(submitted.status == NOTEPADS_MARKDOWN_CORE_OK);
                co_await winrt::resume_background();
                Require(Until([&] { context->Arbitrate(); return successor.Done(); }));
                Require(successor.Status() == NOTEPADS_MARKDOWN_CORE_OK &&
                        controller->Snapshot().lastInstalled == successor.Serial());
            }
            catch (...)
            {
                failure = std::current_exception();
            }
            hooks->ReleaseAll();
            if (registered)
                context->WithdrawController(controller.get());
            controller->Stop();
            co_await winrt::resume_background();
            auto drain = controller->Drain();
            controller.reset();
            receipt.reset();
            ticket = {};
            successor = {};
            if (!failure)
            {
                try
                {
                    Require(!hooks->AnyTimedOut() && drain.offUi && drain.coreDrained && drain.driverExited &&
                            drain.primaryStatus == NOTEPADS_MARKDOWN_CORE_OK &&
                            drain.cleanupStatus == NOTEPADS_MARKDOWN_CORE_OK &&
                            drain.primaryHresult == 0 && drain.cleanupHresult == 0);
                    Require(Until([&] { return lifetime.Snapshot().fullyReleased; }));
                }
                catch (...)
                {
                    failure = std::current_exception();
                }
            }
            co_await winrt::resume_foreground(dispatcher);
        }
    }

    winrt::Windows::Foundation::IAsyncAction RunAsync(
        winrt::Windows::UI::Core::CoreDispatcher dispatcher,
        std::shared_ptr<CoreValidation::Context> context, std::exception_ptr &failure)
    {
        for (bool cancelFirst : {true, false})
        {
            co_await CheckParseBinding(dispatcher, context, cancelFirst, failure);
            if (failure)
                co_return;
        }
        try
        {
            co_await winrt::resume_background();
            CheckThemeConcurrency();
        }
        catch (...)
        {
            failure = std::current_exception();
        }
        co_await winrt::resume_foreground(dispatcher);
    }
}
