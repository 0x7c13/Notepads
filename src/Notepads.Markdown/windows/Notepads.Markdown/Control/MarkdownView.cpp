// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include <winrt/Windows.Foundation.Collections.h>
#include "Control/MarkdownView.h"
#include "Control/MarkdownView.ReadingState.h"
#include "Control/ViewportRasterBand.h"
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
#include "NativeImageFontControlTests.h"
#include "NativeImageReuseControlTests.h"
#include "Diagnostics/NativeStartupTrace.h"
#endif
#include "MarkdownView.g.cpp"
#include "Automation/MarkdownViewAutomationPeer.h"
#include "Control/DocumentUpdateResult.h"
#include "Control/PreviewUpdate.h"
#include "Rendering/NativeMeasurementCache.h"
#include "Resources/Images/NativeImageCoordinator.h"
#include "Resources/Images/NativeImageReuseCache.h"
#include "Control/MarkdownSelection.h"
#include "Control/MarkdownLinkRequestedEventArgs.h"
#include "Automation/ReadingObjectProvider.h"
#include "Automation/TextRangeProvider.h"
#include <winrt/Windows.ApplicationModel.DataTransfer.h>
#include <winrt/Windows.System.h>
#include <winrt/Windows.Devices.Input.h>
#include <winrt/Windows.UI.Input.h>
#include <cmath>
#include <utility>
#include <exception>
#include <winrt/Windows.Foundation.Metadata.h>

namespace Notepads::Markdown::Native
{
    struct PreviewRenderRequest
    {
        ModuleLifetime module;
        winrt::handle completed;
        std::shared_ptr<std::atomic<uint64_t>> latestToken;
        uint64_t token{};
        uint64_t sourceGeneration{};
        std::shared_ptr<winrt::Notepads::Markdown::implementation::PreviewUpdate> update;
        notepads_markdown_preview_key key{};
        PreviewTheme theme;
        winrt::hstring sectionTitle;
        CoreModelPin model;
        std::shared_ptr<PreviewCancellation> cancel;
        std::shared_ptr<ReadingSourceState> reading;
        std::shared_ptr<ReadingVector<notepads_markdown_preview_disclosure_override> const> disclosureOverrides;
        std::shared_ptr<PreviewNoteEmbedExtents const> noteEmbedExtents;
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        std::shared_ptr<ReadingPublicationControlGate> readingPublicationGate;
        uint64_t startupTrace{};
#endif
        std::shared_ptr<ImagePresentationSnapshot const> images;
        PreviewMathSnapshot math;
        PreviewDiagramSnapshot diagram;
        std::shared_ptr<NativeDocumentRenderer> renderer;
        std::shared_ptr<NativeMeasurementCache> measurementCache;
        std::optional<NativeDocumentRenderer::InitialViewport> initialViewport;
        bool prepareEditResources{};
        PreviewEditResources editResources;
        std::shared_ptr<PreviewFrame> frame;
        double y{};
        double height{};
        ViewportGeometry geometry;
        uint64_t viewportSerial{};
        enum class UiPublication
        {
            Stale,
            Published,
            DrawFailed
        };
        // UI-thread result, independent of the background frame status.
        UiPublication publication{UiPublication::Stale};
        std::atomic<bool> deliveryFailed{};
        int32_t status{NOTEPADS_MARKDOWN_CORE_OK};
        wchar_t const *failureStage{L"reading preparation"};
        explicit PreviewRenderRequest(bool background = true)
        {
            if (background)
            {
                completed.attach(::CreateEventExW(nullptr, nullptr, CREATE_EVENT_MANUAL_RESET, EVENT_ALL_ACCESS));
                winrt::check_bool(static_cast<bool>(completed));
            }
        }
        void Run() noexcept
        {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            StartupTrace::WorkerTrace trace{startupTrace};
#endif
            try
            {
                if (!renderer)
                {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                    StartupTrace::Mark(StartupTrace::Phase::WorkerStarted, key.layout_generation);
                    if (readingPublicationGate)
                        readingPublicationGate->Pause(0);
#endif
                    renderer = NativeDocumentRenderer::Create(std::move(model), cancel, key, std::move(theme), reading,
                                                              images, math, diagram, measurementCache,
                                                              initialViewport ? &*initialViewport : nullptr,
                                                              disclosureOverrides ? std::span<notepads_markdown_preview_disclosure_override const>{*disclosureOverrides}
                                                                                  : std::span<notepads_markdown_preview_disclosure_override const>{},
                                                              sectionTitle, noteEmbedExtents,
                                                              prepareEditResources ? &editResources : nullptr);
                    // UI concurrently observes the enqueued generation/theme/width.
                    // Only the worker-owned indent changes during marker preflight.
                    key.indent = renderer->Key().indent;
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                    if (readingPublicationGate)
                        readingPublicationGate->Pause(1);
#endif
                }
                if (!prepareEditResources)
                {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                    StartupTrace::Mark(StartupTrace::Phase::FinalFrame, key.layout_generation);
#endif
                    frame = renderer->CreateFrame(y, height, token, *latestToken);
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                    if (frame)
                        StartupTrace::Mark(StartupTrace::Phase::FinalFrameReady, key.layout_generation);
#endif
                }
            }
            catch (PreviewFailure const &error)
            {
                status = error.status;
                failureStage = error.stage;
            }
            catch (ReadingFailure const &error)
            {
                status = error.status;
            }
            catch (std::bad_alloc const &)
            {
                status = NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY;
            }
            catch (winrt::hresult_error const &error)
            {
                status = error.code().value;
            }
            catch (...)
            {
                status = NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT;
            }
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            if (readingPublicationGate && readingPublicationGate->completedRequests.fetch_add(1) == 0)
            {
                readingPublicationGate->firstStatus.store(status, std::memory_order_relaxed);
                readingPublicationGate->firstToken.store(token, std::memory_order_relaxed);
                readingPublicationGate->firstLatestToken.store(latestToken ? latestToken->load(std::memory_order_acquire) : 0,
                                                               std::memory_order_relaxed);
                readingPublicationGate->firstLayout.store(key.layout_generation, std::memory_order_relaxed);
                readingPublicationGate->firstCompleted.store(true, std::memory_order_release);
            }
#endif
            ::SetEvent(completed.get());
        }
    };

    struct PreviewRenderQueue : std::enable_shared_from_this<PreviewRenderQueue>
    {
        ModuleLifetime module;
        std::mutex mutex;
        std::shared_ptr<PreviewRenderRequest> active;
        std::shared_ptr<PreviewRenderRequest> pending;
        void Enqueue(std::shared_ptr<PreviewRenderRequest> request)
        {
            bool start{};
            {
                std::lock_guard guard(mutex);
                if (pending)
                {
                    pending->status = NOTEPADS_MARKDOWN_CORE_CANCELLED;
                    ::SetEvent(pending->completed.get());
                }
                pending = std::move(request);
                if (!active)
                {
                    active = std::move(pending);
                    start = true;
                }
            }
            if (start)
            {
                try
                {
                    winrt::Windows::System::Threading::ThreadPool::RunAsync([owner = shared_from_this()](auto const &)
                                                                            { owner->Run(); });
                }
                catch (...)
                {
                    std::lock_guard guard(mutex);
                    active->status = NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT;
                    ::SetEvent(active->completed.get());
                    active.reset();
                    throw;
                }
            }
        }
        void Run() noexcept
        {
            for (;;)
            {
                std::shared_ptr<PreviewRenderRequest> work;
                {
                    std::lock_guard guard(mutex);
                    work = active;
                }
                work->Run();
                {
                    std::lock_guard guard(mutex);
                    active = std::move(pending);
                    if (!active)
                        break;
                }
            }
        }
    };
}

namespace winrt::Notepads::Markdown::implementation
{
    using namespace Windows::Foundation;
    using namespace Windows::UI::Xaml;
    using namespace Windows::UI::Xaml::Controls;
    using namespace Windows::UI::Xaml::Media;
    using namespace Windows::UI::Xaml::Media::Imaging;

    struct PreviewHighlightWork
    {
        std::shared_ptr<::Notepads::Markdown::Native::ReadingSourceState> source;
        std::shared_ptr<::Notepads::Markdown::Native::NativeHighlightCache> cache;
        std::shared_ptr<::Notepads::Markdown::Native::PreviewCancellation> cancel;
        MarkdownView::HighlightSelection selection;
        bool superseded{}; // UI only. The cancellation flag can be signaled during FFI work.
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        uint64_t startupTrace{};
#endif

        void Supersede() noexcept
        {
            if (!std::exchange(superseded, true) && cancel)
                cancel->Cancel();
        }
    };

    struct PreviewRasterBandState
    {
        std::shared_ptr<::Notepads::Markdown::Native::PreviewFrame> frame;
        ::Notepads::Markdown::Native::ViewportRasterBand band;
        ::Notepads::Markdown::Native::ViewportGeometry geometry;
        uint32_t selectionStart{}, selectionEnd{};
        ::Notepads::Markdown::Native::ReadingActionTarget focusedAction;
        bool focused{}, inactiveCaret{}, trailingCaret{}, mayExpand{true};
        std::optional<size_t> hoveredLink;

        bool SameSourceLayout(::Notepads::Markdown::Native::PreviewFrame const &candidate,
                              ::Notepads::Markdown::Native::ViewportGeometry const &viewport) const noexcept
        {
            return frame && frame->token == candidate.token &&
                frame->key.layout_generation == candidate.key.layout_generation &&
                frame->key.theme_generation == candidate.key.theme_generation &&
                frame->main == candidate.main &&
                frame->descriptors == candidate.descriptors && frame->navigation == candidate.navigation &&
                frame->quotes == candidate.quotes &&
                frame->images == candidate.images && frame->math.generation == candidate.math.generation &&
                frame->math.enabled == candidate.math.enabled &&
                frame->diagram.generation == candidate.diagram.generation &&
                frame->diagram.enabled == candidate.diagram.enabled && frame->highlights == candidate.highlights &&
                frame->textPrewarm == candidate.textPrewarm &&
                ::Notepads::Markdown::Native::SameViewportRaster(geometry, viewport);
        }

        bool SameContent(::Notepads::Markdown::Native::PreviewFrame const &candidate,
                         ::Notepads::Markdown::Native::ViewportGeometry const &viewport) const noexcept
        {
            return SameSourceLayout(candidate, viewport) && frame->measuredBlocks == candidate.measuredBlocks;
        }
    };

    namespace
    {
        thread_local std::weak_ptr<ID3D11Device> sharedD3dDevice;

        bool HasXamlRoot()
        {
            // OS API presence is process-invariant; the root and its scale are not.
            static const bool present = Windows::Foundation::Metadata::ApiInformation::IsPropertyPresent(
                L"Windows.UI.Xaml.UIElement", L"XamlRoot");
            return present;
        }

        bool StaticRasterBand(::Notepads::Markdown::Native::PreviewFrame const &frame,
            ::Notepads::Markdown::Native::ViewportRasterBand const &band,
            ::Notepads::Markdown::Native::ViewportGeometry const &geometry, double padding,
            uint32_t selectionStart, uint32_t selectionEnd,
            ::Notepads::Markdown::Native::ReadingActionTarget action, bool focused, bool inactiveCaret,
            std::optional<size_t> hoveredLink, bool trailingCaret) noexcept
        {
            using namespace ::Notepads::Markdown::Native;
            if (!ViewportRasterPaintIsStatic(selectionStart != selectionEnd, false, false, hoveredLink.has_value()))
                return false;
            try
            {
                // Quote paint clips to logical viewportHeight. Retain only
                // quote-free bands until that painter has physical-edge semantics.
                if (frame.quotes)
                    for (auto const &quote : *frame.quotes)
                        if (ViewportRasterOverlapsRange(band, geometry, padding, quote.y, quote.height))
                            return false;
                auto visibleRange = [&](uint32_t start, uint32_t end, double inkMargin = 0, bool trailing = false)
                {
                    for (auto const &rectangle : NativeDocumentRenderer::Rectangles(frame, start, end, trailing))
                        if (ViewportRasterOverlapsRange(band, geometry, padding,
                                frame.viewportY + rectangle.y - inkMargin, rectangle.height + 2 * inkMargin))
                            return true;
                    return false;
                };
                // A configured inactive marker away from the band paints no
                // pixels and must not disable retention throughout a document.
                const bool caret = (focused || inactiveCaret) &&
                    visibleRange(selectionStart, selectionEnd, 0, trailingCaret);
                bool actionPainted{};
                if (focused && frame.descriptors)
                {
                    ReadingSpan range{};
                    if (action.kind == ReadingActionKind::Link && action.index < frame.descriptors->links.size())
                        range = frame.descriptors->links[action.index].text;
                    else if (action.kind == ReadingActionKind::Disclosure && action.index < frame.descriptors->disclosures.size())
                        range = frame.descriptors->disclosures[action.index].text;
                    else if ((action.kind == ReadingActionKind::Task || action.kind == ReadingActionKind::Diagram) &&
                             action.index < frame.descriptors->objects.size())
                    {
                        auto const &object = frame.descriptors->objects[action.index];
                        if ((action.kind == ReadingActionKind::Task && object.kind == ReadingObjectKind::Task &&
                             !object.staticHtmlCheckbox && object.hasTaskAction) ||
                            (action.kind == ReadingActionKind::Diagram && object.kind == ReadingObjectKind::Diagram &&
                             frame.DiagramState(object.object) == NativeDiagramState::Ready))
                            range = object.text;
                    }
                    // Draw expands by 1 DIP, then strokes at 1.5 DIP: the
                    // outer focus ink extends 1.75 DIP beyond the range rect.
                    actionPainted = range.start != range.end && visibleRange(range.start, range.end, 1.75);
                }
                return ViewportRasterPaintIsStatic(false, caret, actionPainted, false);
            }
            catch (...) { return false; } // The optional reuse check cannot fail an ordinary paint.
        }

        bool RasterBandTemplate(Border const &target, ImageBrush const &brush)
        {
            if (!target || !brush || target.Background().try_as<ImageBrush>() != brush || target.Child() || target.Clip() ||
                target.Opacity() != 1 || brush.Opacity() != 1 || brush.Stretch() != Stretch::Fill ||
                brush.AlignmentX() != AlignmentX::Left || brush.AlignmentY() != AlignmentY::Top)
                return false;
            auto const border = target.BorderThickness();
            auto const padding = target.Padding();
            auto const corner = target.CornerRadius();
            if (border.Left != 0 || border.Top != 0 || border.Right != 0 || border.Bottom != 0 ||
                padding.Left != 0 || padding.Top != 0 || padding.Right != 0 || padding.Bottom != 0 ||
                corner.TopLeft != 0 || corner.TopRight != 0 || corner.BottomLeft != 0 || corner.BottomRight != 0)
                return false;
            auto neutral = [](Transform const &transform)
            {
                if (!transform)
                    return true;
                auto const origin = transform.TransformPoint({0, 0});
                auto const horizontal = transform.TransformPoint({1, 0});
                auto const vertical = transform.TransformPoint({0, 1});
                return origin.X == 0 && origin.Y == 0 && horizontal.X == 1 && horizontal.Y == 0 &&
                       vertical.X == 0 && vertical.Y == 1;
            };
            return neutral(brush.Transform()) && neutral(brush.RelativeTransform());
        }

        bool Within(DependencyObject child, DependencyObject const &ancestor)
        {
            while (child)
            {
                if (child == ancestor) return true;
                child = VisualTreeHelper::GetParent(child);
            }
            return false;
        }

        DependencyObject VisualRoot(DependencyObject const &owner)
        {
            return owner && VisualTreeHelper::GetChildrenCount(owner) ?
                VisualTreeHelper::GetChild(owner, 0) : nullptr;
        }
    }

    struct PreviewDrain
    {
        std::mutex mutex;
        std::atomic<bool> done{};
        bool terminal{};
        uint64_t serial{};
        std::shared_ptr<PreviewUpdate> waiting;
        uint64_t Begin()
        {
            std::lock_guard guard(mutex);
            ++serial; // Controller creation refuses an exhausted storage serial.
            terminal = false;
            waiting.reset();
            done.store(false, std::memory_order_release);
            return serial;
        }
        bool Wait(std::shared_ptr<PreviewUpdate> const &update)
        {
            std::lock_guard guard(mutex);
            if (!done.load(std::memory_order_acquire))
            {
                waiting = update;
                return true;
            }
            if (waiting == update)
                waiting.reset();
            return false;
        }
        void Finished(uint64_t expected, bool success)
        {
            std::lock_guard guard(mutex);
            if (serial == expected)
            {
                terminal = success;
                done.store(true, std::memory_order_release);
            }
        }
        bool IsCurrent(uint64_t expected)
        {
            std::lock_guard guard(mutex);
            return serial == expected;
        }
        void DeliveryFailed(uint64_t expected)
        {
            std::lock_guard guard(mutex);
            if (serial == expected && waiting)
            {
                waiting->Complete(Markdown::DocumentUpdateStatus::NotLoaded, NOTEPADS_MARKDOWN_CORE_INVALID_STATE);
                waiting.reset();
            }
        }
    };

#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
    com_array<uint64_t> MarkdownView::ResourceWorkSnapshotForDiagnostics() const
    {
        CheckUiThread();
        // Independently locked observations, not an atomic idle/owner barrier.
        // Never hold one authority's lock while reading another or allocating
        // the result. In particular, Ready resources legitimately stay resident.
        std::array<uint64_t, 20> value{};
        value[0] = 1;
        value[1] = _previewSourceGeneration;
        value[2] = _readingSource ? _readingSource->textGeneration : 0;
        if (_previewController)
        {
            auto core = _previewController->Snapshot();
            value[3] = (core.preparing ? 1 : 0) | (core.active ? 2 : 0) |
                       (core.pending ? 4 : 0) | (core.submitEntered ? 8 : 0) |
                       (core.installEntered ? 16 : 0) |
                       (core.wakePhase != ::Notepads::Markdown::Native::CoreWakePhase::Idle ? 32 : 0);
        }
        if (_previewDrain)
        {
            std::lock_guard lock(_previewDrain->mutex);
            if (_previewDrain->serial != 0)
            {
                bool done = _previewDrain->done.load(std::memory_order_acquire);
                value[4] = !done;
                value[5] = done && !_previewDrain->terminal;
            }
        }
        if (_previewQueue)
        {
            std::lock_guard lock(_previewQueue->mutex);
            value[6] = !!_previewQueue->active;
            value[7] = !!_previewQueue->pending;
        }
        // Desired intent persists after completion; registered Rendering work
        // does not. Include the scheduled event before it creates an active job.
        value[8] = (_viewportActive ? 1 : 0) | (_viewportRendering ? 2 : 0);
        if (_images)
        {
            for (auto const &task : _images->tasks)
                if (task && !task->finished.load(std::memory_order_acquire))
                    ++value[9];
            if (_images->policy)
                value[10] = _images->policy->Snapshot().header.active_requests;
        }
        value[11] = _mathCoordinator ? _mathCoordinator->PendingCountForDiagnostics() : 0;
        value[12] = _diagramCoordinator ? _diagramCoordinator->PendingCountForDiagnostics() : 0;
        value[13] = ((_imageRefreshTimer && _imageRefreshTimer.IsEnabled()) ? 1 : 0) |
                    ((_derivedLayoutRefreshTimer && _derivedLayoutRefreshTimer.IsEnabled()) ? 2 : 0);
        std::fill(value.begin() + 14, value.end(), UINT64_MAX);
        if (_mathCoordinator)
        {
            notepads_markdown_math_usage usage{};
            usage.header = {NOTEPADS_MARKDOWN_MATH_ABI_VERSION, sizeof(usage)};
            notepads_markdown_core_error error{};
            error.header = {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(error)};
            if (_mathCoordinator->Usage(&usage, &error) != NOTEPADS_MARKDOWN_CORE_OK)
                throw hresult_error(E_FAIL, L"Derived resource work query failed.");
            value[14] = usage.control.jobs;
            value[15] = usage.process.jobs;
            value[16] = usage.control.residents;
            value[17] = usage.process.residents;
            value[18] = usage.control.retained_bytes;
            value[19] = usage.process.retained_bytes;
        }
        return {value.begin(), value.end()};
    }
#endif

    void MarkdownView::NotifyImageFramePublished()
    {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        namespace UiTrace = ::Notepads::Markdown::Native::StartupTrace;
        UiTrace::UiScope uiTrace{UiTrace::Phase::UiNotifyPublished, UiTrace::Phase::UiNotifyPublishedEnd, _previewLayoutGeneration};
#endif
        ReadingAction action{_presented.frame, _automationSnapshot, _presented.serial};
        auto const &frame = action.frame;
        auto next = frame ? frame->images : nullptr;
        auto mathGeneration = frame ? frame->math.generation : 0;
        auto mathLayoutGeneration = frame ? frame->key.layout_generation : 0;
        auto diagramGeneration = frame ? frame->diagram.generation : 0;
        bool imagesChanged = next != _publishedImagePresentation &&
            (!next || !_publishedImagePresentation || next->textGeneration != _publishedImagePresentation->textGeneration ||
             next->geometryGeneration != _publishedImagePresentation->geometryGeneration ||
             next->policy.header.state_generation != _publishedImagePresentation->policy.header.state_generation);
        _publishedImagePresentation = std::move(next);
        QueueImageAnimation();
        bool mathArtifactChanged = mathGeneration != _publishedMathGeneration;
        bool mathChanged = mathArtifactChanged || mathLayoutGeneration != _publishedMathLayoutGeneration;
        bool diagramsChanged = diagramGeneration != _publishedDiagramGeneration;
        if (!imagesChanged && !mathChanged && !diagramsChanged)
            return;
        _publishedDiagramGeneration = diagramGeneration;
        auto peers = _readingPeers;
        if (peers && _readingSource && peers->snapshot->token == _readingSource->textGeneration)
        {
            // Resource updates target their own semantic peers. Invalidating links
            // here can detach their XAML pattern adapters during unrelated reflow.
            auto const &objects = peers->snapshot->descriptors->objects;
            for (size_t index = 0; index < objects.size(); ++index)
            {
                if (!_readingSource || peers->snapshot->token != _readingSource->textGeneration)
                    break;
                auto const &object = objects[index];
                auto const &peer = peers->peers[index];
                if (!peer)
                    continue;
                using Kind = ::Notepads::Markdown::Native::ReadingObjectKind;
                if (object.kind == Kind::Math && mathChanged && CurrentReadingAction(action) &&
                    ReadingChildVisible(*peers->snapshot, object.text))
                    peer->UpdateMathHelpText(ReadingMathDescription(object.object, object.syntax));
                if ((object.kind == Kind::Math && mathArtifactChanged && CurrentReadingAction(action)) ||
                    (object.kind == Kind::Image && imagesChanged) ||
                    (object.kind == Kind::Diagram && diagramsChanged))
                    peer->InvalidatePeer();
            }
        }
        // A UIA notification can publish a newer Frame synchronously.
        if (_presented.frame == frame)
        {
            _publishedMathGeneration = mathGeneration;
            _publishedMathLayoutGeneration = mathLayoutGeneration;
        }
    }

