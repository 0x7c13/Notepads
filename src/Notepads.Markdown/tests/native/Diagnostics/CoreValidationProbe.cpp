// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Diagnostics/CoreValidationProbe.h"
#include "Diagnostics/NativeStartupTrace.h"
#include "Control/MarkdownView.h"
#include "CoreValidationProbe.g.cpp"
#include "Diagnostics/CoreValidationCheck.h"
#include "Diagnostics/CoreValidationReport.h"
#include "Diagnostics/CoreValidationSuite.h"
#include "CoreControllerTests.h"
#include "NativeReadingSnapshotTests.h"
#include "NativeImageResourcesTests.h"
#include "NativeImagePayloadTests.h"
#include "NativeImageViewportTests.h"
#include "NativeHighlightSnapshotTests.h"
#include "Control/MarkdownImageRequest.h"
#include <notepads/markdown/images.h>
#include <winrt/Windows.Storage.Streams.h>

namespace winrt::Notepads::Markdown::implementation
{
    namespace
    {
        template <typename F> struct OnExit
        {
            F action;
            ~OnExit() noexcept
            {
                action();
            }
        };
    }

    Windows::Foundation::IAsyncOperation<Markdown::CoreValidationReport> CoreValidationProbe::RunAsync()
    {
        ::Notepads::Markdown::Native::ModuleLifetime module;
        // Construction registers the worker before it can enter the thread pool.
        auto context = std::make_shared<::Notepads::Markdown::Native::CoreValidation::Context>();
        auto cancellation = co_await get_cancellation_token();
        cancellation.callback(
            [weak = std::weak_ptr(context)]
            {
                if (auto request = weak.lock())
                {
                    request->RequestCancel();
                }
            });
        OnExit removeCallback{[cancellation]() noexcept { cancellation.callback(nullptr); }};
        auto window = Windows::UI::Core::CoreWindow::GetForCurrentThread();
        if (!window)
        {
            throw hresult_wrong_thread();
        }
        auto dispatcher = window.Dispatcher();
        auto controllerResults = std::make_shared<::Notepads::Markdown::Native::CoreControllerTests::Results>();
        try
        {
            context->Arbitrate();
            // This actual UWP/UI path is deliberately outside the console-PC subset.
            co_await ::Notepads::Markdown::Native::CoreControllerTests::RunAsync(dispatcher, controllerResults, context);
            co_await resume_background();
            if (cancellation())
            {
                context->RequestCancel();
            }
            context->Arbitrate();
            auto result = ::Notepads::Markdown::Native::CoreValidation::Run(*context);
            for (auto &check : controllerResults->checks)
            {
                result.checks.push_back(std::move(check));
            }
            context->Arbitrate();
            std::vector<Markdown::CoreValidationCheck> checks;
            checks.reserve(result.checks.size());
            for (auto &check : result.checks)
            {
                checks.push_back(make<implementation::CoreValidationCheck>(hstring(check.name), check.passed,
                                                                           check.status, hstring(check.detail)));
            }
            auto report = make<implementation::CoreValidationReport>(result.abiVersion, std::move(checks));
            context->Arbitrate();
            co_return report;
        }
        catch (...)
        {
            // Run's local guards have unwound while Context is still available.
            context->RecordException(std::current_exception(), L"CoreValidationProbe exit");
            context->Arbitrate();
            throw;
        }
    }

