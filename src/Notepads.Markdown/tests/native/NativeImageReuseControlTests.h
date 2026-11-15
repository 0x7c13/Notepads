// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "CoreControllerTests.h"
#include "Resources/Images/NativeImageResources.h"
#include <winrt/Windows.UI.Core.h>
#include <winrt/Windows.UI.Xaml.Controls.h>
#include <mutex>
#include <string>
#include <utility>

namespace winrt::Notepads::Markdown { struct MarkdownView; }
namespace Notepads::Markdown::Native
{
    struct ImageReuseControlObservation
    {
        bool loaded{}, current{}, found{};
        uint64_t text{}, cacheScope{}, cacheMetadata{}, cacheEncoded{}, cacheCount{}, rowEncoded{};
        uint32_t textStart{};
        int64_t animationStartTicks{};
        uint32_t frameIndex{};
        notepads_markdown_image_epoch epoch{};
        notepads_markdown_image_entry row{};
        notepads_markdown_image_snapshot usage{};
    };
    struct ViewportRasterBandObservation
    {
        uintptr_t surface{};
        uint64_t bytes{};
        int64_t firstRow{}, viewportRow{};
        int32_t pixelHeight{}, viewportPixelHeight{};
        double viewportY{}, scale{};
        bool current{}, cropMatches{};
    };
    namespace ImageReuseControlTests
    {
        // A value-only failure transcript bypasses the bounded HRESULT message
        // transport. Cancellation may notify the caller before the writer exits.
        class FailureDiagnostics
        {
          public:
            void Set(std::wstring value)
            {
                std::lock_guard guard(_lock);
                _detail = std::move(value);
            }
            std::wstring Snapshot() const
            {
                std::lock_guard guard(_lock);
                return _detail;
            }

          private:
            mutable std::mutex _lock;
            std::wstring _detail;
        };
        // Root can await these assertions from the existing payload case,
        // preserving the suite count. Requires the actual diagnostic UI.
        winrt::Windows::Foundation::IAsyncAction RunAsync(
            winrt::Windows::UI::Core::CoreDispatcher dispatcher,
            std::shared_ptr<CoreValidation::Context> context,
            std::shared_ptr<FailureDiagnostics> diagnostics);
    }
}
namespace winrt::Notepads::Markdown::implementation
{
    struct ImageReuseControlTestAccess
    {
        // Real empty-view setup only; does not admit a reader or image request.
        static void PrepareInitialViewport(Markdown::MarkdownView const &view);
        static bool InitialViewportReady(Markdown::MarkdownView const &view);
        static bool ReplacesPendingWheelAtTop(Markdown::MarkdownView const &view);
        // Pumps the existing policy only after a real current viewport matches the expected range.
        static bool PumpImageAtCurrentViewport(Markdown::MarkdownView const &view, bool offscreen);
        static bool HoverPendingImage(Markdown::MarkdownView const &view);
        static hstring HoverText(Markdown::MarkdownView const &view);
        static bool ReadyImageAutomation(Markdown::MarkdownView const &view);
        static ::Notepads::Markdown::Native::ImageReuseControlObservation Observe(Markdown::MarkdownView const &view, uint32_t occurrence = 0);
        // Sole explicit strong observation: pixels only, never a Frame/source.
        static std::shared_ptr<::Notepads::Markdown::Native::ImagePixels const> Pixels(Markdown::MarkdownView const &view, uint32_t occurrence = 0);
        static ::Notepads::Markdown::Native::NativeEncodedImageHandle HoldEncoded(Markdown::MarkdownView const &view);
        static void PressureClear(Markdown::MarkdownView const &view);
        static ::Notepads::Markdown::Native::ViewportRasterBandObservation ObserveRasterBand(
            Markdown::MarkdownView const &view);
        static Windows::UI::Xaml::Controls::Border RasterTarget(Markdown::MarkdownView const &view);
    };
}
