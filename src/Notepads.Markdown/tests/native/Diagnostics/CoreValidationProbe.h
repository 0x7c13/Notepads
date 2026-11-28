// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "CoreValidationProbe.g.h"

namespace winrt::Notepads::Markdown::implementation
{
    // Static-only runtimeclasses have no implementation instance template.
    struct CoreValidationProbe
    {
        static Windows::Foundation::IAsyncOperation<Markdown::CoreValidationReport> RunAsync();
        static Windows::Foundation::IAsyncOperation<Markdown::CoreValidationReport> RunPreviewAsync();
        static Windows::Foundation::IAsyncOperation<Markdown::CoreValidationReport> RunReadingAsync();
        static Windows::Foundation::IAsyncOperation<Markdown::CoreValidationReport> RunImagesAsync();
        static Windows::Foundation::IAsyncOperation<Markdown::CoreValidationReport> RunHighlightAsync();
        static Windows::Foundation::IAsyncOperation<Markdown::CoreValidationReport> RunImageProviderAsync(Markdown::IMarkdownImageProvider provider);
        static uint32_t LiveWorkers() noexcept;
        static uint32_t LiveHandles() noexcept;
        static com_array<uint64_t> GetImageProcessCapacitySnapshot();
        static com_array<uint64_t> GetResourceWorkSnapshot(Markdown::MarkdownView const &view);
        static void StartPreviewStartupTrace(bool synchronousHighlights);
        static bool StartupHighlightsPending(Markdown::MarkdownView const &view);
        static void SetViewportHighlightBaseline(bool value);
        static com_array<uint64_t> GetViewportHighlightDiagnostics(Markdown::MarkdownView const &view);
        static com_array<uint64_t> StopPreviewStartupTrace();
        static com_array<double> GetStartupHeadingGeometry(Markdown::MarkdownView const &view, hstring const &title);
        static void ArmPreviewDeviceLoss(Markdown::MarkdownView const &view, bool initialViewport);
        static bool HasNoteEmbedParentPin(Markdown::MarkdownView const &view);
        static void TestCodeCopy(Markdown::MarkdownView const &view, bool clipboardSucceeds);
        static uint32_t SearchContextCount(Markdown::MarkdownView const &view);
        static uint32_t SearchOperationCount(Markdown::MarkdownView const &view);
    };
}

namespace winrt::Notepads::Markdown::factory_implementation
{
    struct CoreValidationProbe : CoreValidationProbeT<CoreValidationProbe, implementation::CoreValidationProbe>
    {
        static Windows::Foundation::IAsyncOperation<Markdown::CoreValidationReport> RunAsync()
        {
            return implementation::CoreValidationProbe::RunAsync();
        }
        static Windows::Foundation::IAsyncOperation<Markdown::CoreValidationReport> RunImageProviderAsync(Markdown::IMarkdownImageProvider provider)
        {
            return implementation::CoreValidationProbe::RunImageProviderAsync(std::move(provider));
        }
        static Windows::Foundation::IAsyncOperation<Markdown::CoreValidationReport> RunImagesAsync()
        {
            return implementation::CoreValidationProbe::RunImagesAsync();
        }
        static Windows::Foundation::IAsyncOperation<Markdown::CoreValidationReport> RunHighlightAsync()
        {
            return implementation::CoreValidationProbe::RunHighlightAsync();
        }
        static uint32_t LiveWorkers() noexcept
        {
            return implementation::CoreValidationProbe::LiveWorkers();
        }
        static Windows::Foundation::IAsyncOperation<Markdown::CoreValidationReport> RunPreviewAsync()
        {
            return implementation::CoreValidationProbe::RunPreviewAsync();
        }
        static uint32_t LiveHandles() noexcept
        {
            return implementation::CoreValidationProbe::LiveHandles();
        }
        static com_array<uint64_t> GetImageProcessCapacitySnapshot()
        {
            return implementation::CoreValidationProbe::GetImageProcessCapacitySnapshot();
        }
        static com_array<uint64_t> GetResourceWorkSnapshot(Markdown::MarkdownView const &view)
        {
            return implementation::CoreValidationProbe::GetResourceWorkSnapshot(view);
        }
        static void StartPreviewStartupTrace(bool synchronousHighlights)
        {
            implementation::CoreValidationProbe::StartPreviewStartupTrace(synchronousHighlights);
        }
        static bool StartupHighlightsPending(Markdown::MarkdownView const &view)
        {
            return implementation::CoreValidationProbe::StartupHighlightsPending(view);
        }
        static void SetViewportHighlightBaseline(bool value)
        {
            implementation::CoreValidationProbe::SetViewportHighlightBaseline(value);
        }
        static com_array<uint64_t> GetViewportHighlightDiagnostics(Markdown::MarkdownView const &view)
        {
            return implementation::CoreValidationProbe::GetViewportHighlightDiagnostics(view);
        }
        static com_array<uint64_t> StopPreviewStartupTrace()
        {
            return implementation::CoreValidationProbe::StopPreviewStartupTrace();
        }
        static com_array<double> GetStartupHeadingGeometry(Markdown::MarkdownView const &view, hstring const &title)
        {
            return implementation::CoreValidationProbe::GetStartupHeadingGeometry(view, title);
        }
        static void ArmPreviewDeviceLoss(Markdown::MarkdownView const &view, bool initialViewport)
        {
            implementation::CoreValidationProbe::ArmPreviewDeviceLoss(view, initialViewport);
        }
        static bool HasNoteEmbedParentPin(Markdown::MarkdownView const &view)
        {
            return implementation::CoreValidationProbe::HasNoteEmbedParentPin(view);
        }
        static void TestCodeCopy(Markdown::MarkdownView const &view, bool clipboardSucceeds)
        {
            implementation::CoreValidationProbe::TestCodeCopy(view, clipboardSucceeds);
        }
        static Windows::Foundation::IAsyncOperation<Markdown::CoreValidationReport> RunReadingAsync()
        {
            return implementation::CoreValidationProbe::RunReadingAsync();
        }
        static uint32_t SearchContextCount(Markdown::MarkdownView const &view)
        {
            return implementation::CoreValidationProbe::SearchContextCount(view);
        }
        static uint32_t SearchOperationCount(Markdown::MarkdownView const &view)
        {
            return implementation::CoreValidationProbe::SearchOperationCount(view);
        }
    };
}