    Windows::Foundation::IAsyncOperation<Markdown::CoreValidationReport> CoreValidationProbe::RunImageProviderAsync(
        Markdown::IMarkdownImageProvider provider)
    {
        ::Notepads::Markdown::Native::ModuleLifetime module;
        if (!provider || !Windows::UI::Core::CoreWindow::GetForCurrentThread())
            throw hresult_invalid_argument();
        auto cancellation = co_await get_cancellation_token();
        cancellation.enable_propagation();
        guid document{};
        document.Data1 = 0x4d535249;
        auto request = make<implementation::MarkdownImageRequest>(document, 1, 2, 7, 11, 3,
                                                                  L"notepads-markdown-test:provider-seam", L"Provider seam",
                                                                  L"2 by 2 pixels", 8 * 1024 * 1024, 16 * 1024 * 1024);
        auto stream = co_await provider.OpenAsync(request);
        if (!stream)
            throw hresult_invalid_argument();
        struct StreamOwner
        {
            Windows::Storage::Streams::IRandomAccessStream value{nullptr};
            ~StreamOwner() noexcept
            {
                if (value)
                    try
                    {
                        value.Close();
                    }
                    catch (...)
                    {
                    }
            }
        } owner{std::move(stream)};
        // The first provider seam accepts explicitly agile input streams only.
        // Provider itself remains on the UI thread; no reflected CCW is used.
        if (!owner.value.try_as<::IAgileObject>())
            throw hresult_no_interface();
        co_await resume_background();
        auto size = owner.value.Size();
        bool readable = owner.value.CanRead() && owner.value.Position() == 0 && size >= 24 && size <= 8 * 1024 * 1024;
        std::vector<Markdown::CoreValidationCheck> checks;
        checks.push_back(make<implementation::CoreValidationCheck>(
            L"image provider CCW/native stream", readable, readable ? S_OK : E_INVALIDARG,
            L"UI provider, native worker stream, fixed request identity"));
        if (readable)
        {
            using namespace Windows::Storage::Streams;
            auto buffer = co_await owner.value.ReadAsync(Buffer{64}, 64, InputStreamOptions::None);
            constexpr std::array<uint8_t, 8> signature{137, 80, 78, 71, 13, 10, 26, 10};
            bool png = buffer.Length() >= 24 && std::equal(signature.begin(), signature.end(), buffer.data());
            checks.push_back(make<implementation::CoreValidationCheck>(
                L"image provider bounded PNG read", png, png ? S_OK : E_INVALIDARG,
                L"64-byte worker read; no image render qualification"));
        }
        co_return make<implementation::CoreValidationReport>(notepads_markdown_core_abi_version(), std::move(checks), 2);
    }

    using NativeDiagnostic = Windows::Foundation::IAsyncAction (*)(
        Windows::UI::Core::CoreDispatcher const &,
        std::shared_ptr<::Notepads::Markdown::Native::CoreControllerTests::Results> const &,
        std::shared_ptr<::Notepads::Markdown::Native::CoreValidation::Context> const &);

    static Windows::Foundation::IAsyncOperation<Markdown::CoreValidationReport> RunNativeDiagnosticsAsync(
        NativeDiagnostic run, size_t expectedChecks)
    {
        ::Notepads::Markdown::Native::ModuleLifetime module;
        auto context = std::make_shared<::Notepads::Markdown::Native::CoreValidation::Context>();
        auto cancellation = co_await get_cancellation_token();
        cancellation.callback(
            [weak = std::weak_ptr(context)]
            {
                if (auto request = weak.lock())
                {
                    request->RequestCancel();
                }
            });
        OnExit removeCallback{[cancellation]() noexcept { cancellation.callback(nullptr); }};
        auto window = Windows::UI::Core::CoreWindow::GetForCurrentThread();
        if (!window)
        {
            throw hresult_wrong_thread();
        }
        auto results = std::make_shared<::Notepads::Markdown::Native::CoreControllerTests::Results>();
        try
        {
            context->Arbitrate();
            co_await run(window.Dispatcher(), results, context);
            context->Arbitrate();
            std::vector<Markdown::CoreValidationCheck> checks;
            checks.reserve(results->checks.size());
            for (auto &check : results->checks)
            {
                checks.push_back(make<implementation::CoreValidationCheck>(hstring(check.name), check.passed,
                                                                           check.status, hstring(check.detail)));
            }
            auto report =
                make<implementation::CoreValidationReport>(notepads_markdown_core_abi_version(), std::move(checks), expectedChecks);
            context->Arbitrate();
            co_return report;
        }
        catch (...)
        {
            context->RecordException(std::current_exception(), L"CoreValidationProbe native diagnostics exit");
            try
            {
                context->Arbitrate();
            }
            catch (hresult_error const &primary)
            {
                std::wstring message{primary.message()};
                for (auto const &check : results->checks)
                    if (!check.passed)
                    {
                        message.append(L"; diagnostic: ");
                        message.append(check.name);
                        message.append(L"; ");
                        // Diagnostic details are bounded; the original primary
                        // HRESULT/first fault remains the authoritative failure.
                        message.append(check.detail.substr(0, 2048));
                        break;
                    }
                throw hresult_error(primary.code(), message);
            }
            throw;
        }
    }

