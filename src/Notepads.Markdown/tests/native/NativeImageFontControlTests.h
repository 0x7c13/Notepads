// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information (MIT).
// ---------------------------------------------------------------------------------------------
#pragma once
#include "CoreControllerTests.h"
#include "Resources/Images/NativeImageCoordinator.h"
#include <winrt/Windows.UI.Xaml.h>
#include <array>
#include <atomic>
#include <memory>
#include <mutex>
#include <optional>
#include <string>
#include <vector>

namespace winrt::Notepads::Markdown
{
    struct MarkdownView;
}

namespace Notepads::Markdown::Native
{
    // Owned by the real request as well as the test; no handle can close while
    // a worker uses it. All waits are bounded and cleanup releases both gates.
    struct ReadingPublicationControlGate
    {
        std::atomic<bool> firstCompleted{};
        std::atomic<uint64_t> completedRequests{}, firstToken{}, firstLatestToken{}, firstLayout{};
        std::atomic<int32_t> firstStatus{};
        std::array<winrt::handle, 2> reached, proceed;
        std::atomic<bool> timedOut{};
        ReadingPublicationControlGate();
        ~ReadingPublicationControlGate();
        void Pause(uint32_t stage);
        bool Reached(uint32_t stage) const noexcept;
        void Release(uint32_t stage) noexcept;
        void ReleaseAll() noexcept;
    };

    // Test-owned storage. The control and its actual observer can share this
    // storage, but it never owns a view, dispatcher, source, or observer.
    struct ImageFontControlTestHooks
    {
        // Only sampled DPI inputs are replaceable; captures and target keys stay real.
        std::optional<std::array<double, 2>> rasterScaleInputs;
        bool failNextFreeze{}, gateNextTask{}, gateReading{};
        bool gateColdAutoTask{}; // Explicit test-only opt-in before SVG sniffing.
        uint64_t resourceCallbacks{}; // UI only, counts actual deferred callbacks.
        std::shared_ptr<ReadingPublicationControlGate> readingPublication;
        uint64_t freezeCalls{}, armedFreezeCall{}, failedFreezeCall{}, freezeFaultHits{};
        uint64_t forbiddenRequest{}, viewportEvents{};
        bool forbiddenPublished{}, wrongFontPublished{}, publicationError{};
        std::atomic<uint32_t> watchesCreated{}, watchesFreed{}, watchFreeFailures{};
        std::vector<std::wstring> publications;
        std::wstring phase;

        ~ImageFontControlTestHooks();
        ReadingAllocationFactory BeforeFreeze() noexcept;
        void InitializeTask(std::shared_ptr<ImageLoadTask> const &task);
        std::shared_ptr<ImageLoadTask> HeldTask() const;
        void ReleaseGate() noexcept;
        void ForgetTask() noexcept;

      private:
        static void *FailFreeze(void *context, ReadingMemory const &memory, size_t bytes);
        mutable std::mutex _taskGate;
        std::shared_ptr<ImageLoadTask> _task;
    };

    // Value-only observations: no Frame/table/encoded strong pin is hidden in
    // the diagnostic oracle or its publication transcript.
    struct ImageFontControlObservation
    {
        bool enabled{}, loaded{}, source{}, surface{}, currentAction{}, currentImage{}, watchIdle{}, watchBaseline{};
        bool refreshScheduled{};
        bool readingMain{}, readingDescriptors{}, mathBound{}, diagramBound{};
        bool svgFound{}, rasterFound{}, frameSvgFound{}, frameRasterFound{};
        uint64_t textGeneration{}, fontGeneration{}, loadedGeneration{}, watchVersion{}, barrier{}, layoutGeneration{};
        uint64_t tableGeneration{}, frameGeneration{}, tableTextGeneration{};
        uintptr_t table{}, frame{}, frameTable{}, publishedTable{}, encoded{}, rasterPixels{};
        double actualWidth{}, viewportWidth{}, measuredWidth{};
        notepads_markdown_image_epoch epoch{}, tableEpoch{};
        uint64_t policyGeneration{};
        notepads_markdown_image_entry svg{}, raster{}, frameSvg{}, frameRaster{};
        winrt::hstring error;
        std::wstring Describe() const;
    };

    namespace ImageFontControlTests
    {
        // Starts after the original image fixture has completely retired.
        // All coroutine arguments are owned by value; total wait is <=45s.
        winrt::Windows::Foundation::IAsyncAction RunAsync(
            winrt::Windows::UI::Core::CoreDispatcher dispatcher,
            std::shared_ptr<CoreControllerTests::Results> results,
            std::shared_ptr<CoreValidation::Context> context);
    }
}

namespace winrt::Notepads::Markdown::implementation
{
    // Native/private bridge only. Implemented beside the complete production
    // ImageFontWatch; it never fabricates a font notification or image result.
    struct ImageFontControlTestAccess
    {
        static void Attach(Markdown::MarkdownView const &view,
                           std::shared_ptr<::Notepads::Markdown::Native::ImageFontControlTestHooks> hooks);
        static ::Notepads::Markdown::Native::ImageFontControlObservation Observe(Markdown::MarkdownView const &view);
        static bool RequestRefresh(Markdown::MarkdownView const &view);
        static bool CoalescePendingResourceLayout(Markdown::MarkdownView const &view);
        static bool CheckRasterScaleTargets(Markdown::MarkdownView const &view);
        static void EnsureFontWatch(Markdown::MarkdownView const &view);
        static ::Notepads::Markdown::Native::CoreControllerLifetime Lifetime(Markdown::MarkdownView const &view);
        static std::weak_ptr<::Notepads::Markdown::Native::ReadingMemory> SourceMemory(Markdown::MarkdownView const &view);
        static std::array<std::shared_ptr<::Notepads::Markdown::Native::ImageLoadTask>, 2> Tasks(Markdown::MarkdownView const &view);
        static void DetachAndStopWatch(Markdown::MarkdownView const &view);
    };
}