    namespace
    {
        Markdown::DocumentUpdateStatus PreviewStatus(int32_t status)
        {
            if (status == NOTEPADS_MARKDOWN_CORE_CANCELLED)
                return Markdown::DocumentUpdateStatus::Canceled;
            if (status == NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT || status == NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY || status == NOTEPADS_MARKDOWN_CORE_DEADLINE)
                return Markdown::DocumentUpdateStatus::ResourceLimit;
            if (status == NOTEPADS_MARKDOWN_CORE_INVALID_UTF8 || status == NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT || status == NOTEPADS_MARKDOWN_CORE_INVALID_RANGE)
                return Markdown::DocumentUpdateStatus::InvalidInput;
            return Markdown::DocumentUpdateStatus::Failed;
        }
    }

    IAsyncOperation<Markdown::DocumentUpdateResult> MarkdownView::SetDocumentAsync(guid document, uint64_t revision,
                                                                              array_view<uint8_t const> source)
    {
        return SetDocumentWithExtensionsAsync(document, revision, source, Markdown::MarkdownExtensions::None);
    }

    IAsyncOperation<Markdown::DocumentUpdateResult> MarkdownView::SetDocumentWithExtensionsAsync(
        guid document, uint64_t revision, array_view<uint8_t const> source, Markdown::MarkdownExtensions extensions)
    {
        CheckUiThread();
        auto update = std::make_shared<PreviewUpdate>();
        update->document = document;
        update->revision = revision;
        update->generation = _generation;
        update->fullSynchronization = true;
        update->waitForEmbeddedViewport = _embedBudgetParent && _noteParentView.get() && !_documentAccepted;
        constexpr auto supportedExtensions =
            static_cast<uint32_t>(Markdown::MarkdownExtensions::Math) |
            static_cast<uint32_t>(Markdown::MarkdownExtensions::Mermaid) |
            static_cast<uint32_t>(Markdown::MarkdownExtensions::HtmlBlocks) |
            static_cast<uint32_t>(Markdown::MarkdownExtensions::HtmlInline) |
            static_cast<uint32_t>(Markdown::MarkdownExtensions::HeadingIds) |
            static_cast<uint32_t>(Markdown::MarkdownExtensions::Footnotes) |
            static_cast<uint32_t>(Markdown::MarkdownExtensions::Callouts) |
            static_cast<uint32_t>(Markdown::MarkdownExtensions::DescriptionLists) |
            static_cast<uint32_t>(Markdown::MarkdownExtensions::Highlight) |
            static_cast<uint32_t>(Markdown::MarkdownExtensions::InlineScripts) |
            static_cast<uint32_t>(Markdown::MarkdownExtensions::EmojiShortcodes) |
            static_cast<uint32_t>(Markdown::MarkdownExtensions::TableOfContents) |
            static_cast<uint32_t>(Markdown::MarkdownExtensions::LocalWikiLinks) |
            static_cast<uint32_t>(Markdown::MarkdownExtensions::NoteEmbeds) |
            static_cast<uint32_t>(Markdown::MarkdownExtensions::FrontMatter) |
            static_cast<uint32_t>(Markdown::MarkdownExtensions::FrontMatterCode) |
            static_cast<uint32_t>(Markdown::MarkdownExtensions::FrontMatterHide) |
            static_cast<uint32_t>(Markdown::MarkdownExtensions::Citations);
        auto const extensionBits = static_cast<uint32_t>(extensions);
        auto const frontMatterMode = extensionBits & 0x600u;
        if ((extensionBits & ~supportedExtensions) != 0 ||
            (frontMatterMode != 0 && (extensionBits & 0x100u) == 0) || frontMatterMode == 0x600u || revision == 0 ||
            source.size() > 4u * 1024u * 1024u ||
            _generation == UINT64_MAX || _previewSourceGeneration == UINT64_MAX ||
            (_acceptedSource && _acceptedDocument == document && revision <= _acceptedRevision))
        {
            _editChannelNeedsResync = true;
            if (_previewUpdate)
                _previewUpdate->fullSynchronization = false;
            update->Complete(Markdown::DocumentUpdateStatus::InvalidInput, NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT);
            return AwaitPreview(std::move(update));
        }
        // Capture borrowed input before template attachment can reenter the host.
        auto owned = std::make_shared<std::vector<uint8_t> const>(source.begin(), source.end());
        if (_cachedPresentationSuspended)
        {
            const auto generation = _generation;
            if (generation >= UINT64_MAX - 1 || !ReopenCachedPresentation())
            {
                update->Complete(Markdown::DocumentUpdateStatus::NotLoaded);
                return AwaitPreview(std::move(update));
            }
            // Template attachment can reenter the host. An intervening source
            // or lifecycle change owns the newer generation.
            if (_generation != generation + 1)
            {
                update->Complete(Markdown::DocumentUpdateStatus::Superseded);
                return AwaitPreview(std::move(update));
            }
        }
        ClearMeasurementCache();
        // Keep the controller's single active/latest pending discipline across
        // ordinary updates and source resynchronization. A local input error
        // never creates a second independent worker/session budget.
        bool resetController = !_documentAccepted || (_previewController && _previewController->Snapshot().closed);
        _editChannelNeedsResync = true;
        StopPreview(Markdown::DocumentUpdateStatus::Superseded, resetController, true,
                    !resetController && _acceptedDocument == document);
        _documentAccepted = true;
        _acceptedSource = std::move(owned);
        _acceptedDocument = document;
        _acceptedRevision = revision;
        _acceptedExtensions = extensions;
        _previewDeviceRetry = false;
        ++_generation;
        ++_previewSourceGeneration;
        update->generation = _generation;
        update->sourceGeneration = _previewSourceGeneration;
        _previewUpdate = update;
        _deferredRender = false;
        if (_previewScroll)
        {
            // Reserve the gutter before width-dependent layout. Publish the
            // source identity first; a synchronous callback may admit a newer
            // document while either XAML property is being changed.
            _previewScroll.VerticalScrollMode(ScrollMode::Enabled);
            if (_previewUpdate != update || !update->Pending())
                return AwaitPreview(std::move(update));
            if (_previewScroll)
                _previewScroll.VerticalScrollBarVisibility(ScrollBarVisibility::Visible);
            if (_previewUpdate != update || !update->Pending())
                return AwaitPreview(std::move(update));
        }
        if (!IsPreviewLoaded() || !CaptureViewportGeometry())
        {
            if (_viewportTemplateState == ViewportTemplateState::Unsupported)
                update->Complete(Markdown::DocumentUpdateStatus::Failed, E_INVALIDARG);
            else
                update->CompleteViewportWait();
        }
        else
        {
            BeginPreviewSource(update);
        }
        return AwaitPreview(std::move(update));
    }

    IAsyncOperation<Markdown::DocumentUpdateResult> MarkdownView::AwaitPreview(std::shared_ptr<PreviewUpdate> update)
    {
        auto owner = get_weak();
        auto dispatcher = _owningDispatcher;
        auto cancellation = co_await get_cancellation_token();
        cancellation.callback(
            [weak = std::weak_ptr(update)]
            {
                if (auto current = weak.lock())
                    current->Cancel();
            });
        // Arm the completion wait before exposing the cancellable operation.
        // The callback signals it; a later cancellable await must not bypass
        // terminal cleanup, even when cancellation loses to an earlier failure.
        co_await resume_on_signal(update->completed.get());
        (void)update->state.load(std::memory_order_acquire);
        if (update->status != Markdown::DocumentUpdateStatus::Ready)
        {
            try
            {
                // Terminal updates can finish without a Frame callback. A
                // newer update owns its transitions and mounted children.
                (void)dispatcher.TryRunAsync(Windows::UI::Core::CoreDispatcherPriority::Normal, [owner, update]
                {
                    if (auto control = owner.get(); control && control->_previewUpdate == update &&
                        control->_previewSourceGeneration == update->sourceGeneration)
                    {
                        if (control->_images)
                            control->_images->DiscardTransition();
                        control->_derivedEditReuse.reset();
                        if (update->incrementalEdit)
                            control->ClearNoteEmbeds();
                    }
                });
            }
            catch (...) { /* A closed dispatcher is retired by control teardown. */ }
        }
        co_await resume_background();
        co_return make<implementation::DocumentUpdateResult>(update->document, update->revision, update->generation,
                                                             update->status, update->error, update->published,
                                                             update->blocks);
    }