    Windows::Foundation::IAsyncOperation<Markdown::CoreValidationReport> CoreValidationProbe::RunPreviewAsync()
    {
        return RunNativeDiagnosticsAsync(::Notepads::Markdown::Native::CoreControllerTests::RunPreviewTicketsAsync, 7);
    }

    Windows::Foundation::IAsyncOperation<Markdown::CoreValidationReport> CoreValidationProbe::RunReadingAsync()
    {
        return RunNativeDiagnosticsAsync(::Notepads::Markdown::Native::ReadingSnapshotTests::RunAsync,
                                         ::Notepads::Markdown::Native::ReadingSnapshotTests::CheckCount());
    }

    static Windows::Foundation::IAsyncAction RunImagesWithPayloadAsync(
        Windows::UI::Core::CoreDispatcher dispatcher,
        std::shared_ptr<::Notepads::Markdown::Native::CoreControllerTests::Results> results,
        std::shared_ptr<::Notepads::Markdown::Native::CoreValidation::Context> context)
    {
        co_await ::Notepads::Markdown::Native::ImageResourcesTests::RunAsync(dispatcher, results, context);
        results->checks.push_back(::Notepads::Markdown::Native::ImageViewportTests::Run());
        co_await ::Notepads::Markdown::Native::ImagePayloadTests::RunAsync(dispatcher, results, context);
    }

    Windows::Foundation::IAsyncOperation<Markdown::CoreValidationReport> CoreValidationProbe::RunImagesAsync()
    {
        return RunNativeDiagnosticsAsync(
            [](auto const &dispatcher, auto const &results, auto const &context)
            { return RunImagesWithPayloadAsync(dispatcher, results, context); },
            ::Notepads::Markdown::Native::ImageResourcesTests::CheckCount() + 2);
    }

    Windows::Foundation::IAsyncOperation<Markdown::CoreValidationReport> CoreValidationProbe::RunHighlightAsync()
    {
        return RunNativeDiagnosticsAsync(
            [](auto const &dispatcher, auto const &results, auto const &context)
            { return ::Notepads::Markdown::Native::HighlightSnapshotTests::RunAsync(dispatcher, results, context); }, 3);
    }

    uint32_t CoreValidationProbe::LiveWorkers() noexcept
    {
        return ::Notepads::Markdown::Native::CoreValidation::LiveWorkers();
    }

    uint32_t CoreValidationProbe::LiveHandles() noexcept
    {
        return ::Notepads::Markdown::Native::CoreValidation::LiveHandles();
    }

    com_array<uint64_t> CoreValidationProbe::GetImageProcessCapacitySnapshot()
    {
        ::Notepads::Markdown::Native::ModuleLifetime module;
        notepads_markdown_image_process_capacity capacity{};
        capacity.header = {NOTEPADS_MARKDOWN_IMAGE_ABI_VERSION, sizeof(capacity)};
        notepads_markdown_core_error error{};
        error.header = {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(error)};
        if (notepads_markdown_image_get_process_capacity(&capacity, &error) != NOTEPADS_MARKDOWN_CORE_OK)
            throw hresult_error(E_FAIL, L"Image process capacity query failed.");
        return {capacity.generation, capacity.encoded_bytes, capacity.decoded_bytes};
    }

    uint32_t CoreValidationProbe::SearchContextCount(Markdown::MarkdownView const &view)
    {
        if (!view)
            throw hresult_invalid_argument();
        // Validate the owning apartment before unwrapping our implementation.
        if (!view.Dispatcher().HasThreadAccess())
            throw hresult_wrong_thread();
        return get_self<MarkdownView>(view)->SearchContextCount();
    }

    com_array<uint64_t> CoreValidationProbe::GetResourceWorkSnapshot(Markdown::MarkdownView const &view)
    {
        if (!view)
            throw hresult_invalid_argument();
        if (!view.Dispatcher().HasThreadAccess())
            throw hresult_wrong_thread();
        return get_self<MarkdownView>(view)->ResourceWorkSnapshotForDiagnostics();
    }

    void CoreValidationProbe::StartPreviewStartupTrace(bool synchronousHighlights)
    {
        ::Notepads::Markdown::Native::StartupTrace::Start(synchronousHighlights);
    }

    bool CoreValidationProbe::StartupHighlightsPending(Markdown::MarkdownView const &view)
    {
        if (!view)
            throw hresult_invalid_argument();
        if (!view.Dispatcher().HasThreadAccess())
            throw hresult_wrong_thread();
        return get_self<MarkdownView>(view)->StartupHighlightsPendingForDiagnostics();
    }

    void CoreValidationProbe::SetViewportHighlightBaseline(bool value)
    {
        ::Notepads::Markdown::Native::StartupTrace::fullHighlightBaseline.store(value);
    }

    com_array<uint64_t> CoreValidationProbe::GetViewportHighlightDiagnostics(Markdown::MarkdownView const &view)
    {
        if (!view)
            throw hresult_invalid_argument();
        return get_self<MarkdownView>(view)->ViewportHighlightDiagnostics();
    }

    com_array<uint64_t> CoreValidationProbe::StopPreviewStartupTrace()
    {
        return ::Notepads::Markdown::Native::StartupTrace::Stop();
    }

    com_array<double> CoreValidationProbe::GetStartupHeadingGeometry(Markdown::MarkdownView const &view, hstring const &title)
    {
        if (!view)
            throw hresult_invalid_argument();
        if (!view.Dispatcher().HasThreadAccess())
            throw hresult_wrong_thread();
        return get_self<MarkdownView>(view)->StartupHeadingGeometryForDiagnostics(title);
    }

    uint32_t CoreValidationProbe::SearchOperationCount(Markdown::MarkdownView const &view)
    {
        if (!view)
            throw hresult_invalid_argument();
        if (!view.Dispatcher().HasThreadAccess())
            throw hresult_wrong_thread();
        return get_self<MarkdownView>(view)->SearchOperationCount();
    }

    void CoreValidationProbe::ArmPreviewDeviceLoss(Markdown::MarkdownView const &view, bool initialViewport)
    {
        if (!view)
            throw hresult_invalid_argument();
        if (!view.Dispatcher().HasThreadAccess())
            throw hresult_wrong_thread();
        get_self<MarkdownView>(view)->ArmPreviewDeviceLossForDiagnostics(initialViewport);
    }

    void CoreValidationProbe::TestCodeCopy(Markdown::MarkdownView const &view, bool clipboardSucceeds)
    {
        if (!view)
            throw hresult_invalid_argument();
        get_self<MarkdownView>(view)->CopyCodeBlockForDiagnostics(clipboardSucceeds);
    }

    bool CoreValidationProbe::HasNoteEmbedParentPin(Markdown::MarkdownView const &view)
    {
        if (!view)
            throw hresult_invalid_argument();
        return get_self<MarkdownView>(view)->HasNoteEmbedParentPinForDiagnostics();
    }

}