    void MarkdownView::BeginPreviewSource(std::shared_ptr<PreviewUpdate> const &update)
    {
        if (_previewSubmittedSourceGeneration == update->sourceGeneration)
            return;
        if (_previewDrain)
        {
            if (_previewDrain->Wait(update))
                return;
            if (!update->Pending())
                return;
            if (!_previewDrain->terminal)
            {
                _editChannelNeedsResync = true;
                _previewSubmittedSourceGeneration = update->sourceGeneration;
                update->Complete(Markdown::DocumentUpdateStatus::Failed, NOTEPADS_MARKDOWN_CORE_INVALID_STATE);
                _renderingError = L"The previous Markdown controller did not prove a completed drain.";
                return;
            }
            _previewDrain.reset();
        }
        // Controller lifetime and source submission are independent: a positive
        // viewport can arrive after a newer source was accepted as NotLoaded.
        // Count failed attempts too, so layout events cannot retry a bad source.
        _previewSubmittedSourceGeneration = update->sourceGeneration;
        try
        {
            if (_isEmbeddedChild && (!_embedBudgetParent || !_noteParentView.get()))
                throw ::Notepads::Markdown::Native::PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_STATE, L"detached note embed"};
            if (!_previewController)
            {
                // Allocate completion storage before creating any driver. Stop,
                // final_release and destruction reuse it without new allocation.
                if (!_previewDrainStorage)
                    _previewDrainStorage = std::make_shared<PreviewDrain>();
                if (_previewDrainStorage->serial == UINT64_MAX)
                    throw hresult_error(E_UNEXPECTED);
                _previewSourceState = {};
                using ::Notepads::Markdown::Native::CoreController;
                _previewController = _embedBudgetParent
                    ? CoreController::CreateEmbeddedPreview(Dispatcher(), _embedBudgetParent)
                    : CoreController::CreatePreview(Dispatcher());
            }
            notepads_markdown_core_snapshot descriptor{};
            descriptor.header = {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(descriptor)};
            memcpy(descriptor.source.document, &_acceptedDocument, 16);
            descriptor.source.revision = _acceptedRevision;
            descriptor.profile = NOTEPADS_MARKDOWN_CORE_G3;
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            if (!_isEmbeddedChild)
                ::Notepads::Markdown::Native::StartupTrace::Mark(::Notepads::Markdown::Native::StartupTrace::Phase::Submit);
#endif
            auto result = _previewController->SubmitUtf8(
                *_acceptedSource, descriptor, &_previewTicket, static_cast<uint32_t>(_acceptedExtensions));
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            if (!_isEmbeddedChild)
                ::Notepads::Markdown::Native::StartupTrace::Mark(::Notepads::Markdown::Native::StartupTrace::Phase::Submitted);
#endif
            RecordPreviewSourceState(result);
            bool accepted = result.sourceState.state_valid && result.sourceState.has_source &&
                            result.sourceState.source_accepted &&
                            result.sourceState.source.revision == update->revision &&
                            std::memcmp(result.sourceState.source.document, &update->document, 16) == 0;
            if (update->fullSynchronization && accepted && !result.sourceState.needs_resync)
                _editChannelNeedsResync = false;
            else if (!accepted)
                _editChannelNeedsResync = true;
            update->BindParse(_previewTicket);
            if (result.status != NOTEPADS_MARKDOWN_CORE_OK)
            {
                update->Complete(PreviewStatus(result.status), result.status);
                _renderingError = L"Markdown source submit failed (" + to_hstring(result.status) + L").";
                _lastStatus = Markdown::RenderingStatus::InvalidState;
                return;
            }
            ObservePreviewParse(get_weak(), Dispatcher(), _previewTicket, update);
        }
        catch (::Notepads::Markdown::Native::ReadingFailure const &failure)
        {
            _editChannelNeedsResync = true;
            if (_previewUpdate)
                _previewUpdate->Complete(PreviewStatus(failure.status), failure.status);
        }
        catch (::Notepads::Markdown::Native::PreviewFailure const &failure)
        {
            _editChannelNeedsResync = true;
            update->Complete(PreviewStatus(failure.status), failure.status);
        }
        catch (...)
        {
            _editChannelNeedsResync = true;
            _renderingError = hresult_error(to_hresult()).message();
            update->Complete(Markdown::DocumentUpdateStatus::Failed, static_cast<int32_t>(to_hresult()));
        }
    }

    fire_and_forget MarkdownView::ObservePreviewParse(weak_ref<MarkdownView> owner,
                                                      Windows::UI::Core::CoreDispatcher dispatcher,
                                                      ::Notepads::Markdown::Native::CoreCompletionTicket ticket,
                                                      std::shared_ptr<PreviewUpdate> update)
    {
        ::Notepads::Markdown::Native::ModuleLifetime module;
        try
        {
            co_await resume_background();
            co_await resume_on_signal(ticket.EventHandle());
            auto dispatched = co_await resume_agile(
                dispatcher.TryRunAsync(Windows::UI::Core::CoreDispatcherPriority::Normal,
                                       [owner, ticket, update]
                                       {
                                           if (auto control = owner.get())
                                               control->PreviewParsed(ticket, update);
                                           else
                                               update->Complete(Markdown::DocumentUpdateStatus::NotLoaded);
                                       }));
            if (!dispatched)
                update->Complete(Markdown::DocumentUpdateStatus::NotLoaded);
        }
        catch (...)
        {
            update->Complete(Markdown::DocumentUpdateStatus::Failed, static_cast<int32_t>(to_hresult()));
        }
    }

    void MarkdownView::PreviewParsed(::Notepads::Markdown::Native::CoreCompletionTicket const &ticket,
                                     std::shared_ptr<PreviewUpdate> const &update)
    {
        if (!_loaded || !_documentAccepted || update != _previewUpdate ||
            update->sourceGeneration != _previewSourceGeneration)
            return;
        const auto receiptState = update->state.load(std::memory_order_acquire);
        // Geometry may finish this receipt while the owning parse is queued.
        // Its successful current model still belongs to the accepted source;
        // StartPreviewLayout creates a new receipt without reopening this one.
        if (receiptState != 0 && !(receiptState == 3 && update->status == Markdown::DocumentUpdateStatus::NotLoaded &&
                                  ticket.Status() == NOTEPADS_MARKDOWN_CORE_OK))
            return;
        if (ticket.Status() != NOTEPADS_MARKDOWN_CORE_OK)
        {
            if (_images && _images->transitionDescriptors)
                _images->DiscardTransition();
            _derivedEditReuse.reset();
            update->Complete(PreviewStatus(ticket.Status()), ticket.Status());
            _renderingError = L"Markdown parse failed (" + to_hstring(ticket.Status()) + L").";
            return;
        }
        if (!_previewController || _previewController->Snapshot().lastInstalled != ticket.Serial())
        {
            if (receiptState == 0)
            {
                _derivedEditReuse.reset();
                update->Complete(Markdown::DocumentUpdateStatus::Superseded);
            }
            return;
        }
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        if (!_isEmbeddedChild)
            ::Notepads::Markdown::Native::StartupTrace::Mark(::Notepads::Markdown::Native::StartupTrace::Phase::ParseObserved);
#endif
        _previewModelReady = true;
        StartPreviewLayout();
    }

    void MarkdownView::CancelViewportRendering() noexcept
    {
        _viewportRendering.revoke();
    }

    void MarkdownView::InvalidateViewportTemplate(bool rediscover) noexcept
    {
        _previewRasterBand.reset();
        SetReadingLinkCursor(false);
        ClearReadingHover();
        CancelWheelScroll();
        CancelViewportRendering();
        _viewportDesired.reset();
        _viewportFailed.reset();
        _viewportActive.reset();
        _viewportPresenter = nullptr;
        _viewportRediscover = rediscover;
        _viewportRecovery = rediscover;
        _viewportTemplateState = ViewportTemplateState::Waiting;
        ++_viewportGeometryEpoch;
        _previewToken->fetch_add(1, std::memory_order_acq_rel);
        if (_previewCancel)
            _previewCancel->Cancel();
        _previewRenderer.reset();
        _presented = {};
        _surfaceReady = false;
        HideCodeCopyButton();
        try
        {
            if (_surfaceBrush)
                _surfaceBrush.ImageSource(nullptr);
        }
        catch (...)
        {
        }
    }

    void MarkdownView::DetachViewportTemplate() noexcept
    {
        DetachWheelScrollEvents();
        CancelViewportRendering();
        _viewportRootChanged.revoke();
        _viewportRoot = nullptr;
        _viewportScrollLoaded.revoke();
        _viewportScrollUnloaded.revoke();
        if (_viewportCallbacks && _previewScroll)
        {
            try
            {
                _previewScroll.UnregisterPropertyChangedCallback(Control::TemplateProperty(),
                                                                 _viewportTemplateCallback);
                _previewScroll.UnregisterPropertyChangedCallback(ContentControl::ContentProperty(),
                                                                 _viewportContentCallback);
            }
            catch (...)
            {
            }
        }
        _viewportCallbacks = false;
        _viewportPresenter = nullptr;
        _viewportVisualRoot = nullptr;
        _viewportAwaitingVisualRoot = nullptr;
        _viewportInnerTemplate = nullptr;
        _viewportRediscover = true;
    }

    void MarkdownView::AttachViewportTemplate()
    {
        if (!_previewScroll || _viewportCallbacks)
            return;
        auto invalidate = [weak = get_weak()](auto const &, auto const &)
        {
            if (auto owner = weak.get())
                owner->InvalidateViewportTemplate();
        };
        _viewportTemplateCallback = _previewScroll.RegisterPropertyChangedCallback(
            Control::TemplateProperty(),
            [weak = get_weak()](auto const &, auto const &)
            {
                if (auto owner = weak.get())
                {
                    // The callback can run after XAML has already
                    // installed the new visual root. Only a root
                    // previously discovered under another template
                    // is an old tree that must be retired.
                    owner->_viewportAwaitingVisualRoot =
                        owner->_viewportPresenter && owner->_viewportInnerTemplate != owner->_previewScroll.Template()
                            ? owner->_viewportVisualRoot
                            : nullptr;
                    owner->InvalidateViewportTemplate();
                }
            });
        _viewportContentCallback =
            _previewScroll.RegisterPropertyChangedCallback(ContentControl::ContentProperty(), invalidate);
        _viewportCallbacks = true;
        _viewportScrollLoaded = _previewScroll.Loaded(auto_revoke,
                                                      [weak = get_weak()](auto const &sender, auto const &)
                                                      {
                                                          if (auto owner = weak.get())
                                                          {
                                                              if (sender != owner->_previewScroll)
                                                                  return;
                                                              owner->InvalidateViewportTemplate();
                                                              // Loaded can follow the final LayoutUpdated of a
                                                              // replacement tree. Resume from this actual event
                                                              // instead of waiting for a layout that need not occur.
                                                              owner->OnPreviewLayoutUpdated(nullptr, nullptr);
                                                          }
                                                      });
        _viewportScrollUnloaded = _previewScroll.Unloaded(auto_revoke, invalidate);
    }

    void MarkdownView::FailViewportTemplate(wchar_t const *reason)
    {
        if (_viewportTemplateState != ViewportTemplateState::Unsupported)
        {
            InvalidateViewportTemplate(false);
            _viewportTemplateState = ViewportTemplateState::Unsupported;
            _renderingError = hstring{L"Markdown template unsupported: "} + reason;
            _lastStatus = Markdown::RenderingStatus::InvalidState;
        }
        if (_previewUpdate)
            _previewUpdate->Complete(Markdown::DocumentUpdateStatus::Failed, E_INVALIDARG);
    }

    double MarkdownView::EffectiveRasterScale() const
    {
        auto rootScale = std::numeric_limits<double>::quiet_NaN();
        if (HasXamlRoot())
            if (auto root = XamlRoot())
                rootScale = root.RasterizationScale();
        auto valid = [](double scale) { return std::isfinite(scale) && scale > 0; };
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        if (_imageFontControlTests && _imageFontControlTests->rasterScaleInputs)
        {
            auto const [root, display] = *_imageFontControlTests->rasterScaleInputs;
            return valid(root) ? root : valid(display) ? display : 1.0;
        }
#endif
        if (valid(rootScale))
            return rootScale;
        const auto displayScale = _displayInfo ? _displayInfo.RawPixelsPerViewPixel() : 1.0;
        return valid(displayScale) ? displayScale : 1.0;
    }

    bool MarkdownView::CaptureViewportGeometry()
    {
        if (!_loaded || !IsLoaded())
            return false;
        try
        {
            auto root = VisualRoot(_previewScroll);
            if (_viewportTemplateState == ViewportTemplateState::Unsupported && !_viewportRediscover &&
                root == _viewportVisualRoot && (!_previewScroll || _previewScroll.Template() == _viewportInnerTemplate))
                return false;
            if (!_previewScroll || !_previewExtent || !_viewportOverlay || !_surfaceTarget)
            {
                FailViewportTemplate(
                    L"PreviewScrollViewer/DocumentExtent/ViewportOverlay/SurfaceTarget is missing or has the wrong type.");
                return false;
            }
            _viewportVisualRoot = root;
            if (_viewportAwaitingVisualRoot && root == _viewportAwaitingVisualRoot)
            {
                _viewportTemplateState = ViewportTemplateState::Waiting;
                if (_previewUpdate)
                    _previewUpdate->CompleteViewportWait();
                return false;
            }
            _viewportAwaitingVisualRoot = nullptr;
            if (_previewScroll.Content() != _previewExtent ||
                VisualTreeHelper::GetParent(_surfaceTarget) != _viewportOverlay ||
                Within(_viewportOverlay, _previewScroll) || Within(_surfaceTarget, _previewExtent) ||
                !Within(_previewScroll, *this) || !Within(_viewportOverlay, *this))
            {
                FailViewportTemplate(L"The extent or non-scrolling overlay has an invalid parent.");
                return false;
            }
            if (!_previewScroll.IsLoaded() || !root)
            {
                _viewportTemplateState = ViewportTemplateState::Waiting;
                if (_previewUpdate)
                    _previewUpdate->CompleteViewportWait();
                return false;
            }
            auto rootElement = root.try_as<FrameworkElement>();
            if (_previewScroll.ActualWidth() <= 0 || _previewScroll.ActualHeight() <= 0 ||
                (rootElement && (rootElement.ActualWidth() <= 0 || rootElement.ActualHeight() <= 0)))
            {
                // A newly applied template can have a visual root before its
                // content bindings and first arrange are complete. A natural
                // Loaded/LayoutUpdated event retries discovery; no self-polling.
                _viewportTemplateState = ViewportTemplateState::Waiting;
                if (_previewUpdate)
                    _previewUpdate->CompleteViewportWait();
                return false;
            }
            auto currentTemplate = _previewScroll.Template();
            if (_viewportInnerTemplate != currentTemplate ||
                (_viewportPresenter &&
                 (!Within(_viewportPresenter, root) || !Within(_previewExtent, _viewportPresenter))))
            {
                InvalidateViewportTemplate();
            }
            _viewportInnerTemplate = currentTemplate;
            _viewportVisualRoot = root;
            if (!_viewportPresenter)
            {
                std::vector<DependencyObject> pending{root};
                ScrollContentPresenter found{nullptr};
                while (!pending.empty())
                {
                    auto item = std::move(pending.back());
                    pending.pop_back();
                    if (auto presenter = item.try_as<ScrollContentPresenter>();
                        presenter && presenter.Content() == _previewExtent)
                    {
                        auto parent = VisualTreeHelper::GetParent(presenter);
                        while (parent && !parent.try_as<ScrollViewer>())
                            parent = VisualTreeHelper::GetParent(parent);
                        if (parent == _previewScroll)
                        {
                            if (found)
                            {
                                FailViewportTemplate(L"More than one current ScrollContentPresenter owns the extent.");
                                return false;
                            }
                            found = presenter;
                        }
                    }
                    auto count = VisualTreeHelper::GetChildrenCount(item);
                    for (int index = 0; index < count; ++index)
                        pending.push_back(VisualTreeHelper::GetChild(item, index));
                }
                if (!found)
                {
                    FailViewportTemplate(L"The applied ScrollViewer template has no content presenter for the extent.");
                    return false;
                }
                _viewportPresenter = found;
            }
            _viewportRediscover = false;
            auto width = _previewScroll.ViewportWidth();
            auto height = _previewScroll.ViewportHeight();
            if (width <= 0 || height <= 0 || _viewportPresenter.ActualWidth() <= 0 ||
                _viewportPresenter.ActualHeight() <= 0)
            {
                _viewportTemplateState = ViewportTemplateState::Waiting;
                if (_previewUpdate)
                    _previewUpdate->CompleteViewportWait();
                return false;
            }
            const auto scale = EffectiveRasterScale();
            if (HasXamlRoot())
            {
                auto xamlRoot = XamlRoot();
                if (xamlRoot)
                {
                    if (_viewportRoot != xamlRoot)
                    {
                        _viewportRootChanged.revoke();
                        _viewportRoot = xamlRoot;
                        _viewportRootChanged =
                            xamlRoot.Changed(auto_revoke,
                                             [weak = get_weak()](auto const &, auto const &)
                                             {
                                                 if (auto owner = weak.get())
                                                 {
                                                     auto previousScale = owner->_viewportGeometry.scale;
                                                     if (!owner->CaptureViewportGeometry())
                                                         return;
                                                     if (owner->_viewportGeometry.scale != previousScale)
                                                         owner->OnDpiChanged(nullptr, nullptr);
                                                     else
                                                         owner->OnPreviewLayoutUpdated(nullptr, nullptr);
                                                 }
                                             });
                    }
                }
            }
            auto toControl = _viewportPresenter.TransformToVisual(*this);
            auto at = toControl.TransformPoint({0, 0});
            auto toOverlay = _viewportPresenter.TransformToVisual(_viewportOverlay);
            auto overlayAt = toOverlay.TransformPoint({0, 0});
            auto toRoot = _viewportPresenter.TransformToVisual(nullptr);
            auto rootAt = toRoot.TransformPoint({0, 0});
            auto rootX = toRoot.TransformPoint({1, 0});
            auto rootY = toRoot.TransformPoint({0, 1});
            bool unit = std::abs(rootX.X - rootAt.X - 1) < 0.0001 && std::abs(rootX.Y - rootAt.Y) < 0.0001 &&
                        std::abs(rootY.X - rootAt.X) < 0.0001 && std::abs(rootY.Y - rootAt.Y - 1) < 0.0001;
            ::Notepads::Markdown::Native::ViewportGeometry next;
            if (!::Notepads::Markdown::Native::MakeViewportGeometry(at.X, at.Y, width, height, scale, rootAt.X, rootAt.Y,
                                                             overlayAt.X, overlayAt.Y, unit, next))
                throw hresult_invalid_argument(L"The native preview viewport exceeds the graphics surface limit.");
            const bool geometryChanged = !_viewportGeometry.Same(next);
            const bool originChanged = rootAt.X != _readingLinkViewportOrigin.X ||
                                       rootAt.Y != _readingLinkViewportOrigin.Y;
            _readingLinkViewportOrigin = rootAt;
            if (geometryChanged && (_viewportGeometry.width != next.width || _viewportGeometry.height != next.height ||
                                    _viewportGeometry.scale != next.scale))
                CaptureImageAnchorBeforeChange();
            if (geometryChanged)
                next.epoch = ++_viewportGeometryEpoch;
            else
                next.epoch = _viewportGeometry.epoch;
            _viewportGeometry = next;
            _viewportTemplateState = ViewportTemplateState::Ready;
            if (geometryChanged)
            {
                ClearInitialViewport();
                // Publish geometry first, so a synchronous ViewChanged sees
                // this same geometry rather than recursively canceling it.
                auto intent = BeginImmediateScroll();
                if (!CurrentScrollIntent(intent))
                    return false;
            }
            else if (originChanged && _readingLinkPoint)
            {
                if (_readingHover && _readingHover->kind != ReadingHoverTarget::Kind::Fallback)
                    ClearReadingHover();
                UpdateReadingLinkCursor();
            }
            return true;
        }
        catch (...)
        {
            CancelWheelScroll();
            _surfaceReady = false;
            _renderingError = hresult_error(to_hresult()).message();
            if (_previewUpdate)
                _previewUpdate->Complete(Markdown::DocumentUpdateStatus::Failed, static_cast<int32_t>(to_hresult()));
            return false;
        }
    }

    Point MarkdownView::ControlToViewport(Point point) const noexcept
    {
        return {point.X - static_cast<float>(_presented.geometry.x),
                point.Y - static_cast<float>(_presented.geometry.y)};
    }

    bool MarkdownView::CanScheduleViewportFrame() noexcept
    {
        if (_previewUpdate && _previewUpdate->AllowsViewportFrame())
            return true;
        // A later Frame cannot publish through this receipt. Keep the active
        // request owned until its completion, but discard pending UI work.
        CancelViewportRendering();
        CancelWheelScroll();
        _viewportDesired.reset();
        return false;
    }

    void MarkdownView::ScheduleViewportRendering()
    {
        if (!_loaded || !_documentAccepted || !_viewportDesired || _viewportActive || _viewportRendering)
            return;
        _viewportRendering = CompositionTarget::Rendering(auto_revoke,
                                                          [weak = get_weak()](auto const &, auto const &)
                                                          {
                                                              if (auto owner = weak.get())
                                                              {
                                                                  owner->CancelViewportRendering();
                                                                  owner->DispatchViewportFrame();
                                                              }
                                                          });
    }

    void MarkdownView::FinishViewportRequest(std::shared_ptr<PreviewRequest> const &request) noexcept
    {
        if (_viewportActive != request)
            return;
        _viewportActive.reset();
        try
        {
            auto completeFrame = request->frame && request->frame->main && request->frame->descriptors &&
                                 request->frame->navigation;
            auto sameLayout = request->sourceGeneration == _previewSourceGeneration &&
                              request->key.layout_generation == _previewLayoutGeneration &&
                              request->token == _previewToken->load(std::memory_order_acquire) &&
                              request->update == _previewUpdate;
            auto geometryChanged = request->geometry.epoch != _viewportGeometry.epoch;
            if (_initialViewportToken == request->token &&
                (geometryChanged || request->status != NOTEPADS_MARKDOWN_CORE_OK ||
                 (request->update && !request->update->AllowsViewportFrame())))
                ClearInitialViewport();
            auto current = sameLayout && !geometryChanged &&
                           (!completeFrame || CurrentViewportFrame(*request->frame, request->geometry));
            if (current && request->publication == PreviewRequest::UiPublication::DrawFailed)
            {
                CancelWheelScroll();
                int64_t row{};
                double y{};
                if (::Notepads::Markdown::Native::QuantizeViewportY(request->y, request->geometry.scale, row, y))
                {
                    _viewportFailed = ::Notepads::Markdown::Native::ViewportTarget{
                        request->geometry, row, y, request->viewportSerial};
                    _viewportFailedToken = request->token;
                }
            }
            else if (current && request->publication == PreviewRequest::UiPublication::Published)
                _viewportFailed.reset();
            // A different latest target is already new input. The target guard
            // suppresses only the failed one, preserving that newer intent.
            if (CanScheduleViewportFrame() && _loaded && _documentAccepted && request->status == NOTEPADS_MARKDOWN_CORE_OK)
            {
                // The queued image refresh owns this generation change.
                // Retrying the same target with the same table cannot publish.
                if (!current && sameLayout && !geometryChanged && completeFrame && !request->reading &&
                    request->publication == PreviewRequest::UiPublication::Stale && _previewRenderer &&
                    _viewportDesired && _viewportDesired->serial == request->viewportSerial &&
                    _imageRefreshTimer && _imageRefreshTimer.IsEnabled() && _images && _images->bound &&
                    _imagePresentation && request->frame->images == _imagePresentation)
                {
                    try
                    {
                        if (_images->policy->StateGeneration() != _imagePresentation->policy.header.state_generation)
                            return;
                    }
                    catch (...) { /* Leave retry handling to normal publication. */ }
                }
                // A geometry change may overtake the first layout. Its stale
                // frame cannot publish, and there is no installed renderer yet.
                if (sameLayout && geometryChanged && request->reading && !_previewRenderer &&
                    request->publication == PreviewRequest::UiPublication::Stale)
                    StartPreviewLayout();
                else
                    StartPreviewFrame();
            }
        }
        catch (...)
        {
        }
    }

    bool MarkdownView::FailedViewportTarget(::Notepads::Markdown::Native::ViewportTarget const &target) const noexcept
    {
        return _viewportFailed && _viewportFailedToken == _previewToken->load(std::memory_order_acquire) &&
               _viewportFailed->SameTarget(target);
    }

    bool MarkdownView::HasActivePreviewLayout() const
    {
        return _viewportActive && _viewportActive->reading &&
            !_viewportActive->deliveryFailed.load(std::memory_order_acquire) &&
            _viewportActive->update == _previewUpdate && _previewUpdate && _previewUpdate->AllowsViewportFrame() &&
            _viewportActive->token == _previewToken->load(std::memory_order_acquire) &&
            _viewportActive->sourceGeneration == _previewSourceGeneration &&
            _viewportActive->disclosureOverrides == _disclosureOverrides &&
            _viewportActive->noteEmbedExtents == _noteEmbedExtents &&
            _viewportActive->key.theme_generation == _effectiveThemeGeneration &&
            std::abs(_viewportActive->key.width - (_viewportGeometry.width - 2 * _padding)) <= 0.25 &&
            _viewportActive->geometry.scale == _viewportGeometry.scale;
    }

    void MarkdownView::StartPreviewLayout()
    {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        namespace UiTrace = ::Notepads::Markdown::Native::StartupTrace;
        UiTrace::UiScope uiTrace{UiTrace::Phase::UiStartLayout, UiTrace::Phase::UiStartLayoutEnd, _previewLayoutGeneration};
#endif
        if (!_documentAccepted || !_loaded || !_previewModelReady || (!_previewController && !_cachedPreviewModel))
            return;
        CaptureImageAnchorBeforeChange();
        if (!CaptureViewportGeometry())
        {
            SetReadingLinkCursor(false);
            return;
        }
        if (_viewportActive && _viewportActive->deliveryFailed.load(std::memory_order_acquire))
            _viewportActive.reset();
        auto width = _viewportGeometry.width;
        auto height = _viewportGeometry.height;
        // Only a renderer-creation request owns these layout/resource inputs.
        // A viewport request reuses an immutable renderer and cannot satisfy a
        // refresh merely by observing newer resource generations on the UI thread.
        if (HasActivePreviewLayout() &&
            (_viewportActive->images == _imagePresentation || (_viewportActive->images && _imagePresentation &&
                _viewportActive->images->geometryGeneration == _imagePresentation->geometryGeneration)) &&
            _viewportActive->math.generation == _mathGeneration &&
            _viewportActive->diagram.generation == _diagramGeneration &&
            _viewportActive->math.attempts == CaptureMath().attempts &&
            _viewportActive->diagram.attempts == CaptureDiagram().attempts)
            return;
        auto scrollIntent = BeginImmediateScroll();
        if (!CurrentScrollIntent(scrollIntent))
            return;
        if (width <= 2 * _padding || height <= 0)
        {
            SetReadingLinkCursor(false);
            if (_previewUpdate)
                _previewUpdate->Complete(Markdown::DocumentUpdateStatus::NotLoaded);
            return;
        }
        // A NotLoaded public receipt remains terminal. When a real viewport
        // arrives, redraw the retained source with an independent internal receipt.
        if (_previewUpdate && _previewUpdate->state.load(std::memory_order_acquire) == 3 &&
            (_previewUpdate->status == Markdown::DocumentUpdateStatus::NotLoaded ||
             (_viewportRecovery && _previewUpdate->status == Markdown::DocumentUpdateStatus::Failed)))
        {
            auto update = std::make_shared<PreviewUpdate>();
            update->surfaceRecovery = _previewUpdate->status == Markdown::DocumentUpdateStatus::Failed;
            update->document = _acceptedDocument;
            update->revision = _acceptedRevision;
            update->generation = _generation;
            update->sourceGeneration = _previewSourceGeneration;
            _previewUpdate = std::move(update);
        }
        _viewportRecovery = false;
        std::shared_ptr<PreviewRequest> request;
        bool observed{};
        struct SubmissionRollback
        {
            MarkdownView *owner;
            std::shared_ptr<PreviewRequest> &request;
            bool &observed;
            ~SubmissionRollback() noexcept
            {
                if (observed || !request)
                    return;
                request->deliveryFailed.store(true, std::memory_order_release);
                if (request->cancel)
                    request->cancel->Cancel();
                if (owner->_viewportActive == request)
                    owner->_viewportActive.reset();
            }
        } rollback{this, request, observed};
        try
        {
            InvalidateVerticalNavigation();
            ClearInitialViewport();
            if (_previewCancel)
                _previewCancel->Cancel();
            _previewRenderer.reset();
            request = std::make_shared<PreviewRequest>();
            auto pinModel = [this]
            {
                return _cachedPreviewModel ? _cachedPreviewModel.Clone() : _previewController->PinInstalled();
            };
            request->model = pinModel();
            request->measurementCache = EnsureMeasurementCache();
            if (!request->model)
                throw ::Notepads::Markdown::Native::PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
            notepads_markdown_core_model_summary_result summary{};
            summary.header = {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(summary)};
            notepads_markdown_core_error error{};
            error.header = {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(error)};
            auto status = notepads_markdown_core_model_summary(request->model.Get(), &summary, &error);
            if (status != NOTEPADS_MARKDOWN_CORE_OK)
                throw ::Notepads::Markdown::Native::PreviewFailure{status};
            if (!_readingSource)
            {
                bool deferHighlights = true;
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                namespace Trace = ::Notepads::Markdown::Native::StartupTrace;
                _previewViewportHighlights = !Trace::fullHighlightBaseline.load();
                deferHighlights = (_previewViewportHighlights ||
                                   (!_isEmbeddedChild && _previewUpdate && _previewUpdate->fullSynchronization)) &&
                                  !Trace::synchronousHighlights.load();
#endif
                _readingSource = ::Notepads::Markdown::Native::ReadingSourceState::Create(
                    pinModel(), _previewSourceGeneration, deferHighlights);
            }
            BindImages();
            // Retained resources need exact descriptors for the new model,
            // not a throwaway full-document measurement to discover them.
            request->prepareEditResources = _previewUpdate && _previewUpdate->Pending() &&
                !_previewUpdate->surfaceRecovery && !_readingSource->Descriptors() &&
                _previewEditPreparationSourceGeneration != _previewSourceGeneration &&
                (_noteEmbedsPending || _derivedEditReuse || (_images && _images->transitionDescriptors));
            request->images = _imagePresentation;
            request->reading = _readingSource;
            request->disclosureOverrides = _disclosureOverrides;
            request->noteEmbedExtents = _noteEmbedExtents;
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            // A viewport/theme replacement can retire the first layout while
            // it is held. Fence every renderer for this test source so no
            // successful replacement can publish a Frame past the second gate.
            if (_imageFontControlTests && _imageFontControlTests->gateReading)
                request->readingPublicationGate = _imageFontControlTests->readingPublication;
#endif
            InvalidateDiagramConfiguration();
            if ((static_cast<uint32_t>(_acceptedExtensions) & 3u) != 0)
                EnsureGraphicsFactory();
            request->math = CaptureMath();
            request->diagram = CaptureDiagram();
            if (_previewUpdate && _previewUpdate->Pending())
                _previewUpdate->BindSourceCancel(_readingSource->cancel);
            request->cancel = std::make_shared<::Notepads::Markdown::Native::PreviewCancellation>(request->model.Get());
            _previewCancel = request->cancel;
            if (_previewUpdate && _previewUpdate->Pending())
                _previewUpdate->BindCancel(request->cancel);
            request->key.header = {NOTEPADS_MARKDOWN_PREVIEW_ABI_VERSION, sizeof(request->key)};
            request->key.model = summary.identity;
            request->key.layout_generation = ++_previewLayoutGeneration;
            request->update = _previewUpdate;
            if (_previewUpdate)
                _previewUpdate->layoutGeneration.store(_previewLayoutGeneration, std::memory_order_release);
            request->key.theme_generation = _effectiveThemeGeneration;
            request->key.measurement_generation = _previewLayoutGeneration;
            request->key.width = width - 2 * _padding;
            request->key.indent = 28.0;
            request->key.gap = 12.0;
            request->theme = _readingTheme;
            request->sectionTitle = _noteSectionTitle;
            request->theme.physicalScale = _viewportGeometry.scale;
            request->theme.highContrast = _accessibility && _accessibility.HighContrast();
            if (_accessibility && _accessibility.HighContrast() && _systemColors)
            {
                using Element = Windows::UI::ViewManagement::UIElementType;
                request->theme.foreground = _systemColors.UIElementColor(Element::WindowText);
                request->theme.background = _systemColors.UIElementColor(Element::Window);
                request->theme.linkForeground = _systemColors.UIElementColor(Element::Hotlight);
                request->theme.selectionBackground = _systemColors.UIElementColor(Element::Highlight);
                request->theme.selectionForeground = _systemColors.UIElementColor(Element::HighlightText);
                request->theme.borderColor = request->theme.focusColor = request->theme.foreground;
                request->theme.panelBackground = request->theme.background;
                request->theme.codeForeground.fill(request->theme.foreground);
            }
            request->latestToken = _previewToken;
            request->token = _previewToken->fetch_add(1, std::memory_order_acq_rel) + 1;
            _viewportFailed.reset();
            request->sourceGeneration = _previewSourceGeneration;
            _previewScheduledToken = request->token;
            request->height = height;
            request->geometry = _viewportGeometry;
            int64_t pixelRow{};
            if (!::Notepads::Markdown::Native::QuantizeViewportY(_previewScroll.VerticalOffset(), _viewportGeometry.scale,
                                                          pixelRow, request->y))
                throw hresult_invalid_argument();
            request->viewportSerial = ++_viewportSerial;
            if (!request->prepareEditResources && pixelRow == 0 && !_surfaceReady && !_presented.frame && !_viewportToRestore &&
                !_imageRestoringY && !_resourceRenderingDeferred && request->update && request->update->Pending() &&
                !request->update->surfaceRecovery)
            {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                const bool retainPrefixForFault = _failNextPreviewDrawOnInitialViewport.value_or(false);
#endif
                request->initialViewport.emplace(
                    ::Notepads::Markdown::Native::NativeDocumentRenderer::InitialViewport{
                        height, request->token, *_previewToken,
                        [owner = get_weak(), dispatcher = Dispatcher(), pending = std::weak_ptr{request}
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                         , retainPrefixForFault
#endif
                        ]
                        (std::shared_ptr<::Notepads::Markdown::Native::PreviewFrame> frame)
                        {
                            try
                            {
                                if (auto request = pending.lock())
                                    (void)dispatcher.TryRunAsync(Windows::UI::Core::CoreDispatcherPriority::Normal,
                                        [owner, request, pendingFrame = std::weak_ptr{frame}
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                                         , retained = retainPrefixForFault ? frame : nullptr
#endif
                                        ]
                                        {
                                            if (auto self = owner.get())
                                            {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                                                // Guarantee the test sees the speculative paint even when
                                                // final measurement finishes before the UI callback.
                                                if (retained)
                                                    return self->InstallInitialViewport(request, retained);
#endif
                                                if (auto frame = pendingFrame.lock())
                                                    self->InstallInitialViewport(request, frame);
                                            }
                                        });
                            }
                            catch (...) { /* Complete publication still runs. */ }
                        }});
            }
            _viewportDesired =
                ::Notepads::Markdown::Native::ViewportTarget{_viewportGeometry, pixelRow, request->y, request->viewportSerial};
            _previewRequestedWidth = request->key.width;
            _previewRequestedHeight = height;
            _previewRequestedY = request->y;
            if (!_previewQueue)
                _previewQueue = std::make_shared<::Notepads::Markdown::Native::PreviewRenderQueue>();
            _viewportActive = request;
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            namespace Trace = ::Notepads::Markdown::Native::StartupTrace;
            request->startupTrace = _isEmbeddedChild ? 0 : Trace::Capture();
            Trace::MarkFor(request->startupTrace, Trace::Phase::LayoutEnqueued, request->key.layout_generation);
#endif
            _previewQueue->Enqueue(request);
            // This fresh request already captures the resource batch that was
            // waiting for its deadline. A later difference starts its own.
            _offscreenDerivedPendingAt.reset();
            if (request->prepareEditResources)
                _previewEditPreparationSourceGeneration = request->sourceGeneration;
            ObservePreviewFrame(get_weak(), Dispatcher(), request);
            observed = true;
            UpdateReadingLinkCursor();
            UpdateCodeCopyButton();
        }
        catch (::Notepads::Markdown::Native::ReadingFailure const &failure)
        {
            _derivedEditReuse.reset();
            _renderingError = L"Reading source preparation failed (" + to_hstring(failure.status) + L").";
            if (_previewUpdate)
                _previewUpdate->Complete(PreviewStatus(failure.status), failure.status);
        }
        catch (::Notepads::Markdown::Native::PreviewFailure const &failure)
        {
            _derivedEditReuse.reset();
            _renderingError =
                L"Markdown " + hstring(failure.stage) + L" preparation failed (" + to_hstring(failure.status) + L").";
            if (_previewUpdate)
                _previewUpdate->Complete(PreviewStatus(failure.status), failure.status);
        }
        catch (...)
        {
            _derivedEditReuse.reset();
            _renderingError = hresult_error(to_hresult()).message();
            if (_previewUpdate)
                _previewUpdate->Complete(Markdown::DocumentUpdateStatus::Failed, static_cast<int32_t>(to_hresult()));
        }
    }

    void MarkdownView::StartPreviewFrame()
    {
        if (!CanScheduleViewportFrame() || !_documentAccepted || !_loaded || !_previewRenderer)
            return;
        try
        {
            // A dropped dispatch has no UI callback to bump resource generation.
            // A normal viewport request must observe its terminal ordinal with
            // a fresh renderer instead of reusing the Pending snapshot.
            if (_presented.frame &&
                (_presented.frame->math.attempts != CaptureMath().attempts ||
                 _presented.frame->diagram.attempts != CaptureDiagram().attempts))
            {
                StartPreviewLayout();
                return;
            }
            auto geometryReady = CaptureViewportGeometry();
            if (!CanScheduleViewportFrame() || !geometryReady || !_previewRenderer)
                return;
            if (_imageRestoringY && *_imageRestoringY > _previewScroll.ScrollableHeight())
                _imageRestoringY = std::max(0.0, _previewScroll.ScrollableHeight());
            if (_imageRestoringY && std::abs(_previewScroll.VerticalOffset() - *_imageRestoringY) > 0.5)
                return;
            if (!::Notepads::Markdown::Native::CoalesceViewportTarget(_viewportDesired, _viewportGeometry,
                                                               _previewScroll.VerticalOffset(), _viewportSerial))
                throw hresult_invalid_argument();
            if (FailedViewportTarget(*_viewportDesired))
                return;
            if (_surfaceReady && _presented.frame && CurrentPreviewGeometry(*_presented.frame) &&
                _presented.frame->viewportY == _viewportDesired->y)
                return;
            if (!::Notepads::Markdown::Native::CoalesceViewportTarget(_viewportDesired, _viewportGeometry,
                                                               _previewScroll.VerticalOffset(), _viewportSerial,
                                                               _presented.serial))
                throw hresult_invalid_argument();
            if (_viewportActive && _viewportActive->deliveryFailed.load(std::memory_order_acquire))
                _viewportActive.reset();
            ScheduleViewportRendering();
        }
        catch (...)
        {
            _renderingError = hresult_error(to_hresult()).message();
            if (_previewUpdate)
                _previewUpdate->Complete(Markdown::DocumentUpdateStatus::Failed, static_cast<int32_t>(to_hresult()));
            (void)CanScheduleViewportFrame();
        }
    }

    void MarkdownView::DispatchViewportFrame()
    {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        namespace UiTrace = ::Notepads::Markdown::Native::StartupTrace;
        UiTrace::UiScope uiTrace{UiTrace::Phase::UiDispatchViewport, UiTrace::Phase::UiDispatchViewportEnd, _previewLayoutGeneration};
#endif
        if (!CanScheduleViewportFrame() || !_documentAccepted || !_loaded || !_previewRenderer || _viewportActive ||
            !_viewportDesired)
            return;
        std::shared_ptr<PreviewRequest> request;
        try
        {
            auto geometryReady = CaptureViewportGeometry();
            if (!CanScheduleViewportFrame() || !geometryReady || !_previewRenderer)
                return;
            if (!::Notepads::Markdown::Native::CoalesceViewportTarget(_viewportDesired, _viewportGeometry,
                                                               _previewScroll.VerticalOffset(), _viewportSerial,
                                                               _presented.serial))
                throw hresult_invalid_argument();
            auto desired = *_viewportDesired;
            if (FailedViewportTarget(desired))
                return;
            std::shared_ptr<::Notepads::Markdown::Native::PreviewFrame> frame;
            if (_surfaceReady && _presented.frame && CurrentPreviewGeometry(*_presented.frame))
            {
                if (_presented.frame->viewportY == desired.y)
                    return;
                frame = _presented.frame->Reposition(desired.y, desired.geometry.height);
            }
            request = std::make_shared<PreviewRequest>(!frame);
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            request->startupTrace = _isEmbeddedChild ? 0 : ::Notepads::Markdown::Native::StartupTrace::Capture();
#endif
            request->renderer = _previewRenderer;
            request->disclosureOverrides = _disclosureOverrides;
            request->noteEmbedExtents = _noteEmbedExtents;
            request->cancel = _previewCancel;
            request->key = _previewRenderer->Key();
            request->update = _previewUpdate;
            request->sourceGeneration = _previewSourceGeneration;
            request->latestToken = _previewToken;
            request->token = _previewToken->load(std::memory_order_acquire);
            _previewScheduledToken = request->token;
            request->geometry = desired.geometry;
            request->height = desired.geometry.height;
            request->y = desired.y;
            request->viewportSerial = desired.serial;
            if (request->height <= 0 || ActualWidth() <= 0)
            {
                if (_previewUpdate)
                    _previewUpdate->CompleteViewportWait();
                (void)CanScheduleViewportFrame();
                return;
            }
            _previewRequestedHeight = request->height;
            _previewRequestedY = request->y;
            _viewportActive = request;
            if (frame)
            {
                request->frame = std::move(frame);
                InstallPreviewFrame(request);
            }
            else
            {
                _previewQueue->Enqueue(request);
                ObservePreviewFrame(get_weak(), Dispatcher(), request);
            }
        }
        catch (...)
        {
            if (_viewportActive == request)
                _viewportActive.reset();
            _renderingError = hresult_error(to_hresult()).message();
            if (_previewUpdate)
                _previewUpdate->Complete(Markdown::DocumentUpdateStatus::Failed, static_cast<int32_t>(to_hresult()));
            (void)CanScheduleViewportFrame();
        }
    }

    fire_and_forget MarkdownView::ObservePreviewFrame(weak_ref<MarkdownView> owner,
                                                      Windows::UI::Core::CoreDispatcher dispatcher,
                                                      std::shared_ptr<PreviewRequest> request)
    {
        ::Notepads::Markdown::Native::ModuleLifetime module;
        try
        {
            co_await resume_background();
            co_await resume_on_signal(request->completed.get());
            auto dispatched = co_await resume_agile(
                dispatcher.TryRunAsync(Windows::UI::Core::CoreDispatcherPriority::Normal,
                                       [owner, request]
                                       {
                                           if (auto control = owner.get())
                                               control->InstallPreviewFrame(request);
                                           else if (request->update)
                                               request->update->Complete(Markdown::DocumentUpdateStatus::NotLoaded);
                                       }));
            if (!dispatched && request->update &&
                request->update->layoutGeneration.load(std::memory_order_acquire) == request->key.layout_generation)
                request->update->Complete(Markdown::DocumentUpdateStatus::NotLoaded);
            if (!dispatched)
                request->deliveryFailed.store(true, std::memory_order_release);
        }
        catch (...)
        {
            request->deliveryFailed.store(true, std::memory_order_release);
            if (request->update &&
                request->update->layoutGeneration.load(std::memory_order_acquire) == request->key.layout_generation)
                request->update->Complete(Markdown::DocumentUpdateStatus::Failed, static_cast<int32_t>(to_hresult()));
        }
    }

    void MarkdownView::InstallInitialViewport(std::shared_ptr<PreviewRequest> const &request,
        std::shared_ptr<::Notepads::Markdown::Native::PreviewFrame> const &frame) noexcept
    {
        try
        {
            auto current = [&]
            {
                return _viewportActive == request && !_presented.frame && !_viewportToRestore && !_imageRestoringY &&
                    !_resourceRenderingDeferred && _previewUpdate == request->update && request->update->Pending() &&
                    CurrentViewportFrame(*frame, request->geometry) && _previewScroll.VerticalOffset() == 0;
            };
            if (_surfaceReady || !CaptureViewportGeometry() || !current())
                return;
            PresentedViewport candidate{frame, request->geometry, request->viewportSerial};
            if (!DrawPreview(candidate, false))
                return;
            _initialViewportToken = request->token;
            if (!current())
            {
                ClearInitialViewport();
                return;
            }
            _initialViewportChanged(*this, nullptr);
        }
        catch (...)
        {
            if (_initialViewportToken == request->token)
                ClearInitialViewport();
        }
    }

    void MarkdownView::ClearInitialViewport() noexcept
    {
        if (!std::exchange(_initialViewportToken, 0))
            return;
        _surfaceReady = false;
        try
        {
            if (_surfaceBrush)
                _surfaceBrush.ImageSource(nullptr);
            if (_loaded)
                (void)Dispatcher().TryRunAsync(Windows::UI::Core::CoreDispatcherPriority::Normal, [owner = get_weak()]
                {
                    try
                    {
                        if (auto self = owner.get())
                            self->_initialViewportChanged(*self, nullptr);
                    }
                    catch (...) {}
                });
        }
        catch (...) {}
    }

    void MarkdownView::InstallPreviewFrame(std::shared_ptr<PreviewRequest> const &request)
    {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        namespace UiTrace = ::Notepads::Markdown::Native::StartupTrace;
        UiTrace::UiScope uiTrace{UiTrace::Phase::UiInstall, UiTrace::Phase::UiInstallEnd, request->key.layout_generation};
#endif
        auto lifetime = get_strong();
        struct Completion
        {
            MarkdownView *owner;
            std::shared_ptr<PreviewRequest> request;
            int exceptions{std::uncaught_exceptions()};
            ~Completion() noexcept
            {
                // An extent/scroll setter can throw before DrawPreview starts.
                // Ordinary stale returns and an already committed publication
                // retain their own outcome; only an escaping exception settles
                // otherwise-unclassified UI work as a failed attempt.
                if (std::uncaught_exceptions() > exceptions &&
                    request->publication == PreviewRequest::UiPublication::Stale)
                    request->publication = PreviewRequest::UiPublication::DrawFailed;
                owner->FinishViewportRequest(request);
            }
        } completion{this, request};
        if (!_loaded || !_documentAccepted || request->sourceGeneration != _previewSourceGeneration ||
            request->key.layout_generation != _previewLayoutGeneration)
            return;
        if (request->prepareEditResources && request->status != NOTEPADS_MARKDOWN_CORE_OK &&
            request->token == _previewToken->load(std::memory_order_acquire) &&
            request->update == _previewUpdate && _viewportActive == request &&
            request->update && request->update->AllowsViewportFrame() && !request->reading->fault)
        {
            // Optional preflight must preserve the ordinary restoration path
            // under pressure. This source attempts it only once, and retains
            // every old resource owner for the first fully measured Frame.
            _viewportActive.reset();
            request->renderer.reset();
            StartPreviewLayout();
            return;
        }
        if (request->status != NOTEPADS_MARKDOWN_CORE_OK)
        {
            if (request->token != _previewToken->load(std::memory_order_acquire))
                return;
            if (_images && _images->transitionDescriptors)
                _images->DiscardTransition();
            _derivedEditReuse.reset();
            if (_previewUpdate)
                _previewUpdate->Complete(PreviewStatus(request->status), request->status);
            _lastStatus = Markdown::RenderingStatus::RenderFailure;
            _renderingError =
                L"Markdown " + hstring(request->failureStage) + L" failed (" + to_hstring(request->status) + L").";
            return;
        }
        if (!CaptureViewportGeometry())
            return;
        if (request->token != _previewToken->load(std::memory_order_acquire) || request->update != _previewUpdate ||
            _viewportActive != request)
        {
            // A current Frame may already be queued. Do not create an endless
            // chain where every obsolete completion supersedes its successor.
            if (_previewScheduledToken != _previewToken->load(std::memory_order_acquire))
                StartPreviewFrame();
            return;
        }
        if (_previewUpdate && _previewUpdate->state.load(std::memory_order_acquire) == 3 &&
            _previewUpdate->status != Markdown::DocumentUpdateStatus::Ready)
            return;
        if (request->prepareEditResources)
        {
            if (request->geometry.epoch != _viewportGeometry.epoch || request->viewportSerial <= _presented.serial)
                return;
            auto const &resources = request->editResources;
            if (!resources.main || !resources.descriptors ||
                resources.main->TextGeneration() != resources.descriptors->textGeneration)
            {
                if (request->update)
                    request->update->Complete(Markdown::DocumentUpdateStatus::Failed, NOTEPADS_MARKDOWN_CORE_INVALID_STATE);
                request->publication = PreviewRequest::UiPublication::DrawFailed;
                return;
            }
            auto current = [&]
            {
                return _loaded && _documentAccepted && request->sourceGeneration == _previewSourceGeneration &&
                    request->key.layout_generation == _previewLayoutGeneration &&
                    request->token == _previewToken->load(std::memory_order_acquire) &&
                    request->update == _previewUpdate && _viewportActive == request &&
                    request->geometry.epoch == _viewportGeometry.epoch && request->update &&
                    request->update->AllowsViewportFrame();
            };
            // Existing transfer code consumes only these semantic/resource
            // fields. This local carrier is never drawn or published as Ready.
            ::Notepads::Markdown::Native::PreviewFrame transfer;
            transfer.main = resources.main;
            transfer.descriptors = resources.descriptors;
            transfer.math = request->math;
            transfer.diagram = request->diagram;
            try
            {
                (void)RestoreNoteEmbeds(transfer);
                if (!current())
                    return;
                if (resources.imageWidths && !resources.imageWidths->empty())
                {
                    (void)RestoreCompletedImages(transfer, *resources.imageWidths);
                    if (!current())
                        return;
                }
                (void)RestoreDerivedForEdit(transfer);
            }
            catch (...)
            {
                // Current optional transfer state remains valid for the
                // ordinary measured path, including partial admission.
            }
            if (!current())
                return;
            _viewportActive.reset();
            request->renderer.reset();
            StartPreviewLayout();
            return;
        }
        if (!request->frame || !request->frame->main || !request->frame->descriptors || !request->frame->navigation ||
            request->frame->main->TextGeneration() != request->frame->descriptors->textGeneration)
        {
            _derivedEditReuse.reset();
            _renderingError = L"Reading Frame is incomplete: main=" +
                              to_hstring(request->frame && static_cast<bool>(request->frame->main)) + L", descriptors=" +
                              to_hstring(request->frame && static_cast<bool>(request->frame->descriptors)) + L", navigation=" +
                              to_hstring(request->frame && static_cast<bool>(request->frame->navigation));
            if (_previewUpdate)
                _previewUpdate->Complete(Markdown::DocumentUpdateStatus::Failed, NOTEPADS_MARKDOWN_CORE_INVALID_STATE);
            request->publication = PreviewRequest::UiPublication::DrawFailed;
            return;
        }
        if (request->geometry.epoch != _viewportGeometry.epoch || request->viewportSerial <= _presented.serial)
            return;
        // The first layout completes semantic descriptors for this revision.
        // Rebind unchanged completed resources before any fallback frame can
        // replace the old surface, then measure their current image geometry.
        try
        {
            const bool notesChanged = RestoreNoteEmbeds(*request->frame);
            if (_previewUpdate != request->update || _viewportActive != request)
                return;
            const bool imagesChanged = RestoreCompletedImages(*request->frame) && request->frame->images &&
                _imagePresentation && request->frame->images->geometryGeneration != _imagePresentation->geometryGeneration;
            const bool derivedChanged = RestoreDerivedForEdit(*request->frame);
            if (notesChanged || imagesChanged || derivedChanged)
            {
                StartPreviewLayout();
                return;
            }
        }
        catch (...)
        {
            auto error = static_cast<int32_t>(to_hresult());
            if (request->update == _previewUpdate && request->sourceGeneration == _previewSourceGeneration)
            {
                _derivedEditReuse.reset();
                if (request->update)
                    request->update->Complete(Markdown::DocumentUpdateStatus::Failed, error);
                _lastStatus = Markdown::RenderingStatus::RenderFailure;
                _renderingError = L"Markdown resource transfer failed (" + to_hstring(error) + L").";
            }
            request->publication = PreviewRequest::UiPublication::DrawFailed;
            return;
        }
        request->frame = ProjectImageFrame(request->frame);
        if (!CurrentViewportFrame(*request->frame, request->geometry))
            return;
        _previewRenderer = request->renderer;
        if (RestoreCachedViewport(*request->frame) || (!_viewportToRestore && RestoreImageAnchor(request->frame)))
            return;
        PresentedViewport candidate{request->frame, request->geometry, request->viewportSerial};
        if (_previewExtent)
        {
            auto width = request->frame->key.width + 2 * _padding;
            auto height = std::max(ActualHeight(), request->frame->totalHeight + 2 * _padding);
            // XAML may return the single-precision layout dimension through a double property.
            const auto matchesLayoutDimension = [](double actual, double requested) noexcept
            {
                return actual == requested || actual == static_cast<double>(static_cast<float>(requested));
            };
            if (!matchesLayoutDimension(_previewExtent.Width(), width) ||
                !matchesLayoutDimension(_previewExtent.Height(), height))
            {
                auto intent = BeginImmediateScroll();
                if (!CurrentScrollIntent(intent) || !CurrentViewportFrame(*request->frame, request->geometry))
                    return;
                _previewExtent.Width(width);
                _previewExtent.Height(height);
            }
        }
        if (_previewScroll)
        {
            if (_previewScroll.VerticalScrollMode() != ScrollMode::Enabled)
                _previewScroll.VerticalScrollMode(ScrollMode::Enabled);
            if (_previewScroll.VerticalScrollBarVisibility() != ScrollBarVisibility::Visible)
                _previewScroll.VerticalScrollBarVisibility(ScrollBarVisibility::Visible);
        }
        if (request->token != _previewToken->load(std::memory_order_acquire) || request->update != _previewUpdate)
            return;
        if (_previewUpdate && _previewUpdate->Pending())
        {
            int64_t pixelRow{};
            double targetY{};
            if (!::Notepads::Markdown::Native::QuantizeViewportY(_previewScroll.VerticalOffset(), _viewportGeometry.scale,
                                                               pixelRow, targetY))
                throw hresult_invalid_argument();
            // FinishViewportRequest can use the installed renderer to draw the current offset.
            if (request->frame->viewportY != targetY)
                return;
        }
        std::shared_ptr<TextSnapshot const> nextSnapshot = _automationSnapshot;
        auto text = request->frame->main;
        bool changed = !nextSnapshot || !nextSnapshot->main || nextSnapshot->token != text->TextGeneration();
        auto previousVisibility = changed || !_presented.frame ? nullptr : _presented.frame->visibility;
        if (changed)
        {
            try
            {
                auto snapshot = std::allocate_shared<TextSnapshot>(
                    ::Notepads::Markdown::Native::ReadingAllocator<TextSnapshot>{text->Memory()});
                snapshot->document = _acceptedDocument;
                snapshot->revision = _acceptedRevision;
                snapshot->generation = _previewUpdate ? _previewUpdate->generation : _generation;
                snapshot->token = text->TextGeneration();
                snapshot->main = text;
                snapshot->descriptors = request->frame->descriptors;
                snapshot->section = request->frame->section;
                nextSnapshot = std::move(snapshot);
            }
            catch (::Notepads::Markdown::Native::ReadingFailure const &error)
            {
                if (_previewUpdate)
                    _previewUpdate->Complete(PreviewStatus(error.status), error.status);
                return;
            }
            catch (...)
            {
                if (_previewUpdate)
                    _previewUpdate->Complete(Markdown::DocumentUpdateStatus::Failed, static_cast<int32_t>(to_hresult()));
                return;
            }
        }
        auto update = _previewUpdate;
        bool first = update && update->Pending();
        if (first && !update->Claim())
            return;
        struct ClaimedPublication
        {
            std::shared_ptr<PreviewUpdate> update;
            ~ClaimedPublication() noexcept
            {
                // Every exit after Claim must signal the actual claimed receipt,
                // including an exception before the visible state is committed.
                if (update && update->state.load(std::memory_order_acquire) == 1)
                    update->Published(false, 0);
            }
        } publication{first ? update : nullptr};
        auto publicationDocument = _acceptedDocument;
        auto publicationRevision = _acceptedRevision;
        auto publicationGeneration = update ? update->generation : _generation;
        request->publication = PreviewRequest::UiPublication::DrawFailed;
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        namespace Trace = ::Notepads::Markdown::Native::StartupTrace;
        Trace::MarkFor(request->startupTrace, Trace::Phase::Draw, request->key.layout_generation);
#endif
        bool drawn = DrawPreview(candidate);
        auto current = CurrentViewportFrame(*candidate.frame, candidate.geometry) && _previewUpdate == update &&
                       _acceptedDocument == publicationDocument && _acceptedRevision == publicationRevision &&
                       _viewportActive == request && _presented.serial < candidate.serial;
        if (!current)
            request->publication = PreviewRequest::UiPublication::Stale;
        drawn = drawn && current;
        if (drawn)
        {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            Trace::MarkFor(request->startupTrace, Trace::Phase::DrawReady, request->key.layout_generation);
#endif
            _initialViewportToken = 0;
            _displayedDocument = publicationDocument;
            _displayedRevision = publicationRevision;
            _displayedGeneration = publicationGeneration;
            auto previousInspection = _presented.inspection;
            _presented = std::move(candidate);
            _automationSnapshot = std::move(nextSnapshot);
            CompleteImageAnchor(*_presented.frame);
            if (changed)
            {
                _selectionAnchor = _selectionActive = _automationSnapshot->Bounds().start;
                _readingCaretTrailing = false;
            }
            // Presentation carets resolve owner aliases; nonempty ranges keep
            // their exact TSV separator endpoints, in either selection direction.
            request->frame->navigation->SnapSelection(_selectionAnchor, _selectionActive);
            _lastStatus = Markdown::RenderingStatus::Success;
            request->publication = PreviewRequest::UiPublication::Published;
            if (changed)
                ClearReadingHover();
            else
                RefreshReadingHover();
            UpdateReadingLinkCursor();
            UpdateCodeCopyButton();
            NotifyDiagramInspectionPublished(previousInspection);
        }
        if (first)
        {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            if (drawn)
                Trace::MarkFor(request->startupTrace, Trace::Phase::Published, request->key.layout_generation);
#endif
            update->Published(drawn, request->frame->totalBlocks);
        }
        ReadingAction action{request->frame, _automationSnapshot, _presented.serial};
        if (!drawn || !CurrentReadingAction(action))
            return;
        UpdateNoteEmbeds();
        if (!CurrentReadingAction(action))
            return;
        if (auto parent = _noteParentView.get())
            parent->UpdateNoteEmbedExtent(*this);
        if (!CurrentReadingAction(action))
            return;
        if (changed)
        {
            RaiseReadingTextChanged();
            if (auto parent = _noteParentView.get())
                parent->RaiseNoteEmbedStructureChanged();
            if (!CurrentReadingAction(action))
                return;
        }
        else if (previousVisibility && previousVisibility != request->frame->visibility)
        {
            if (_focusedAction.kind != ::Notepads::Markdown::Native::ReadingActionKind::None)
            {
                using Kind = ::Notepads::Markdown::Native::ReadingActionKind;
                using Child = ::Notepads::Markdown::Native::ReadingChildKind;
                auto kind = _focusedAction.kind == Kind::Link ? Child::Link :
                    _focusedAction.kind == Kind::Disclosure ? Child::Disclosure : Child::Object;
                if (!ReadingChildVisible(*action.snapshot, action.snapshot->descriptors->Child(kind, _focusedAction.index)))
                    _focusedAction = {};
            }
            NotifyDisclosureChanges(action, *previousVisibility);
            if (!CurrentReadingAction(action))
                return;
        }
        try
        {
            TryApplyPendingReadingReveal();
            if (!CurrentReadingAction(action))
                return;
            FinishPendingCaret();
            if (!CurrentReadingAction(action))
                return;
            if (_dragging)
                ContinueReadingDrag();
        }
        catch (...)
        {
            if (CurrentReadingAction(action))
                _pendingCaretLine.reset();
        }
        if (!CurrentReadingAction(action))
            return;
        NotifyImageFramePublished();
        if (!CurrentReadingAction(action))
            return;
        RaiseViewportChanged();
        if (!CurrentReadingAction(action))
            return;
        if (update && std::exchange(update->surfaceRecovery, false))
        {
            // Empty/thematic documents have no text anchor and therefore no
            // ViewportChanged notification. Publish the recovered surface only
            // after its exact displayed and reading identities are committed.
            try { _initialViewportChanged(*this, nullptr); }
            catch (...) { /* A host callback cannot invalidate the recovered Frame. */ }
            if (!CurrentReadingAction(action))
                return;
        }
        QueueResourceRendering();
    }

    void MarkdownView::IsResourceRenderingDeferred(bool value)
    {
        CheckUiThread();
        if (_resourceRenderingDeferred == value)
            return;
        _resourceRenderingDeferred = value;
        if (value)
        {
            ClearInitialViewport();
            if (auto work = _previewHighlightWork.lock(); work && work->cache)
                work->Supersede();
        }
        else
            QueueResourceRendering();
    }

    void MarkdownView::QueueResourceRendering()
    {
        if (_resourceRenderingDeferred || !_loaded || !_documentAccepted || !_readingSource)
            return;
        // Resource admission must not invalidate a committed viewport before
        // observers receive it, or block the host's reveal callback with setup.
        auto sourceGeneration = _previewSourceGeneration;
        (void)Dispatcher().RunAsync(Windows::UI::Core::CoreDispatcherPriority::Low,
                                   [weak = get_weak(), sourceGeneration]
                                   {
                                       auto control = weak.get();
                                       auto current = [&]
                                       {
                                           return control && !control->_resourceRenderingDeferred && control->_loaded &&
                                                  control->_documentAccepted && control->_readingSource &&
                                                  control->_previewSourceGeneration == sourceGeneration;
                                       };
                                       if (!current())
                                           return;
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                                       namespace UiTrace = ::Notepads::Markdown::Native::StartupTrace;
                                       UiTrace::UiScope uiTrace{UiTrace::Phase::UiResourceAdmission, UiTrace::Phase::UiResourceAdmissionEnd, control->_previewLayoutGeneration};
#endif
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                                       if (control->_imageFontControlTests)
                                           ++control->_imageFontControlTests->resourceCallbacks;
#endif
                                       // Images require the current per-cell geometry; derived
                                       // binding advances resource generations and invalidates it.
                                       control->PumpImages();
                                       if (!current())
                                           return;
                                       control->BindMath();
                                       if (current())
                                           control->BindDiagram();
                                       if (current())
                                           control->QueuePreviewHighlights();
                                   });
    }

    MarkdownView::HighlightSelection MarkdownView::VisibleHighlightBlocks() const noexcept
    {
        HighlightSelection selection;
        auto const &frame = _presented.frame;
        if (!frame || !frame->measuredBlocks || _presented.inspection)
            return selection;
        auto const top = frame->viewportY - _padding;
        auto const bottom = top + frame->viewportHeight;
        for (auto const &visual : frame->Blocks())
        {
            auto const &block = visual.block;
            // Layout measures overscan for raster reuse. Lex only actual
            // visible code, including a fence that crosses either viewport edge.
            if (block.kind != 3 || block.height <= 0 || block.y >= bottom ||
                block.y + block.height <= top)
                continue;
            if (selection.count == selection.nodes.size())
                break;
            selection.nodes[selection.count++] = block.node;
        }
        auto end = selection.nodes.begin() + selection.count;
        std::sort(selection.nodes.begin(), end);
        selection.count = static_cast<uint32_t>(std::unique(selection.nodes.begin(), end) - selection.nodes.begin());
        return selection;
    }

    bool MarkdownView::PreviewHighlightsCover(HighlightSelection const &selection) const noexcept
    {
        return selection.count == 0 || (_previewHighlights && std::all_of(
            selection.Nodes().begin(), selection.Nodes().end(),
            [&](uint32_t node) { return _previewHighlights->Find(node) != nullptr; }));
    }

    void MarkdownView::QueuePreviewHighlights() noexcept
    {
        auto source = _readingSource;
        if (!_loaded || _resourceRenderingDeferred || !source || !source->deferHighlights ||
            !_presented.frame || !_presented.frame->main ||
            _presented.frame->main->TextGeneration() != source->textGeneration)
            return;

        const bool viewport = _previewViewportHighlights;
        auto selection = viewport ? VisibleHighlightBlocks() : HighlightSelection{};
        if (auto active = _previewHighlightWork.lock())
        {
            if (viewport && active->source == source && std::none_of(active->selection.Nodes().begin(),
                    active->selection.Nodes().end(), [&](uint32_t node) { return selection.Contains(node); }))
                active->Supersede();
            return;
        }
        if (_previewHighlightFailed && _previewHighlightFailed != selection)
            _previewHighlightFailed.reset();
        if (viewport ? PreviewHighlightsCover(selection) || _previewHighlightFailed == selection :
                       _previewLegacyHighlightsAttempted)
            return;

        try
        {
            auto work = std::make_shared<PreviewHighlightWork>();
            work->source = source;
            work->selection = selection;
            if (viewport)
            {
                if (!_previewHighlightCache)
                    _previewHighlightCache = std::make_shared<::Notepads::Markdown::Native::NativeHighlightCache>();
                work->cache = _previewHighlightCache;
                // Scrolling may cancel this batch without canceling layout.
                // StopPreview cancels both flags when the source is retired.
                work->cancel = std::make_shared<::Notepads::Markdown::Native::PreviewCancellation>(
                    source->memory->ModelOwner().Get());
            }
            else
                _previewLegacyHighlightsAttempted = true;
            _previewHighlightWork = work;
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            ++_previewHighlightCounts.requested;
            work->startupTrace = _isEmbeddedChild ? 0 : ::Notepads::Markdown::Native::StartupTrace::Capture();
#endif
            ObservePreviewHighlights(get_weak(), _owningDispatcher, std::move(work));
        }
        catch (...)
        {
            if (viewport)
                _previewHighlightFailed = selection;
        }
    }

    fire_and_forget MarkdownView::ObservePreviewHighlights(weak_ref<MarkdownView> owner,
        Windows::UI::Core::CoreDispatcher dispatcher, std::shared_ptr<PreviewHighlightWork> work)
    {
        ::Notepads::Markdown::Native::ModuleLifetime module;
        try
        {
            co_await resume_background();
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            using ::Notepads::Markdown::Native::StartupTrace::MarkFor;
            using ::Notepads::Markdown::Native::StartupTrace::Phase;
            LARGE_INTEGER started{}, ended{};
            ::QueryPerformanceCounter(&started);
            MarkFor(work->startupTrace, Phase::AsyncHighlights);
#endif
            auto result = work->source->BuildHighlights(work->selection.Nodes(), work->cache.get(), work->cancel.get());
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            ::QueryPerformanceCounter(&ended);
            auto elapsed = static_cast<uint64_t>(ended.QuadPart - started.QuadPart);
            if (result.snapshot)
                MarkFor(work->startupTrace, Phase::AsyncHighlightsReady);
#endif
            (void)co_await resume_agile(dispatcher.TryRunAsync(Windows::UI::Core::CoreDispatcherPriority::Low,
                [owner, work, result = std::move(result)
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                 , elapsed
#endif
                ]
                {
                    auto control = owner.get();
                    if (!control || control->_previewHighlightWork.lock() != work)
                        return;
                    control->_previewHighlightWork.reset();
                    const bool currentSource = control->_loaded && control->_readingSource == work->source &&
                        !work->superseded && (!work->cache || !control->_resourceRenderingDeferred);
                    auto visible = work->cache ? control->VisibleHighlightBlocks() : HighlightSelection{};
                    const bool currentViewport = !work->cache ||
                        (visible.count != 0 && std::all_of(visible.Nodes().begin(), visible.Nodes().end(),
                            [&](uint32_t node) { return work->selection.Contains(node); }));
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                    auto &counts = control->_previewHighlightCounts;
                    ++counts.started;
                    if (result.snapshot)
                    {
                        ++counts.completed;
                        counts.reusedBlocks += result.reusedBlocks;
                        counts.newlyCompletedBlocks += result.snapshot->Summary().complete_blocks - result.reusedBlocks;
                    }
                    counts.workTicks += elapsed;
                    if (!currentSource || !currentViewport)
                        ++counts.rejected;
#endif
                    if (currentSource && currentViewport && result.snapshot)
                    {
                        const bool colorsChanged = result.snapshot->Summary().complete_blocks != 0;
                        // Keep terminal rows even when all visible fences use plain text.
                        // Only a pixel change needs to invalidate the retained raster.
                        control->_previewHighlights = std::move(result.snapshot);
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                        ++control->_previewHighlightCounts.applied;
#endif
                        if (colorsChanged)
                        {
                            control->_previewRasterBand.reset();
                            try
                            {
                                if (control->ReadingAvailable() && control->DrawPreview(control->_presented.frame))
                                {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                                    MarkFor(work->startupTrace, Phase::AsyncHighlightsApplied);
#endif
                                }
                            }
                            catch (...) { /* A later paint uses the same result. */ }
                        }
                    }
                    else if (currentSource && currentViewport && work->cache)
                        control->_previewHighlightFailed = work->selection;
                    control->QueuePreviewHighlights();
                }));
        }
        catch (...) { /* A closed dispatcher drops optional colors. */ }
    }

#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
    bool MarkdownView::StartupHighlightsPendingForDiagnostics() const
    {
        CheckUiThread();
        if (!_readingSource || !_readingSource->deferHighlights)
            return false;
        if (!_previewViewportHighlights)
            return !_previewLegacyHighlightsAttempted || !_previewHighlightWork.expired();
        if (!_presented.frame || !_presented.frame->main ||
            _presented.frame->main->TextGeneration() != _readingSource->textGeneration)
            return true;
        auto selection = VisibleHighlightBlocks();
        return !PreviewHighlightsCover(selection) && _previewHighlightFailed != selection;
    }

    com_array<uint64_t> MarkdownView::ViewportHighlightDiagnostics() const
    {
        CheckUiThread();
        auto const selection = VisibleHighlightBlocks();
        auto highlight = _previewHighlights ? _previewHighlights : _presented.frame ? _presented.frame->highlights : nullptr;
        uint64_t terminal{}, complete{};
        if (highlight && _presented.frame && _presented.frame->main && _readingSource &&
            _presented.frame->main->TextGeneration() == _readingSource->textGeneration)
            for (auto node : selection.Nodes())
                if (auto row = highlight->Find(node))
                {
                    ++terminal;
                    complete += row->state == NOTEPADS_MARKDOWN_HIGHLIGHT_COMPLETE;
                }
        auto const &c = _previewHighlightCounts;
        return {1, c.requested, c.started, c.completed, c.applied, c.rejected, c.reusedBlocks, c.newlyCompletedBlocks,
                _previewHighlightWork.expired() ? 0ull : 1ull, selection.count, terminal, complete,
                _readingSource ? _readingSource->textGeneration : 0, c.workTicks};
    }
#endif

    void MarkdownView::OnPreviewLayoutUpdated(IInspectable const &, IInspectable const &)
    {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        namespace UiTrace = ::Notepads::Markdown::Native::StartupTrace;
        UiTrace::UiScope uiTrace{UiTrace::Phase::UiLayoutUpdated, UiTrace::Phase::UiLayoutUpdatedEnd, _previewLayoutGeneration};
#endif
        if (!_loaded || !CaptureViewportGeometry())
        {
            SetReadingLinkCursor(false);
            return;
        }
        ObserveWheelScroll();
        if (!_documentAccepted)
            return;
        if (!_previewModelReady)
        {
            ScheduleCurrent();
            return;
        }
        if (_previewUpdate && _previewUpdate->state.load(std::memory_order_acquire) == 3 &&
            _previewUpdate->status != Markdown::DocumentUpdateStatus::Ready &&
            _previewUpdate->status != Markdown::DocumentUpdateStatus::NotLoaded &&
            !(_viewportRecovery && _previewUpdate->status == Markdown::DocumentUpdateStatus::Failed))
            return;
        auto width = _viewportGeometry.width;
        auto height = _viewportGeometry.height;
        if (width <= 2 * _padding || height <= 0)
        {
            SetReadingLinkCursor(false);
            return;
        }
        // A vertical scrollbar can change ViewportWidth without changing this
        // Control's ActualWidth or its scroll offset. Observe actual viewport geometry.
        const bool retry = _previewUpdate && _previewUpdate->state.load(std::memory_order_acquire) == 3 &&
            _previewUpdate->status == Markdown::DocumentUpdateStatus::NotLoaded;
        // Resource deliveries already own a coalescing refresh timer. An
        // unrelated XAML layout pass must not cancel their in-flight measure
        // before that timer captures the latest immutable resource inputs.
        if (!retry && !_previewRenderer && HasActivePreviewLayout() &&
            ((_imageRefreshTimer && _imageRefreshTimer.IsEnabled()) ||
             (_derivedLayoutRefreshTimer && _derivedLayoutRefreshTimer.IsEnabled())))
            return;
        if (retry || !_previewRenderer || std::abs(_previewRequestedWidth - (width - 2 * _padding)) > 0.25)
            StartPreviewLayout();
        else if (std::abs(_previewRequestedHeight - height) > 0.25)
            StartPreviewFrame();
        else if (_presented.frame && _presented.geometry.epoch != _viewportGeometry.epoch)
        {
            if (!DrawPreview())
                StartPreviewFrame();
        }
        // XAML can measure an expanded extent after its frame has already published.
        TryApplyPendingReadingReveal();
    }

#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
    ::Notepads::Markdown::Native::ViewportRasterBandObservation ImageReuseControlTestAccess::ObserveRasterBand(
        Markdown::MarkdownView const &view)
    {
        auto control = get_self<MarkdownView>(view);
        control->CheckUiThread();
        ::Notepads::Markdown::Native::ViewportRasterBandObservation result;
        auto const band = control->_previewRasterBand;
        auto const frame = control->_presented.frame;
        if (!band || !frame)
            return result;
        result.surface = reinterpret_cast<uintptr_t>(get_abi(control->_surface));
        result.bytes = band->band.bytes;
        result.firstRow = band->band.firstRow;
        result.pixelHeight = band->band.pixelHeight;
        result.viewportPixelHeight = control->_presented.geometry.pixelHeight;
        result.viewportY = frame->viewportY;
        result.scale = control->_presented.geometry.scale;
        double snapped{};
        if (!::Notepads::Markdown::Native::QuantizeViewportY(frame->viewportY, result.scale, result.viewportRow, snapped))
            return result;
        result.current = control->_surfaceReady && !control->_viewportActive && !control->_viewportRendering &&
            control->CurrentViewportFrame(*frame, control->_presented.geometry) &&
            band->SameContent(*frame, control->_presented.geometry);
        auto const placement = control->_surfaceTarget.RenderTransform().try_as<TranslateTransform>();
        auto const clip = control->_viewportOverlay.Clip().try_as<RectangleGeometry>();
        if (placement && clip)
        {
            auto const rectangle = clip.Rect();
            auto const &geometry = control->_presented.geometry;
            result.cropMatches = band->band.Contains(result.viewportRow, geometry.pixelWidth, geometry.pixelHeight) &&
                control->_surfaceBrush.ImageSource().try_as<SurfaceImageSource>() == control->_surface &&
                control->_surfaceTarget.Width() == geometry.surfaceWidth &&
                control->_surfaceTarget.Height() == band->band.pixelHeight / geometry.scale &&
                placement.X() == geometry.surfaceX && placement.Y() == geometry.surfaceY -
                    static_cast<double>(result.viewportRow - band->band.firstRow) / geometry.scale &&
                rectangle.X == static_cast<float>(geometry.overlayX) && rectangle.Y == static_cast<float>(geometry.overlayY) &&
                rectangle.Width == static_cast<float>(geometry.width) && rectangle.Height == static_cast<float>(geometry.height);
        }
        return result;
    }

    Windows::UI::Xaml::Controls::Border ImageReuseControlTestAccess::RasterTarget(Markdown::MarkdownView const &view)
    {
        auto control = get_self<MarkdownView>(view);
        control->CheckUiThread();
        return control->_surfaceTarget;
    }

    bool ImageFontControlTestAccess::CoalescePendingResourceLayout(Markdown::MarkdownView const &view)
    {
        auto control = get_self<MarkdownView>(view);
        control->CheckUiThread();
        if (control->_previewRenderer || !control->HasActivePreviewLayout() || !control->_images ||
            !control->_images->bound)
            return false;
        auto const request = control->_viewportActive;
        const auto layout = control->_previewLayoutGeneration;
        const auto token = control->_previewToken->load(std::memory_order_acquire);
        // Use the actual immutable image-table boundary and its owning timer;
        // no request, resource result, generation or cancellation is fabricated.
        control->FreezeImagePresentation();
        control->QueueImageRefresh();
        if (request->images == control->_imagePresentation)
            return false;
        control->OnPreviewLayoutUpdated(nullptr, nullptr);
        return control->_viewportActive == request && control->_previewLayoutGeneration == layout &&
               control->_previewToken->load(std::memory_order_acquire) == token &&
               control->_imageRefreshTimer && control->_imageRefreshTimer.IsEnabled();
    }
#endif

    void MarkdownView::OnPreviewViewChanged(IInspectable const &, ScrollViewerViewChangedEventArgs const &args)
    {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        namespace UiTrace = ::Notepads::Markdown::Native::StartupTrace;
        UiTrace::UiScope uiTrace{UiTrace::Phase::UiViewChanged, UiTrace::Phase::UiViewChangedEnd, _previewLayoutGeneration};
#endif
        const bool extentChanged = _previewScroll && _wheelScrollableHeight >= 0 &&
                                   _wheelScrollableHeight != _previewScroll.ScrollableHeight();
        if (extentChanged && _imageUserScrollY)
            _imageUserScrollY = std::clamp(*_imageUserScrollY, 0.0, _previewScroll.ScrollableHeight());
        ObserveWheelScroll();
        if (!_documentAccepted)
            return;
        if (_previewUpdate && _previewUpdate->state.load(std::memory_order_acquire) == 3 &&
            _previewUpdate->status != Markdown::DocumentUpdateStatus::Ready &&
            _previewUpdate->status != Markdown::DocumentUpdateStatus::NotLoaded)
            return;
        auto y = _previewScroll ? _previewScroll.VerticalOffset() : 0.0;
        if (std::exchange(_lastPreviewScrollOffset, y) != y)
        {
            _lastPreviewScrollAt = GetTickCount64();
            _diagramInspection.reset();
        }
        int64_t pixelRow{};
        double paintedY{};
        if (_readingHover && _presented.frame &&
            (!::Notepads::Markdown::Native::QuantizeViewportY(y, _presented.geometry.scale, pixelRow, paintedY) ||
             paintedY != _presented.frame->viewportY))
            ClearReadingHover();
        if (!_imageAnchorUpdatingExtent)
        {
            if (_imageRestoringY)
            {
                // Keep the marker until its frame publishes. More than one
                // ViewChanged can arrive before the requested frame is drawn.
                if (std::abs(y - *_imageRestoringY) > 0.5 && args && !args.IsIntermediate())
                {
                    if (extentChanged || _imageAnchorExtentPending)
                        _imageRestoringY.reset();
                    else
                        CancelViewportRestore();
                }
            }
            else if (!extentChanged && !_imageAnchorExtentPending)
                UpdateImageAnchorForScroll(y);
        }
        auto width =
            _previewScroll && _previewScroll.ViewportWidth() > 0 ? _previewScroll.ViewportWidth() : ActualWidth();
        if (_previewModelReady && std::abs(_previewRequestedWidth - (width - 2 * _padding)) > 0.25)
            StartPreviewLayout();
        else if (_previewRenderer)
            StartPreviewFrame();
        UpdateCodeCopyButton();
        try { _verticalScrollChanged(*this, nullptr); }
        catch (...) { /* A host observer must not invalidate scrolling. */ }
    }

    void MarkdownView::StopPreview(Markdown::DocumentUpdateStatus status, bool stopController, bool wakeOwner,
                                   bool keepImageReuse, bool keepNoteEmbeds, bool keepDerivedReuse)
    {
        _previewDeviceRetry = false;
        _derivedLayoutDeviceRecovery = false;
        if (!keepNoteEmbeds)
            ClearNoteEmbeds();
        ClearInitialViewport();
        _lastPreviewScrollOffset = 0;
        _lastPreviewScrollAt = 0;
        _previewRasterBand.reset();
        _codeCopyPointer.reset();
        if (status == Markdown::DocumentUpdateStatus::NotLoaded || _dragging)
            _readingLinkPoint.reset();
        SetReadingLinkCursor(false);
        ClearReadingHover();
        CancelWheelScroll();
        CancelViewportRendering();
        _viewportDesired.reset();
        _viewportFailed.reset();
        _viewportActive.reset();
        _readingSearch.Invalidate();
        if (status != Markdown::DocumentUpdateStatus::Superseded)
            ClearMeasurementCache();
        auto derivedBeforeEdit = keepDerivedReuse ? TakeDerivedEditReuse() : std::nullopt;
        RetireDiagram();
        RetireMath();
        _derivedEditReuse = std::move(derivedBeforeEdit);
        RetireImages(keepImageReuse && status == Markdown::DocumentUpdateStatus::Superseded && !stopController);
        if (_previewUpdate)
            _previewUpdate->Complete(status);
        _previewTicket.Cancel();
        _previewTicket = {};
        _previewUpdate.reset();
        if (_previewCancel)
            _previewCancel->Cancel();
        if (_readingSource)
            _readingSource->Cancel();
        if (auto work = _previewHighlightWork.lock())
            work->Supersede();
        _readingSource.reset();
        _previewHighlights.reset();
        _previewLegacyHighlightsAttempted = false;
        _previewHighlightFailed.reset();
        if (!keepDerivedReuse)
            _previewHighlightCache.reset();
        _disclosureOverrides.reset();
        _noteEmbedExtents.reset();
        _pendingReadingReveal.reset();
        _textBitmapCache.Clear();
        _selectionAnchor = _selectionActive = 0;
        _readingCaretTrailing = false;
        _focusedAction = {};
        InvalidateVerticalNavigation();
        _dragging = _dragExceeded = false;
        _readingPointerId.reset();
        _pressedAction = {};
        _readingPressedFrame.reset();
        _readingPressedTaskGeneration = 0;
        try
        {
            if (_owningDispatcher && _owningDispatcher.HasThreadAccess())
            {
                try
                {
                    if (_readingDragTimer)
                        _readingDragTimer.Stop();
                }
                catch (...) {}
                try { ReleasePointerCaptures(); }
                catch (...) {}
                _readingPeers.reset();
            }
        }
        catch (...)
        {
            // UI teardown cannot skip the remaining native cancellation and drain.
        }
        _previewToken->fetch_add(1, std::memory_order_acq_rel);
        _previewRenderer.reset();
        _presented = {};
        RetireCodeCopyButton();
        _previewCancel.reset();
        _previewModelReady = false;
        _cachedPreviewModel.Reset();
        _lastViewport.reset();
        CancelViewportRestore();
        // Cancel remains terminal until an explicit Refresh/new source. Reload
        // and a new source may rebuild using a fresh internal receipt.
        _previewSubmittedSourceGeneration =
            status == Markdown::DocumentUpdateStatus::Canceled ? _previewSourceGeneration : 0;
        if (_previewController)
        {
            if (stopController)
            {
                auto drain = _previewDrainStorage;
                auto serial = drain->Begin();
                _previewController->Stop();
                _previewDrain = drain;
                try
                {
                    auto owner = wakeOwner ? get_weak() : weak_ref<MarkdownView>{};
                    DrainPreview(owner, _owningDispatcher, std::move(_previewController), drain, serial, wakeOwner);
                }
                catch (...)
                {
                    // Coroutine allocation failure never leaves a wait without
                    // a terminal receipt or grants another independent driver.
                    drain->Finished(serial, false);
                    drain->DeliveryFailed(serial);
                }
            }
            else
            {
                _previewController->RequestCancel();
            }
        }
    }

    fire_and_forget MarkdownView::DrainPreview(weak_ref<MarkdownView> owner,
                                                Windows::UI::Core::CoreDispatcher dispatcher,
                                                std::unique_ptr<::Notepads::Markdown::Native::CoreController> controller,
                                                std::shared_ptr<PreviewDrain> drain, uint64_t serial, bool wakeOwner)
    {
        ::Notepads::Markdown::Native::ModuleLifetime module;
        bool terminal{};
        try
        {
            co_await resume_background();
            auto result = controller->Drain();
            terminal = result.coreDrained && result.driverExited;
        }
        catch (...)
        {
            controller->Stop();
        }
        controller.reset();
        drain->Finished(serial, terminal);
        if (!wakeOwner)
            co_return;
        try
        {
            bool delivered = co_await resume_agile(dispatcher.TryRunAsync(
                Windows::UI::Core::CoreDispatcherPriority::Normal,
                [owner, drain, serial]
                {
                    if (auto self = owner.get(); self && self->_previewDrain == drain && self->_documentAccepted &&
                        drain->IsCurrent(serial) && self->IsPreviewLoaded() && self->_surfaceTarget &&
                        self->ActualWidth() > 0 && self->ActualHeight() > 0)
                        self->ScheduleCurrent();
                    else
                        drain->DeliveryFailed(serial);
                }));
            if (!delivered)
                drain->DeliveryFailed(serial);
        }
        catch (...)
        {
            drain->DeliveryFailed(serial);
        }
    }

    bool MarkdownView::DrawPreview(std::shared_ptr<::Notepads::Markdown::Native::PreviewFrame> projected)
    {
        if (!_presented.frame || !CaptureViewportGeometry())
            return false;
        auto candidate = _presented;
        if (!candidate.frame)
            return false;
        // Layout/selection redraw callbacks are not fresh viewport intent.
        // Do not repeatedly redraw the older retained surface after a failure.
        if (_viewportFailed && _viewportFailedToken == _previewToken->load(std::memory_order_acquire) &&
            _viewportFailed->geometry.epoch == _viewportGeometry.epoch)
            return false;
        auto originalFrame = _presented.frame;
        auto originalSerial = _presented.serial;
        try
        {
            candidate.frame = projected ? std::move(projected) : ProjectImageFrame(originalFrame);
        }
        catch (...)
        {
            return false;
        }
        if (candidate.geometry.epoch != _viewportGeometry.epoch)
        {
            candidate.geometry = _viewportGeometry;
            candidate.serial = ++_viewportSerial;
        }
        ::Notepads::Markdown::Native::ViewportTarget target;
        target.geometry = candidate.geometry;
        if (!::Notepads::Markdown::Native::QuantizeViewportY(candidate.frame->viewportY, candidate.geometry.scale,
                                                        target.pixelRow, target.y) ||
            FailedViewportTarget(target))
            return false;
        if (!DrawPreview(candidate) || !CurrentViewportFrame(*candidate.frame, candidate.geometry) ||
            _presented.frame != originalFrame || _presented.serial != originalSerial)
            return false;
        auto previousInspection = _presented.inspection;
        _presented = std::move(candidate);
        CompleteImageAnchor(*_presented.frame);
        _viewportFailed.reset();
        if (_presented.serial != originalSerial)
        {
            ClearReadingHover();
            UpdateReadingLinkCursor();
        }
        UpdateCodeCopyButton();
        if (_presented.frame && _presented.frame->images != originalFrame->images)
        {
            RefreshReadingHover();
            NotifyImageFramePublished();
        }
        NotifyDiagramInspectionPublished(previousInspection);
        if (previousInspection && !_presented.inspection)
            QueuePreviewHighlights();
        return true;
    }
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
    void MarkdownView::ArmPreviewDeviceLossForDiagnostics(bool initialViewport)
    {
        CheckUiThread();
        _failNextPreviewDrawOnInitialViewport = initialViewport;
    }
#endif

    bool MarkdownView::DrawPreview(PresentedViewport &candidate, bool showLinkHover)
    {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        namespace UiTrace = ::Notepads::Markdown::Native::StartupTrace;
        UiTrace::UiScope uiTrace{UiTrace::Phase::UiDraw, UiTrace::Phase::UiDrawEnd, candidate.frame ? candidate.frame->key.layout_generation : _previewLayoutGeneration};
#endif
        // One synchronous retry shares only these stack records. Diagnostics never
        // introduce ownership, callbacks, or history across later async attempts.
        struct DrawFault
        {
            wchar_t const *stage{};
            bool hasHresult{};
            HRESULT hresult{};
            bool hasDeviceReason{};
            HRESULT deviceReason{};
            bool hasCleanup{};
            HRESULT cleanup{};
        };
        DrawFault first{}, last{};
        auto remember = [&](DrawFault const &fault) noexcept
        {
            if (!first.stage)
                first = fault;
            last = fault;
            // Formatting/assignment must not prevent cleanup, recovery, or the
            // caller's displayed-identity rollback when DrawPreview returns false.
            try
            {
                std::wstring message{L"preview draw "};
                auto hex = [&](HRESULT value)
                {
                    constexpr wchar_t digits[]{L"0123456789ABCDEF"};
                    wchar_t number[11]{L'0', L'x'};
                    auto bits = static_cast<uint32_t>(value);
                    for (uint32_t index = 0; index < 8; ++index)
                        number[index + 2] = digits[(bits >> ((7 - index) * 4)) & 15];
                    message.append(number);
                };
                auto append = [&](wchar_t const *name, DrawFault const &value)
                {
                    message.append(name).append(L"{stage=").append(value.stage).append(L" hresult=");
                    if (value.hasHresult)
                        hex(value.hresult);
                    else
                        message.append(L"none");
                    message.append(L" deviceRemovedReason=");
                    if (value.hasDeviceReason)
                        hex(value.deviceReason);
                    else
                        message.append(L"not_available");
                    message.append(L" cleanupEndDraw=");
                    if (value.hasCleanup)
                        hex(value.cleanup);
                    else
                        message.append(L"not_called");
                    message.append(L"}");
                };
                append(L"first", first);
                message.append(L" ");
                append(L"last", last);
                _renderingError = hstring{message};
            }
            catch (...)
            {
                // The original failure and control flow remain authoritative.
            }
        };
        auto reject = [&](wchar_t const *stage) noexcept
        {
            remember(DrawFault{stage});
            return false;
        };
        bool paintStateRetried{}, bandAllocationRetried{};
        auto draw = [&](auto &&retry) -> bool
        {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            const auto traceLayout = candidate.frame ? candidate.frame->key.layout_generation : _previewLayoutGeneration;
            std::optional<UiTrace::UiScope> rasterTrace;
            auto traceStage = [&](UiTrace::Phase begin, UiTrace::Phase end) noexcept
            {
                rasterTrace.reset();
                rasterTrace.emplace(begin, end, traceLayout);
            };
            traceStage(UiTrace::Phase::UiRasterPrepare, UiTrace::Phase::UiRasterPrepareEnd);
#endif
            auto const retainedBand = _previewRasterBand;
            const bool previouslyReady = _surfaceReady;
            uint64_t attachmentSerial{};
            com_ptr<ISurfaceImageSourceNativeWithD2D> drawingNative;
            _surfaceReady = false;
            bool drawing{}, optionalSurface{};
            wchar_t const *stage = L"preflight.surface_target";
            try
            {
                if (!_surfaceTarget)
                    return reject(stage);
                stage = L"preflight.frame";
                if (!candidate.frame)
                    return reject(stage);
                stage = L"preflight.geometry";
                if (!CurrentViewportFrame(*candidate.frame, candidate.geometry))
                    return reject(stage);
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                if (_failNextPreviewDrawOnInitialViewport && _previewUpdate && _previewUpdate->fullSynchronization)
                {
                    const bool initialViewport = !candidate.frame->navigation;
                    const auto state = _previewUpdate->state.load(std::memory_order_acquire);
                    if (*_failNextPreviewDrawOnInitialViewport == initialViewport && state == (initialViewport ? 0 : 1))
                    {
                        _failNextPreviewDrawOnInitialViewport.reset();
                        if (initialViewport)
                        {
                            namespace Trace = ::Notepads::Markdown::Native::StartupTrace;
                            Trace::Mark(Trace::Phase::InitialViewportDeviceLoss);
                        }
                        stage = L"diagnostics.device_loss";
                        throw_hresult(D2DERR_RECREATE_TARGET);
                    }
                }
#endif
                stage = L"viewport.geometry";
                auto scale = static_cast<float>(candidate.geometry.scale);
                stage = L"viewport.pixel_size";
                auto pixelWidth = candidate.geometry.pixelWidth;
                auto pixelHeight = candidate.geometry.pixelHeight;
                if (pixelWidth < 1 || pixelHeight < 1 || pixelWidth > 16384 || pixelHeight > 16384)
                    throw hresult_invalid_argument(L"The native preview viewport exceeds the graphics surface limit.");
                if (_previewSurfaceSerial == UINT64_MAX)
                    return reject(L"surface.attachment_serial");
                attachmentSerial = ++_previewSurfaceSerial;
                candidate.hoveredLink = showLinkHover ? ReadingLinkAtPointer(candidate) : std::nullopt;
                int64_t pixelRow{}, previousRow{};
                double snappedY{};
                if (!::Notepads::Markdown::Native::QuantizeViewportY(candidate.frame->viewportY,
                        candidate.geometry.scale, pixelRow, snappedY))
                    return reject(L"surface.raster_row");
                auto bandPlan = ::Notepads::Markdown::Native::PlanViewportRasterBand(candidate.geometry, pixelRow,
                    _padding, candidate.frame->measuredTop, candidate.frame->measuredBottom,
                    candidate.frame->totalHeight, false);
                if (_diagramInspection && !FindDiagramVisual(*candidate.frame, _diagramInspection->key))
                    _diagramInspection.reset();
                if (_pendingDiagramInspection && FindDiagramVisual(*candidate.frame, *_pendingDiagramInspection, true))
                {
                    _diagramInspection = DiagramInspection{*_pendingDiagramInspection, {}};
                    _pendingDiagramInspection.reset();
                }
                const auto inspection = _diagramInspection;
                if (!candidate.frame->navigation || inspection)
                    bandPlan.reset();
#if defined(NOTEPADS_MARKDOWN_DISABLE_VIEWPORT_RASTER_BAND)
                bandPlan.reset();
#endif
                const auto selectionStart = std::min(_selectionAnchor, _selectionActive);
                const auto selectionEnd = std::max(_selectionAnchor, _selectionActive);
                const auto focusedAction = _focusedAction;
                const bool readingFocused = _readingFocused, inactiveCaret = _showInactiveCaretMarker;
                const bool trailingCaret = _readingCaretTrailing;
                const auto linkPoint = _readingLinkPoint;
                auto currentHighlights = [&]
                {
                    return _previewHighlights && _readingSource && candidate.frame->main &&
                        candidate.frame->main->TextGeneration() == _readingSource->textGeneration ? _previewHighlights : nullptr;
                };
                const auto highlights = currentHighlights();
                auto staticBand = [&](auto const &band)
                {
                    return StaticRasterBand(*candidate.frame, band, candidate.geometry, _padding,
                        selectionStart, selectionEnd, focusedAction, readingFocused, inactiveCaret, candidate.hoveredLink,
                        trailingCaret);
                };
                const bool mayExpand = !bandAllocationRetried && (!retainedBand ||
                    !retainedBand->SameSourceLayout(*candidate.frame, candidate.geometry) || retainedBand->mayExpand);
                auto samePaintState = [&]
                {
                    const bool samePoint = !showLinkHover ||
                        (linkPoint.has_value() == _readingLinkPoint.has_value() && (!linkPoint ||
                            (linkPoint->X == _readingLinkPoint->X && linkPoint->Y == _readingLinkPoint->Y)));
                    return selectionStart == std::min(_selectionAnchor, _selectionActive) &&
                        selectionEnd == std::max(_selectionAnchor, _selectionActive) &&
                        focusedAction == _focusedAction && readingFocused == _readingFocused &&
                        inactiveCaret == _showInactiveCaretMarker && trailingCaret == _readingCaretTrailing &&
                        samePoint && inspection == _diagramInspection &&
                        highlights == currentHighlights();
                };
                const bool samePaint = retainedBand && previouslyReady &&
                    retainedBand->SameSourceLayout(*candidate.frame, candidate.geometry) &&
                    retainedBand->selectionStart == selectionStart && retainedBand->selectionEnd == selectionEnd &&
                    retainedBand->focusedAction == _focusedAction && retainedBand->focused == _readingFocused &&
                    retainedBand->inactiveCaret == _showInactiveCaretMarker &&
                    retainedBand->trailingCaret == trailingCaret &&
                    retainedBand->hoveredLink == candidate.hoveredLink;
                const bool scrolling = samePaint && _presented.frame &&
                    candidate.frame->viewportY != _presented.frame->viewportY &&
                    ::Notepads::Markdown::Native::QuantizeViewportY(_presented.frame->viewportY,
                        candidate.geometry.scale, previousRow, snappedY);
                bool simpleTemplate{};
                if (bandPlan && scrolling)
                {
                    try { simpleTemplate = RasterBandTemplate(_surfaceTarget, _surfaceBrush); }
                    catch (...) { /* Optional retention never makes a supported paint fail. */ }
                }
                const bool moveRaster = bandPlan && scrolling && simpleTemplate && _surface && _surfaceNative &&
                    retainedBand->SameContent(*candidate.frame, candidate.geometry) &&
                    _d3dDevice && SUCCEEDED(_d3dDevice->GetDeviceRemovedReason()) &&
                    retainedBand->band.Contains(pixelRow, pixelWidth, pixelHeight) &&
                    ::Notepads::Markdown::Native::ViewportRasterTranslationCompatible(
                        pixelRow - retainedBand->band.firstRow, candidate.geometry.scale) &&
                    staticBand(retainedBand->band);
                auto paintFrame = candidate.frame;
                std::shared_ptr<PreviewRasterBandState> nextBand;
                int64_t rasterFirstRow = pixelRow;
                double rasterHeight = candidate.geometry.surfaceHeight;
                if (moveRaster)
                {
                    rasterFirstRow = retainedBand->band.firstRow;
                    pixelHeight = retainedBand->band.pixelHeight;
                    rasterHeight = pixelHeight / candidate.geometry.scale;
                    paintFrame = retainedBand->frame;
                    drawingNative = _surfaceNative;
                }
                else
                {
                    _previewRasterBand.reset();
                    // Initial, semantic, overlay and large-jump paints retain the
                    // original viewport work. Only a subsequent small scroll may
                    // consume the Frame's already measured lookahead buffer.
                    if (bandPlan && mayExpand && scrolling && simpleTemplate &&
                        std::abs(static_cast<double>(pixelRow) - static_cast<double>(previousRow)) <=
                            candidate.geometry.pixelHeight / 2.0)
                    {
                        try
                        {
                            auto expanded = ::Notepads::Markdown::Native::PlanViewportRasterBand(candidate.geometry,
                                pixelRow, _padding, candidate.frame->measuredTop, candidate.frame->measuredBottom,
                                candidate.frame->totalHeight, true);
                            if (expanded && expanded->pixelHeight > pixelHeight && staticBand(*expanded))
                            {
                                auto frozen = candidate.frame->Reposition(expanded->firstRow / candidate.geometry.scale,
                                    expanded->pixelHeight / candidate.geometry.scale);
                                if (frozen)
                                {
                                    paintFrame = std::move(frozen);
                                    bandPlan = expanded;
                                    rasterFirstRow = expanded->firstRow;
                                    pixelHeight = expanded->pixelHeight;
                                    rasterHeight = pixelHeight / candidate.geometry.scale;
                                    optionalSurface = true;
                                }
                            }
                        }
                        catch (...) { /* Budget exhaustion falls back to one viewport. */ }
                    }
                    if (!_d3dDevice)
                    {
                        stage = L"graphics.d3d_create";
                        auto lease = sharedD3dDevice.lock();
                        if (!lease || FAILED(lease->GetDeviceRemovedReason()))
                        {
                            com_ptr<ID3D11Device> device;
                            auto created = ::D3D11CreateDevice(nullptr, D3D_DRIVER_TYPE_HARDWARE, nullptr,
                                                              D3D11_CREATE_DEVICE_BGRA_SUPPORT, nullptr, 0, D3D11_SDK_VERSION,
                                                              device.put(), nullptr, nullptr);
                            if (FAILED(created))
                                created = ::D3D11CreateDevice(nullptr, D3D_DRIVER_TYPE_WARP, nullptr,
                                                             D3D11_CREATE_DEVICE_BGRA_SUPPORT, nullptr, 0, D3D11_SDK_VERSION,
                                                             device.put(), nullptr, nullptr);
                            check_hresult(created);
                            lease = std::shared_ptr<ID3D11Device>(device.detach(),
                                [](ID3D11Device *value) noexcept { value->Release(); });
                            sharedD3dDevice = lease;
                        }
                        _d3dDevice.copy_from(lease.get());
                        _d3dLease = std::move(lease);
                        stage = L"graphics.factory";
                        EnsureGraphicsFactory();
                        stage = L"graphics.d2d_create";
                        com_ptr<ID2D1Device> d2dDevice;
                        check_hresult(_d2dFactory->CreateDevice(_d3dDevice.as<IDXGIDevice>().get(), d2dDevice.put()));
                        _d2dDevice = std::move(d2dDevice);
                    }
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                    traceStage(UiTrace::Phase::UiRasterSurface, UiTrace::Phase::UiRasterSurfaceEnd);
#endif
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                    UiTrace::UiCallBatch<5> surfaceCalls{traceLayout};
                    stage = L"surface.create";
                    surfaceCalls.Measure(UiTrace::Phase::UiRasterSurfaceNativeRelease,
                        UiTrace::Phase::UiRasterSurfaceNativeReleaseEnd, [&] { _surfaceNative = nullptr; });
                    surfaceCalls.Measure(UiTrace::Phase::UiRasterSurfaceCreateReplace,
                        UiTrace::Phase::UiRasterSurfaceCreateReplaceEnd,
                        [&] { _surface = SurfaceImageSource(pixelWidth, pixelHeight, false); });
                    stage = L"surface.interface";
                    surfaceCalls.Measure(UiTrace::Phase::UiRasterSurfaceInterfaceCopy,
                        UiTrace::Phase::UiRasterSurfaceInterfaceCopyEnd, [&]
                        {
                            _surfaceNative = _surface.as<ISurfaceImageSourceNativeWithD2D>();
                            drawingNative = _surfaceNative;
                        });
                    stage = L"surface.set_device";
                    surfaceCalls.Measure(UiTrace::Phase::UiRasterSurfaceSetDevice,
                        UiTrace::Phase::UiRasterSurfaceSetDeviceEnd,
                        [&] { check_hresult(_surfaceNative->SetDevice(_d2dDevice.get())); });
                    RECT rectangle{0, 0, pixelWidth, pixelHeight};
                    POINT offset{};
                    com_ptr<ID2D1DeviceContext> context;
                    stage = L"surface.begin_draw";
                    surfaceCalls.Measure(UiTrace::Phase::UiRasterSurfaceBeginDraw,
                        UiTrace::Phase::UiRasterSurfaceBeginDrawEnd, [&]
                        {
                            check_hresult(
                                drawingNative->BeginDraw(rectangle, __uuidof(ID2D1DeviceContext), context.put_void(), &offset));
                        });
                    drawing = true;
                    surfaceCalls.Flush();
#else
                    stage = L"surface.create";
                    _surfaceNative = nullptr;
                    _surface = SurfaceImageSource(pixelWidth, pixelHeight, false);
                    stage = L"surface.interface";
                    _surfaceNative = _surface.as<ISurfaceImageSourceNativeWithD2D>();
                    drawingNative = _surfaceNative;
                    stage = L"surface.set_device";
                    check_hresult(_surfaceNative->SetDevice(_d2dDevice.get()));
                    RECT rectangle{0, 0, pixelWidth, pixelHeight};
                    POINT offset{};
                    com_ptr<ID2D1DeviceContext> context;
                    stage = L"surface.begin_draw";
                    check_hresult(
                        drawingNative->BeginDraw(rectangle, __uuidof(ID2D1DeviceContext), context.put_void(), &offset));
                    drawing = true;
#endif
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                    traceStage(UiTrace::Phase::UiRasterPaint, UiTrace::Phase::UiRasterPaintEnd);
#endif
                    stage = L"frame.draw";
                    context->SetDpi(96 * scale, 96 * scale);
                    context->SetTransform(D2D1::Matrix3x2F::Translation(offset.x / scale, offset.y / scale));
                    auto color = [](Windows::UI::Color value)
                    { return D2D1::ColorF(value.R / 255.0f, value.G / 255.0f, value.B / 255.0f, value.A / 255.0f); };
                    context->Clear(color(candidate.frame->theme.background));
                    std::function<void()> repaintText;
                    if (candidate.frame->textPrewarm)
                    {
                        repaintText = [control = get_weak(), dispatcher = Dispatcher(),
                                       expected = std::weak_ptr{candidate.frame->textPrewarm}]
                        {
                            (void)dispatcher.TryRunAsync(Windows::UI::Core::CoreDispatcherPriority::Normal,
                                [control, expected]
                                {
                                    auto self = control.get();
                                    auto store = expected.lock();
                                    if (store && self && self->IsPreviewLoaded() && self->_presented.frame &&
                                        self->_presented.frame->textPrewarm == store)
                                        (void)self->DrawPreview();
                                });
                        };
                    }
                    if (_presented.frame)
                        paintFrame->TakeDiagramDrawingFrom(*_presented.frame);
                    ::Notepads::Markdown::Native::DiagramInspectionPlacement inspectionPlacement;
                    if (inspection && DrawDiagramInspection(*inspection, *paintFrame, context.get(),
                            candidate.geometry, inspectionPlacement))
                        candidate.inspection = PresentedDiagramInspection{*inspection, inspectionPlacement};
                    else
                    {
                        candidate.inspection.reset();
                        if (inspection && inspection == _diagramInspection)
                            _diagramInspection.reset();
                        ::Notepads::Markdown::Native::NativeDocumentRenderer::Draw(
                            *paintFrame, context.get(), _padding + static_cast<float>(candidate.geometry.phaseX),
                        _padding + static_cast<float>(candidate.geometry.phaseY),
                        std::min(_selectionAnchor, _selectionActive), std::max(_selectionAnchor, _selectionActive),
                        _focusedAction, _readingFocused, &_imageDrawCache, _showInactiveCaretMarker, candidate.hoveredLink,
                        D2D1::RectF(0, 0, pixelWidth / scale, pixelHeight / scale), &_textBitmapCache, &repaintText,
                        !_previewUpdate || !_previewUpdate->incrementalEdit, highlights, trailingCaret);
                    }
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                    traceStage(UiTrace::Phase::UiRasterCommit, UiTrace::Phase::UiRasterCommitEnd);
#endif
                    stage = L"surface.end_draw";
                    auto status = drawingNative->EndDraw();
                    drawing = false;
                    check_hresult(status);
                    optionalSurface = false;
                    if (bandPlan)
                    {
                        try
                        {
                            nextBand = std::make_shared<PreviewRasterBandState>();
                            nextBand->frame = paintFrame;
                            nextBand->band = *bandPlan;
                            nextBand->geometry = candidate.geometry;
                            nextBand->selectionStart = selectionStart;
                            nextBand->selectionEnd = selectionEnd;
                            nextBand->focusedAction = _focusedAction;
                            nextBand->focused = _readingFocused;
                            nextBand->inactiveCaret = _showInactiveCaretMarker;
                            nextBand->trailingCaret = trailingCaret;
                            nextBand->hoveredLink = candidate.hoveredLink;
                            nextBand->mayExpand = mayExpand;
                        }
                        catch (...) { /* Retention is optional after a successful paint. */ }
                    }
                }
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                traceStage(UiTrace::Phase::UiRasterAttach, UiTrace::Phase::UiRasterAttachEnd);
#endif
                stage = L"surface.attach";
                // A DP setter can synchronously call the host and replace the
                // source, template or surface. Keep this attachment local and
                // validate ownership after every such boundary.
                auto target = _surfaceTarget;
                auto overlay = _viewportOverlay;
                auto brush = _surfaceBrush;
                auto surface = _surface;
                auto native = _surfaceNative;
                auto ownsAttachment = [&]
                {
                    return _previewSurfaceSerial == attachmentSerial && target && overlay && brush &&
                           _surfaceTarget == target && _viewportOverlay == overlay &&
                           _surfaceBrush == brush && _surface == surface && _surfaceNative.get() == native.get() &&
                           CurrentViewportFrame(*candidate.frame, candidate.geometry);
                };
                auto current = [&] { return ownsAttachment() && samePaintState(); };
                auto withdraw = [&]() noexcept
                {
                    try
                    {
                        // Never erase a newer reentrant draw on this same brush.
                        if (_previewSurfaceSerial == attachmentSerial && brush && brush.ImageSource() == surface)
                            brush.ImageSource(nullptr);
                    }
                    catch (...)
                    {
                    }
                    if (_previewSurfaceSerial == attachmentSerial && _surface == surface && _surfaceBrush == brush)
                        _surfaceReady = false;
                };
                auto interrupted = [&]() -> bool
                {
                    // A public DP callback may alter an overlay while painting
                    // is unavailable. Settle it with one bounded real repaint.
                    if (ownsAttachment() && !samePaintState() && !paintStateRetried)
                    {
                        paintStateRetried = true;
                        _previewRasterBand.reset();
                        return retry(retry);
                    }
                    withdraw();
                    return false;
                };
                TranslateTransform placement;
                placement.X(candidate.geometry.surfaceX);
                placement.Y(candidate.geometry.surfaceY -
                    static_cast<double>(pixelRow - rasterFirstRow) / candidate.geometry.scale);
                RectangleGeometry clip;
                clip.Rect(
                    {static_cast<float>(candidate.geometry.overlayX), static_cast<float>(candidate.geometry.overlayY),
                     static_cast<float>(candidate.geometry.width), static_cast<float>(candidate.geometry.height)});
                try
                {
                    if (!current())
                        return interrupted();
                    target.Width(candidate.geometry.surfaceWidth);
                    if (!current())
                        return interrupted();
                    target.Height(rasterHeight);
                    if (!current())
                        return interrupted();
                    target.RenderTransform(placement);
                    if (!current())
                        return interrupted();
                    overlay.Clip(clip);
                    if (!current())
                        return interrupted();
                    brush.ImageSource(surface);
                    if (!current())
                        return interrupted();
                }
                catch (...)
                {
                    if (!current())
                        return interrupted();
                    throw;
                }
                // Accessibility observes outcomes from the pixels actually on
                // screen, including a cached diagram failure/fallback result.
                if (paintFrame != candidate.frame)
                    for (size_t index = 0; index < candidate.frame->diagramDrawOutcomes.size(); ++index)
                        candidate.frame->diagramDrawOutcomes[index].status.store(
                            paintFrame->diagramDrawOutcomes[index].status.load(std::memory_order_acquire),
                            std::memory_order_release);
                _previewRasterBand = moveRaster ? retainedBand : std::move(nextBand);
                _surfaceReady = true;
                _previewDeviceRetry = false;
                if (candidate.frame->navigation)
                {
                    _measuredWidth = candidate.frame->key.width;
                    _measuredHeight = candidate.frame->totalHeight;
                }
                _renderingError.clear();
                return true;
            }
            catch (...)
            {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                traceStage(UiTrace::Phase::UiRasterRecovery, UiTrace::Phase::UiRasterRecoveryEnd);
#endif
                HRESULT code = to_hresult();
                const auto failedLease = _d3dLease;
                DrawFault fault{stage, true, code};
                // Capture from the actual failing device before cleanup/release.
                // S_OK is retained as an observation, not proof against device loss.
                if (_d3dDevice)
                {
                    fault.hasDeviceReason = true;
                    fault.deviceReason = _d3dDevice->GetDeviceRemovedReason();
                }
                if (drawing)
                {
                    fault.hasCleanup = true;
                    fault.cleanup = drawingNative->EndDraw();
                }
                // A newer reentrant attachment owns its graphics and status.
                if (attachmentSerial && _previewSurfaceSerial != attachmentSerial)
                    return false;
                const bool deviceFailure = code == D2DERR_RECREATE_TARGET || code == DXGI_ERROR_DEVICE_REMOVED ||
                    code == DXGI_ERROR_DEVICE_RESET || code == DXGI_ERROR_DEVICE_HUNG || code == DXGI_ERROR_DRIVER_INTERNAL_ERROR ||
                    (fault.hasDeviceReason && FAILED(fault.deviceReason));
                if (optionalSurface && !bandAllocationRetried && !deviceFailure)
                {
                    // Extra pixels are optional. Retry the original H once,
                    // without releasing a healthy device or growing this layout
                    // again after the allocation failure. A failed H paint still
                    // follows the original recovery/terminal path below.
                    bandAllocationRetried = true;
                    _previewRasterBand.reset();
                    _surfaceNative = nullptr;
                    _surface = nullptr;
                    return retry(retry);
                }
                if (failedLease && deviceFailure && sharedD3dDevice.lock() == failedLease)
                    sharedD3dDevice.reset();
                ReleaseGraphics();
                remember(fault);
                if (!_previewDeviceRetry && deviceFailure)
                {
                    if ((static_cast<uint32_t>(_acceptedExtensions) & 3u) != 0)
                    {
                        // Preserve this failure before the asynchronous refresh:
                        // this call returns false and its first receipt stays Failed.
                        _previewDeviceRetry = true;
                        _derivedLayoutDeviceRecovery = true;
                        QueueDerivedLayoutRefresh();
                        return false;
                    }
                    // Same recovery count/Frame as before; stack records retain both
                    // failures even if the second attempt exits at a preflight guard.
                    _previewDeviceRetry = true;
                    auto retried = retry(retry);
                    _previewDeviceRetry = false;
                    return retried;
                }
                _lastStatus = Markdown::RenderingStatus::RenderFailure;
                return false;
            }
        };
        return draw(draw);
    }

    void MarkdownView::SetTheme(Markdown::MarkdownTheme const &theme)
    {
        CheckUiThread();
        if (!theme)
            throw hresult_invalid_argument();
        auto generation = theme.Generation();
        auto font = theme.FontFamily();
        auto code = theme.CodeFontFamily();
        auto size = theme.FontSize();
        auto line = theme.LineHeight();
        auto padding = theme.Padding();
        if (generation <= _themeGeneration || font.empty() || font.size() > 256 || code.empty() || code.size() > 256 ||
            !std::isfinite(size) || size < 6 || size > 144 || !std::isfinite(line) || line < size || line > 512 ||
            !std::isfinite(padding) || padding < 0 || padding > 256)
            throw hresult_invalid_argument();
        ::Notepads::Markdown::Native::PreviewTheme frozen;
        frozen.fontFamily = font.c_str();
        frozen.codeFontFamily = code.c_str();
        frozen.fontSize = static_cast<float>(size);
        frozen.lineHeight = static_cast<float>(line);
        frozen.padding = static_cast<float>(padding);
        frozen.foreground = theme.Foreground();
        frozen.background = theme.Background();
        frozen.linkForeground = theme.LinkForeground();
        frozen.selectionForeground = theme.SelectionForeground();
        frozen.selectionBackground = theme.SelectionBackground();
        frozen.borderColor = theme.BorderColor();
        frozen.panelBackground = theme.PanelBackground();
        frozen.focusColor = theme.FocusColor();
        frozen.codeForeground = {frozen.foreground,
                                 theme.CodeCommentForeground(),
                                 theme.CodeStringForeground(),
                                 theme.CodeNumberForeground(),
                                 theme.CodeKeywordForeground(),
                                 theme.CodeTypeForeground(),
                                 theme.CodeFunctionForeground()};
        frozen.ApplyDiffColors();
        CaptureImageAnchorBeforeChange();
        _readingTheme = std::move(frozen);
        ClearMeasurementCache();
        _themeGeneration = generation;
        _fontFamily = font;
        _fontSize = static_cast<float>(size);
        _lineHeight = static_cast<float>(line);
        _padding = static_cast<float>(padding);
        _foreground = theme.Foreground();
        _background = theme.Background();
        ++_generation;
        UpdateEffectiveTheme();
    }
    void MarkdownView::UpdateEffectiveTheme()
    {
        if (_effectiveThemeGeneration == UINT64_MAX)
            throw hresult_error(E_UNEXPECTED);
        CaptureImageAnchorBeforeChange();
        ++_effectiveThemeGeneration;
        ApplyEffectiveColors();
        ReconcilePictureSources();
        if (_documentAccepted && _previewModelReady)
            StartPreviewLayout();
        else if (!_documentAccepted)
            DrawCurrent();
    }

    void MarkdownView::ApplyEffectiveColors()
    {
        if (_accessibility && _accessibility.HighContrast() && _systemColors)
        {
            using Element = Windows::UI::ViewManagement::UIElementType;
            _foreground = _systemColors.UIElementColor(Element::WindowText);
            _background = _systemColors.UIElementColor(Element::Window);
        }
        else
        {
            _foreground = _readingTheme.foreground;
            _background = _readingTheme.background;
        }
        if (_previewScroll)
            _previewScroll.Background(SolidColorBrush(_background));
    }
    MarkdownView::MarkdownView()
    {
        if (notepads_markdown_core_abi_version() != NOTEPADS_MARKDOWN_CORE_ABI_VERSION)
        {
            throw hresult_error(E_FAIL, L"Notepads.Markdown private ABI mismatch.");
        }
        _owningDispatcher = Dispatcher();
        DefaultStyleKey(box_value(L"Notepads.Markdown.MarkdownView"));
        _loadedRevoker = Loaded(auto_revoke, {this, &MarkdownView::OnLoaded});
        _unloadedRevoker = Unloaded(auto_revoke, {this, &MarkdownView::OnUnloaded});
        _sizeRevoker = SizeChanged(auto_revoke, {this, &MarkdownView::OnSizeChanged});
        _previewLayoutRevoker = LayoutUpdated(auto_revoke, {this, &MarkdownView::OnPreviewLayoutUpdated});
        _readingPressedRevoker = PointerPressed(auto_revoke, {this, &MarkdownView::OnReadingPointerPressed});
        _readingMovedRevoker = PointerMoved(auto_revoke, {this, &MarkdownView::OnReadingPointerMoved});
        _readingEnteredRevoker = PointerEntered(auto_revoke, [this](auto const &sender, auto const &args)
        {
            if (!_dragging)
                OnReadingPointerMoved(sender, args);
        });
        _readingExitedRevoker = PointerExited(auto_revoke, [this](auto const &, auto const &args)
        {
            if (args.Pointer().PointerDeviceType() == Windows::Devices::Input::PointerDeviceType::Mouse)
            {
                _readingLinkPoint.reset();
                UpdateReadingLinkCursor();
            }
            ClearReadingHover();
            auto point = args.GetCurrentPoint(*this).Position();
            if (point.X < 0 || point.Y < 0 || point.X >= ActualWidth() || point.Y >= ActualHeight())
                _codeCopyPointer.reset();
            UpdateCodeCopyButton();
        });
        _readingReleasedRevoker = PointerReleased(auto_revoke, {this, &MarkdownView::OnReadingPointerReleased});
        _readingCaptureRevoker = PointerCaptureLost(auto_revoke, {this, &MarkdownView::OnReadingPointerCaptureLost});
        _readingKeyRevoker = KeyDown(auto_revoke, {this, &MarkdownView::OnReadingKeyDown});
        _readingGotFocusRevoker = GotFocus(auto_revoke, {this, &MarkdownView::OnReadingFocusChanged});
        _readingLostFocusRevoker = LostFocus(auto_revoke, {this, &MarkdownView::OnReadingFocusChanged});
    }

    MarkdownView::~MarkdownView()
    {
        // A worker owns native state and a module guard, never this XAML object.
        // No worker join occurs during UI destruction.
        StopImageFonts(true);
        try
        {
            StopPreview(Markdown::DocumentUpdateStatus::NotLoaded, true, false);
        }
        catch (...)
        {
            // Final destruction must not propagate a late teardown failure.
        }
    }

    fire_and_forget MarkdownView::final_release(std::unique_ptr<MarkdownView> self) noexcept
    {
        // Keep the DLL loaded even after the UI callback deletes the object and
        // its native state. No continuation targets a closing apartment.
        ::Notepads::Markdown::Native::ModuleLifetime module;
        try
        {
            auto dispatcher = self->_owningDispatcher;
            if (!dispatcher || dispatcher.HasThreadAccess())
            {
                self->StopPreview(Markdown::DocumentUpdateStatus::NotLoaded, true, false);
                self->ReleaseGraphics();
                co_return;
            }
            auto holder = std::make_shared<std::unique_ptr<MarkdownView>>(std::move(self));
            co_await resume_background();
            (void)co_await resume_agile(dispatcher.TryRunAsync(Windows::UI::Core::CoreDispatcherPriority::Normal,
                                                               [holder]
                                                               {
                                                                   auto deleting = std::move(*holder);
                                                                   deleting->StopPreview(Markdown::DocumentUpdateStatus::NotLoaded, true, false);
                                                                   deleting->ReleaseGraphics();
                                                               }));
            // If dispatch was refused, the Closed handler already detached SIS
            // on the UI thread. The remaining fields need only COM Release.
        }
        catch (...)
        {
            // A closed dispatcher cannot accept graphics calls. Self/holder
            // still release their native ownership without an apartment wait.
        }
    }

    void MarkdownView::CheckUiThread() const
    {
        if (!Dispatcher().HasThreadAccess())
        {
            throw hresult_wrong_thread();
        }
    }

    IAsyncOperation<Markdown::DocumentUpdateResult> MarkdownView::ResumeCachedDocumentAsync(guid document,
                                                                                            uint64_t revision)
    {
        CheckUiThread();
        bool matches = _cachedPreviewModel && document == _acceptedDocument && revision == _acceptedRevision;
        if (matches && (_cachedPresentationSuspended || !_loaded))
        {
            const auto sourceGeneration = _previewSourceGeneration;
            (void)ReopenCachedPresentation();
            matches = _cachedPreviewModel && sourceGeneration == _previewSourceGeneration &&
                      document == _acceptedDocument && revision == _acceptedRevision;
        }
        // Layout can retry a NotLoaded receipt independently. Join that work
        // if it is in flight, or return its success if it already published.
        if (matches && _previewUpdate &&
            (_previewUpdate->state.load(std::memory_order_acquire) != 3 ||
             _previewUpdate->status == Markdown::DocumentUpdateStatus::Ready))
            return AwaitPreview(_previewUpdate);

        auto update = std::make_shared<PreviewUpdate>();
        update->document = document;
        update->revision = revision;
        update->generation = _generation;
        update->sourceGeneration = _previewSourceGeneration;
        if (!matches)
        {
            update->Complete(Markdown::DocumentUpdateStatus::ResyncRequired, NOTEPADS_MARKDOWN_CORE_RESYNC_REQUIRED);
        }
        else if (!IsPreviewLoaded())
        {
            update->Complete(Markdown::DocumentUpdateStatus::NotLoaded);
        }
        else
        {
            _previewUpdate = update;
            _previewModelReady = true;
            StartPreviewLayout();
            if (_previewUpdate == update && _viewportTemplateState == ViewportTemplateState::Unsupported)
                update->Complete(Markdown::DocumentUpdateStatus::Failed, E_INVALIDARG);
        }
        return AwaitPreview(std::move(update));
    }

    void MarkdownView::SuspendCachedDocument()
    {
        CheckUiThread();
        if (_cachedPresentationSuspended)
            return;
        _cachedPresentationSuspended = true;
        // Use the same snapshot, drain, template and graphics teardown as a real
        // detach. Non-null sender preserves a model when caching is enabled.
        OnUnloaded(get_strong().as<IInspectable>(), nullptr);
    }

    void MarkdownView::DiscardCachedDocument()
    {
        CheckUiThread();
        auto const source = _acceptedSource;
        SuspendCachedDocument();
        if (!_cachedPresentationSuspended || _acceptedSource != source)
            return;
        const auto generation = _generation;
        StopPreview(Markdown::DocumentUpdateStatus::Canceled);
        if (_generation != generation || _acceptedSource != source || !_cachedPresentationSuspended)
            return;
        _automationSnapshot.reset();
        _cachedPreviewModel.Reset();
        _acceptedSource.reset();
        _acceptedDocument = {};
        _acceptedRevision = 0;
        _documentAccepted = false;
        _previewModelReady = false;
        _previewSourceState = {};
        _editChannelNeedsResync = true;
        _viewportToRestore.reset();
        ClearMeasurementCache();
        _measurementCache.reset();
        _measurementCacheAttempted = false;
        _closedRevoker.revoke();
        ReleaseGraphics();
    }

    uint64_t MarkdownView::GetRetainedByteEstimate()
    {
        CheckUiThread();
        try
        {
            uint64_t total{};
            auto add = [&](uint64_t bytes)
            {
                if (bytes > UINT64_MAX - total)
                    throw hresult_error(E_BOUNDS);
                total += bytes;
            };
            if (_acceptedSource)
                add(_acceptedSource->capacity());
            if (_images && _images->reuse)
            {
                // Cache metadata may be funded by an older root than the
                // installed model. Add it independently even with no model;
                // this deliberately overestimates if the current root also
                // includes it. 8 KiB covers this fixed 32-slot native block
                // and original receipt overhead on the supported layouts.
                add((std::max)(uint64_t{8192}, _images->reuse->Inspect().metadataRequestedBytes));
            }
            ::Notepads::Markdown::Native::CoreModelPin installed;
            auto model = _cachedPreviewModel.Get();
            if (!model && _previewController)
            {
                installed = _previewController->PinInstalled();
                model = installed.Get();
            }
            if (model)
            {
                notepads_markdown_core_budget_usage_result usage{};
                usage.header = {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(usage)};
                notepads_markdown_core_error error{};
                error.header = {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(error)};
                if (notepads_markdown_core_model_budget_usage(model, &usage, &error) != 0)
                    return UINT64_MAX;
                // One root query counts source/model/native grants once even
                // when controller, cached model and reading state alias it.
                add(usage.source_capacity);
                add(usage.auxiliary_capacity);
            }
            else if (_acceptedSource || _previewDrain)
                return UINT64_MAX;
            if (_measurementCache)
            {
                add(_measurementCache->Stats().allocated_bytes);
                add(_measurementCache->NativeBytes());
            }
            return total;
        }
        catch (...)
        {
            return UINT64_MAX;
        }
    }

    bool MarkdownView::ReopenCachedPresentation()
    {
        if (_loaded && !_cachedPresentationSuspended)
            return IsPreviewLoaded();
        if (!IsLoaded() || Visibility() != Windows::UI::Xaml::Visibility::Visible || _generation == UINT64_MAX)
            return false;
        auto const source = _acceptedSource;
        const auto generation = _generation;
        const bool suspended = std::exchange(_cachedPresentationSuspended, false);
        try
        {
            OnLoaded(nullptr, nullptr);
        }
        catch (...)
        {
            // A failed attachment cannot leave a hidden model rendering. Do not
            // tear down a newer source admitted by a reentrant template callback.
            if (_acceptedSource == source && _generation - generation <= 1)
            {
                _cachedPresentationSuspended = suspended;
                try { OnUnloaded(get_strong().as<IInspectable>(), nullptr); }
                catch (...) { ReleaseGraphics(); }
            }
            return false;
        }
        return IsPreviewLoaded();
    }

    void MarkdownView::Cancel()
    {
        CheckUiThread();
        if (_presented.hoveredLink && ReadingAvailable())
        {
            // Cancel keeps the last frame visible. Remove its transient hover decoration first.
            const auto generation = _generation;
            auto retained = _presented;
            DrawPreview(retained, false);
            if (_generation != generation)
                return;
        }
        _editChannelNeedsResync = true;
        ++_generation;
        _deferredRender = false;
        StopPreview(Markdown::DocumentUpdateStatus::Canceled);
        if (!_loaded && _cacheDocumentOnUnload)
            _closedRevoker.revoke();
        _lastStatus = Markdown::RenderingStatus::Canceled;
    }

    void MarkdownView::ScheduleCurrent()
    {
        if (!IsPreviewLoaded() || !_acceptedSource)
        {
            return;
        }
        if (_documentAccepted)
        {
            if (_previewModelReady)
            {
                StartPreviewLayout();
                return;
            }
            if (_previewSubmittedSourceGeneration == _previewSourceGeneration || !_surfaceTarget ||
                ActualWidth() <= 0 || ActualHeight() <= 0)
                return;
            auto update = _previewUpdate;
            bool sameSource = update && update->sourceGeneration == _previewSourceGeneration;
            uint32_t state = sameSource ? update->state.load(std::memory_order_acquire) : 3;
            if (sameSource && state != 0)
            {
                if (state != 3 || update->status == Markdown::DocumentUpdateStatus::Canceled)
                    return;
            }
            if (!sameSource || state == 3)
            {
                bool explicitFull = sameSource && update->status == Markdown::DocumentUpdateStatus::NotLoaded &&
                                    update->fullSynchronization;
                update = std::make_shared<PreviewUpdate>();
                update->document = _acceptedDocument;
                update->revision = _acceptedRevision;
                update->generation = _generation;
                update->sourceGeneration = _previewSourceGeneration;
                update->fullSynchronization = explicitFull;
                _previewUpdate = update;
            }
            BeginPreviewSource(update);
            return;
        }
    }

    void MarkdownView::RefreshDerivedLayout()
    {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        namespace UiTrace = ::Notepads::Markdown::Native::StartupTrace;
        UiTrace::UiScope uiTrace{UiTrace::Phase::UiDerivedRefresh, UiTrace::Phase::UiDerivedRefreshEnd, _previewLayoutGeneration};
#endif
        // An admitted layout can publish its immutable resource snapshot.
        // Give it one turn before replacing it with a newer derived batch.
        if (!_derivedLayoutDeviceRecovery)
        {
            if (HasActivePreviewLayout())
            {
                if (_viewportActive->math.generation != _mathGeneration ||
                    _viewportActive->diagram.generation != _diagramGeneration ||
                    _viewportActive->math.attempts != CaptureMath().attempts ||
                    _viewportActive->diagram.attempts != CaptureDiagram().attempts)
                    QueueDerivedLayoutRefresh();
                return;
            }
            auto const frame = _presented.frame;
            if (_previewRenderer && frame && _previewScroll && CurrentViewportFrame(*frame, _presented.geometry))
            {
                auto const math = CaptureMath();
                auto const diagram = CaptureDiagram();
                if (frame->math.generation == _mathGeneration && frame->diagram.generation == _diagramGeneration &&
                    frame->math.attempts == math.attempts && frame->diagram.attempts == diagram.attempts)
                {
                    _offscreenDerivedPendingAt.reset();
                    return;
                }

                // Geometry below the painted band can wait for a batch of
                // results. Check again on the next timer tick so scrolling to
                // the affected content can still publish at the normal cadence.
                constexpr uint64_t scrollQuietTimeMs = 80;
                constexpr uint64_t offscreenBatchIntervalMs = 80;
                if (frame->descriptors && frame->descriptors == _readingSource->Descriptors() &&
                    frame->navigation && frame->navigation->LineCount() &&
                    frame->math.key == math.key && frame->diagram.key == diagram.key)
                {
                    using ::Notepads::Markdown::Native::ReadingObjectKind;
                    auto const &objects = frame->descriptors->objects;
                    const auto paintBottom = _previewScroll.VerticalOffset() + frame->viewportHeight * 1.5;
                    // Match the per-kind ordinals assigned by BindMath and
                    // BindDiagrams in this same descriptor directory.
                    auto changed = [](auto const &before, auto const &after, uint32_t object, size_t ordinal)
                    {
                        if (ordinal >= after.attempts.size())
                            return false;
                        if (before.attempts[ordinal] != after.attempts[ordinal])
                            return true;
                        auto const oldHandle = before.presentations.Find(before.key, object);
                        auto const newHandle = after.presentations.Find(after.key, object);
                        return (oldHandle ? std::to_address(*oldHandle) : nullptr) !=
                               (newHandle ? std::to_address(*newHandle) : nullptr);
                    };
                    size_t mathOrdinal{}, diagramOrdinal{};
                    const auto affectsViewport = std::any_of(objects.begin(), objects.end(), [&](auto const &object)
                    {
                        bool dirty{};
                        if (math.enabled && object.kind == ReadingObjectKind::Math)
                            dirty = changed(frame->math, math, object.object, mathOrdinal++);
                        else if (diagram.enabled && object.kind == ReadingObjectKind::Diagram)
                            dirty = changed(frame->diagram, diagram, object.object, diagramOrdinal++);
                        if (!dirty)
                            return false;
                        auto const &line = frame->navigation->Line(frame->navigation->LineAt(object.text.start));
                        // Table cells can wrap to different line positions.
                        // Use the containing block, where a height change can begin.
                        return line.y - line.localY + frame->theme.padding <= paintBottom;
                    });
                    if (!affectsViewport)
                    {
                        const auto now = GetTickCount64();
                        if (!_offscreenDerivedPendingAt)
                        {
                            _offscreenDerivedPendingAt = now;
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                            if (!_isEmbeddedChild)
                                ::Notepads::Markdown::Native::StartupTrace::Mark(
                                    ::Notepads::Markdown::Native::StartupTrace::Phase::DerivedOffscreenBatch,
                                    frame->key.layout_generation);
#endif
                        }
                        if (now - *_offscreenDerivedPendingAt < offscreenBatchIntervalMs ||
                            now - _lastPreviewScrollAt < scrollQuietTimeMs)
                        {
                            QueueDerivedLayoutRefresh();
                            return;
                        }
                    }
                }
            }
        }
        if (_derivedLayoutDeviceRecovery)
        {
            _derivedLayoutDeviceRecovery = false;
            // A speculative initial draw has not claimed the host receipt.
            // The recovered full frame must settle that same pending update.
            if (!_previewUpdate || !_previewUpdate->Pending())
            {
                auto update = std::make_shared<PreviewUpdate>();
                update->document = _acceptedDocument;
                update->revision = _acceptedRevision;
                update->generation = _generation;
                update->sourceGeneration = _previewSourceGeneration;
                _previewUpdate = std::move(update);
            }
            _previewUpdate->surfaceRecovery = true;
        }
        _offscreenDerivedPendingAt.reset();
        StartPreviewLayout();
    }

    void MarkdownView::Refresh()
    {
        CheckUiThread();
        _viewportRediscover = true;
        _viewportRecovery = true;
        RequestImageFontRefresh();
        ClearMeasurementCache();
        if (_documentAccepted)
            StopPreview(Markdown::DocumentUpdateStatus::Superseded, false);
        ++_generation;
        ScheduleCurrent();
    }

    void MarkdownView::ApplyTheme(Windows::UI::Color const &foreground, Windows::UI::Color const &background,
                                  hstring const &fontFamily, double fontSize, double lineHeight, double padding,
                                  uint64_t generation)
    {
        CheckUiThread();
        if (generation <= _themeGeneration || fontFamily.empty() || fontFamily.size() > 256 ||
            !std::isfinite(fontSize) || fontSize < 6 || fontSize > 144 || !std::isfinite(lineHeight) ||
            lineHeight < fontSize || lineHeight > 512 || !std::isfinite(padding) || padding < 0 || padding > 256)
        {
            throw hresult_invalid_argument(L"Invalid or stale theme snapshot.");
        }
        CaptureImageAnchorBeforeChange();
        ClearMeasurementCache();
        _fontFamily = fontFamily;
        _fontSize = static_cast<float>(fontSize);
        _lineHeight = static_cast<float>(lineHeight);
        _padding = static_cast<float>(padding);
        _themeGeneration = generation;
        _readingTheme.fontFamily = fontFamily.c_str();
        _readingTheme.fontSize = _fontSize;
        _readingTheme.lineHeight = _lineHeight;
        _readingTheme.padding = _padding;
        _readingTheme.foreground = foreground;
        _readingTheme.background = background;
        bool dark = background.R + background.G + background.B < 384;
        _readingTheme.linkForeground =
            dark ? Windows::UI::Color{255, 128, 185, 255} : Windows::UI::Color{255, 8, 102, 200};
        _readingTheme.borderColor =
            dark ? Windows::UI::Color{255, 107, 114, 128} : Windows::UI::Color{255, 203, 208, 216};
        _readingTheme.panelBackground =
            dark ? Windows::UI::Color{255, 37, 40, 45} : Windows::UI::Color{255, 243, 245, 247};
        _readingTheme.focusColor = _readingTheme.linkForeground;
        _readingTheme.codeForeground = dark ? decltype(_readingTheme.codeForeground){foreground,
                                                                                Windows::UI::Color{255, 106, 153, 85},
                                                                                Windows::UI::Color{255, 206, 145, 120},
                                                                                Windows::UI::Color{255, 181, 206, 168},
                                                                                Windows::UI::Color{255, 86, 156, 214},
                                                                                Windows::UI::Color{255, 78, 201, 176},
                                                                                Windows::UI::Color{255, 220, 220, 170}}
                                            : decltype(_readingTheme.codeForeground){foreground,
                                                                                Windows::UI::Color{255, 90, 112, 81},
                                                                                Windows::UI::Color{255, 163, 21, 21},
                                                                                Windows::UI::Color{255, 9, 134, 88},
                                                                                Windows::UI::Color{255, 0, 0, 180},
                                                                                Windows::UI::Color{255, 38, 127, 153},
                                                                                Windows::UI::Color{255, 121, 94, 38}};
        _readingTheme.ApplyDiffColors();
        if (_effectiveThemeGeneration == UINT64_MAX)
            throw hresult_error(E_UNEXPECTED);
        ++_effectiveThemeGeneration;
        ApplyEffectiveColors();
        ReconcilePictureSources();
        ++_generation;
        if (_documentAccepted && _previewModelReady)
            StartPreviewLayout();
        else
            ScheduleCurrent();
        if (!_acceptedSource)
        {
            DrawCurrent();
        }
    }

    void MarkdownView::OnLoaded(IInspectable const &, RoutedEventArgs const &)
    {
        if (_loaded || _cachedPresentationSuspended)
            return;
        // A composable XAML control can have no Dispatcher during its native
        // constructor. Bind the actual owning UI dispatcher once attached.
        _owningDispatcher = Dispatcher();
        _loaded = true;
        _viewportRediscover = true;
        _viewportTemplateState = ViewportTemplateState::Waiting;
        AttachViewportTemplate();
        AttachWheelScrollEvents();
        LoadImageFonts();
        ++_generation;
        _accessibility = Windows::UI::ViewManagement::AccessibilitySettings();
        _systemColors = Windows::UI::ViewManagement::UISettings();
        auto notify = [weak = get_weak(), dispatcher = Dispatcher()](auto const &, auto const &)
        {
            try
            {
                dispatcher.TryRunAsync(Windows::UI::Core::CoreDispatcherPriority::Normal,
                                       [weak]
                                       {
                                           if (auto owner = weak.get())
                                               owner->UpdateEffectiveTheme();
                                       });
            }
            catch (...)
            {
            }
        };
        _highContrastRevoker = _accessibility.HighContrastChanged(auto_revoke, notify);
        _systemColorsRevoker = _systemColors.ColorValuesChanged(auto_revoke, notify);
        _imageMotionRevoker = _systemColors.AnimationsEnabledChanged(auto_revoke,
            [weak = get_weak(), dispatcher = Dispatcher()](auto const &, auto const &)
            {
                try
                {
                    dispatcher.TryRunAsync(Windows::UI::Core::CoreDispatcherPriority::Normal, [weak]
                    {
                        if (auto owner = weak.get(); owner && owner->_loaded)
                            owner->ResetImageAnimation();
                    });
                }
                catch (...) { /* The view may have closed before dispatch. */ }
            });
        auto visibilityChanged = [weak = get_weak()](auto const &, auto const &)
        {
            if (auto owner = weak.get())
                owner->ImageAnimationVisibilityChanged();
        };
        if (!_imageVisibilityToken)
            _imageVisibilityToken = RegisterPropertyChangedCallback(UIElement::VisibilityProperty(), visibilityChanged);
        auto window = Windows::UI::Core::CoreWindow::GetForCurrentThread();
        if (window)
            _imageWindowVisibilityRevoker = window.VisibilityChanged(auto_revoke, visibilityChanged);
        ImageAnimationVisibilityChanged();
        _displayInfo = Windows::Graphics::Display::DisplayInformation::GetForCurrentView();
        _dpiRevoker = _displayInfo.DpiChanged(auto_revoke, [weak = get_weak()](auto const &sender, auto const &args)
        {
            if (auto owner = weak.get())
                owner->OnDpiChanged(sender, args);
        });
        _surfaceContentsLostRevoker = CompositionTarget::SurfaceContentsLost(auto_revoke,
            [weak = get_weak()](auto const &, auto const &)
            {
                if (auto owner = weak.get())
                {
                    // SIS content loss need not remove the D3D device. Retained
                    // pixels cease to be live until a real draw succeeds again.
                    owner->_previewRasterBand.reset();
                    owner->_surfaceReady = false;
                    owner->_viewportFailed.reset();
                    try
                    {
                        if (owner->_loaded && owner->_documentAccepted && owner->_presented.frame)
                            (void)owner->DrawPreview();
                    }
                    catch (...) { /* Lifecycle recovery must not escape the XAML event. */ }
                }
            });
        if (window)
            _closedRevoker = window.Closed(auto_revoke,
                                                                        [weak = get_weak()](auto const &, auto const &)
                                                                        {
                                                                            if (auto owner = weak.get())
                                                                            {
                                                                                owner->OnUnloaded(nullptr, nullptr);
                                                                            }
                                                                        });
        UpdateEffectiveTheme();
        ScheduleCurrent();
        if (!_acceptedSource)
        {
            DrawCurrent();
        }
    }

    void MarkdownView::OnUnloaded(IInspectable const &sender, RoutedEventArgs const &)
    {
        if (_cacheDocumentOnUnload && !_loaded && sender)
            return;
        ::Notepads::Markdown::Native::CoreModelPin cached;
        if (sender && _cacheDocumentOnUnload)
        {
            cached = std::move(_cachedPreviewModel);
            if (!cached && _previewModelReady && _previewController && !_editChannelNeedsResync &&
                _previewSourceState.state_valid && !_previewSourceState.needs_resync &&
                _displayedDocument == _acceptedDocument && _displayedRevision == _acceptedRevision)
            {
                try { cached = _previewController->PinInstalled(); }
                catch (...) { /* A cache miss can use the normal source submission. */ }
            }
        }
        auto viewport = _viewportToRestore ? _viewportToRestore : _lastViewport;
        if (_imageAnchor && CurrentImageAnchor(*_imageAnchor) && _imageAnchorNavigation)
        {
            auto const &anchor = *_imageAnchor;
            const auto y = _imageAnchorNavigation->RevealY(static_cast<uint32_t>(anchor.offset)) + anchor.lineOffset;
            viewport = CachedViewport{static_cast<uint32_t>(anchor.offset),
                anchor.lineOffset - anchor.padding + _padding, y, anchor.atBottom, anchor.atomic};
        }
        _editChannelNeedsResync = true;
        DetachReadingScrollEvents();
        _loaded = false;
        _surfaceContentsLostRevoker.revoke();
        StopImageFonts();
        auto const source = _acceptedSource;
        const auto generation = ++_generation;
        StopPreview(Markdown::DocumentUpdateStatus::NotLoaded);
        // Pointer-capture/template callbacks can admit a newer lifecycle. Its
        // source and presentation own the control; never resurrect our old pin.
        if (_generation != generation || _acceptedSource != source || _loaded)
            return;
        _cachedPreviewModel = std::move(cached);
        if (_cachedPreviewModel)
            _viewportToRestore = viewport;
        if (_cacheDocumentOnUnload)
        {
            // Wait for the host to validate the editor stamp before restoring any old source.
            _previewSubmittedSourceGeneration = _previewSourceGeneration;
            _automationSnapshot.reset();
        }
        DetachViewportTemplate();
        _viewportTemplateState = ViewportTemplateState::Waiting;
        _dpiRevoker.revoke();
        _highContrastRevoker.revoke();
        _systemColorsRevoker.revoke();
        _imageMotionRevoker.revoke();
        _imageWindowVisibilityRevoker.revoke();
        if (auto token = std::exchange(_imageVisibilityToken, std::nullopt))
            UnregisterPropertyChangedCallback(UIElement::VisibilityProperty(), *token);
        _imageAnimationHiddenAt.reset();
        _accessibility = nullptr;
        _systemColors = nullptr;
        if (!_cachedPreviewModel)
            _closedRevoker.revoke();
        _displayInfo = nullptr;
        ReleaseGraphics();
    }

    void MarkdownView::OnSizeChanged(IInspectable const &, SizeChangedEventArgs const &)
    {
        ClearMeasurementCache();
        if (_documentAccepted)
        {
            if (!_previewModelReady)
                ScheduleCurrent();
            else
                OnPreviewLayoutUpdated(nullptr, nullptr);
            return;
        }
        RetryDeferredRender();
        DrawCurrent();
    }

    void MarkdownView::OnDpiChanged(Windows::Graphics::Display::DisplayInformation const &, IInspectable const &)
    {
        CaptureImageAnchorBeforeChange();
        CancelWheelScroll();
        ClearMeasurementCache();
        if (_documentAccepted)
        {
            StartPreviewLayout();
            return;
        }
        RetryDeferredRender();
        DrawCurrent();
    }

    void MarkdownView::OnApplyTemplate()
    {
        ClearNoteEmbeds();
        DetachReadingScrollEvents();
        DetachViewportTemplate();
        InvalidateViewportTemplate();
        RetireCodeCopyButton();
        base_type::OnApplyTemplate();
        ReleaseGraphics();
        DetachReadingScrollEvents();
        _previewScrollRevoker.revoke();
        _previewScroll = GetTemplateChild(L"PreviewScrollViewer").try_as<ScrollViewer>();
        _previewExtent = GetTemplateChild(L"DocumentExtent").try_as<Canvas>();
        _viewportOverlay = GetTemplateChild(L"ViewportOverlay").try_as<Canvas>();
        if (_previewScroll)
        {
            _previewScroll.Background(SolidColorBrush(_background));
            _previewScrollRevoker =
                _previewScroll.ViewChanged(auto_revoke, [weak = get_weak()](auto const &sender, auto const &args)
                {
                    if (auto owner = weak.get())
                        owner->OnPreviewViewChanged(sender, args);
                });
            AttachViewportTemplate();
            if (!_documentAccepted)
            {
                _previewScroll.VerticalScrollMode(ScrollMode::Disabled);
                _previewScroll.VerticalScrollBarVisibility(ScrollBarVisibility::Disabled);
            }
            else
            {
                _previewScroll.VerticalScrollMode(ScrollMode::Enabled);
                _previewScroll.VerticalScrollBarVisibility(ScrollBarVisibility::Visible);
            }
        }
        _surfaceTarget = GetTemplateChild(L"SurfaceTarget").try_as<Border>();
        _codeCopyTemplate = nullptr;
        if (_viewportOverlay)
        {
            auto resources = _viewportOverlay.Resources();
            auto key = box_value(L"CodeCopyButtonTemplate");
            if (resources && resources.HasKey(key))
                _codeCopyTemplate = resources.Lookup(key).try_as<DataTemplate>();
        }
        AttachWheelScrollEvents();
        if (_surfaceTarget)
        {
            _surfaceBrush = ImageBrush();
            _surfaceBrush.Stretch(Stretch::Fill);
            _surfaceBrush.AlignmentX(AlignmentX::Left);
            _surfaceBrush.AlignmentY(AlignmentY::Top);
            _surfaceTarget.Background(_surfaceBrush);
        }
        if (_documentAccepted)
            ScheduleCurrent();
        RetryDeferredRender();
        DrawCurrent();
    }

    void MarkdownView::RetryDeferredRender()
    {
        if (_deferredRender && _loaded && _surfaceTarget && ActualWidth() > 0 && ActualHeight() > 0)
        {
            ScheduleCurrent();
        }
    }

    void MarkdownView::ReleaseGraphics() noexcept
    {
        _previewRasterBand.reset();
        SetReadingLinkCursor(false);
        ++_mathFactoryEpoch;
        RetireDiagram();
        RetireMath();
        _imageDrawCache.Clear();
        _textBitmapCache.Clear();
        try
        {
            if ((!_loaded || !_readingSource) && _owningDispatcher && _owningDispatcher.HasThreadAccess())
            {
                try
                {
                    if (_readingDragTimer)
                        _readingDragTimer.Stop();
                }
                catch (...) {}
                try { ReleasePointerCaptures(); }
                catch (...) {}
                _readingPeers.reset();
            }
        }
        catch (...)
        {
        }
        _surfaceReady = false;
        if (_surfaceBrush)
        {
            try
            {
                _surfaceBrush.ImageSource(nullptr);
            }
            catch (...)
            {
            }
        }
        _surfaceNative = nullptr;
        _surface = nullptr;
        _diagramFontFallback = nullptr;
        _diagramFontCollection = nullptr;
        ++_diagramFontEpoch;
        _writeFactory = nullptr;
        _d2dDevice = nullptr;
        _d2dFactory = nullptr;
        _d3dDevice = nullptr;
        _d3dLease.reset();
    }

    bool MarkdownView::DrawCurrent()
    {
        // A document is rendered only through the revisioned preview pipeline.
        // Before the first document there is no frame or text surface to publish.
        if (!_documentAccepted)
        {
            _surfaceReady = false;
            return false;
        }
        return DrawPreview();
    }

    Windows::UI::Xaml::Automation::Peers::AutomationPeer MarkdownView::OnCreateAutomationPeer()
    {
        return make<implementation::MarkdownViewAutomationPeer>(*this);
    }

    std::shared_ptr<TextSnapshot const> MarkdownView::AutomationSnapshot() const
    {
        CheckUiThread();
        if (!_loaded)
        {
            throw hresult_error(UIA_E_ELEMENTNOTAVAILABLE);
        }
        if (!ReadingAvailable())
            throw hresult_error(UIA_E_ELEMENTNOTAVAILABLE);
        return _automationSnapshot;
    }

    void MarkdownView::CheckLiveRange(TextSnapshot const &snapshot) const
    {
        CheckUiThread();
        // The framework can become unloaded before our Unloaded handler runs.
        // A retained provider must stop accepting calls in that same window.
        if (!IsLoaded())
            throw hresult_error(UIA_E_ELEMENTNOTAVAILABLE);
        if (snapshot.main)
        {
            if (!ReadingAvailable() || snapshot.token != _automationSnapshot->token)
                throw hresult_error(UIA_E_ELEMENTNOTAVAILABLE);
            return;
        }
        if (!_loaded || snapshot.document != _displayedDocument || snapshot.revision != _displayedRevision ||
            snapshot.generation != _generation || snapshot.generation != _displayedGeneration)
        {
            throw hresult_error(UIA_E_ELEMENTNOTAVAILABLE);
        }
    }
}
