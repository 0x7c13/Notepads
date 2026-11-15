// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "MarkdownView.g.h"
#include "Core/ModuleLifetime.h"
#include "Rendering/NativeDocumentRenderer.h"
#include "Rendering/NativeTextBitmapCache.h"
#include "Reading/NativeReadingSearch.h"
#include "Resources/Images/NativeImageCoordinator.h"
#include "Resources/Math/NativeMathCoordinator.h"
#include "Resources/Diagrams/NativeDiagramCoordinator.h"
#include "Control/ViewportPresentation.h"
#include "Control/NativeWheelScroll.h"
#include <dwrite_2.h>
#include <winrt/Windows.UI.Core.h>
#include <winrt/Windows.UI.Xaml.Input.h>
#include <winrt/Windows.UI.Xaml.Controls.Primitives.h>
#include <winrt/Windows.UI.ViewManagement.h>
#include <algorithm>
#include <array>
#include <optional>

namespace Notepads::Markdown::Native
{
    struct PreviewRenderRequest;
    struct PreviewRenderQueue;
    class NativeMeasurementCache;
    struct ImageFontControlTestHooks;
}

namespace winrt::Notepads::Markdown::implementation
{
    struct TextSnapshot
    {
        hstring text;
        guid document;
        uint64_t revision;
        uint64_t generation;
        uint64_t token;
        std::shared_ptr<::Notepads::Markdown::Native::MainTextSnapshot const> main;
        std::shared_ptr<::Notepads::Markdown::Native::SemanticDescriptors const> descriptors;
        std::optional<::Notepads::Markdown::Native::ReadingSection> section;
        ::Notepads::Markdown::Native::ReadingSpan Bounds() const noexcept
        {
            return section ? section->text : ::Notepads::Markdown::Native::ReadingSpan{0, static_cast<uint32_t>(View().size())};
        }
        std::wstring_view View() const noexcept
        {
            return main ? main->Text() : std::wstring_view(text.c_str(), text.size());
        }
        hstring Export(uint32_t start, uint32_t length) const;
    };

    struct PreviewUpdate;
    struct PreviewDrain;
    struct PreviewRasterBandState;
    struct PreviewHighlightWork;
    struct ReadingPeerCache;
    struct ReadingObjectProvider;
    struct ImageFontWatch;
    struct ImageFontControlTestAccess;

    struct MarkdownView : MarkdownViewT<MarkdownView>
    {
        MarkdownView();
        ~MarkdownView();
        Windows::Foundation::IAsyncOperation<Markdown::DocumentUpdateResult> SetDocumentAsync(
            guid document, uint64_t revision, array_view<uint8_t const> source);
        Windows::Foundation::IAsyncOperation<Markdown::DocumentUpdateResult> SetDocumentWithExtensionsAsync(
            guid document, uint64_t revision, array_view<uint8_t const> source, Markdown::MarkdownExtensions extensions);
        Windows::Foundation::IAsyncOperation<Markdown::DocumentUpdateResult> ApplyEditsAsync(
            guid document, uint64_t baseRevision, uint64_t revision, array_view<Markdown::MarkdownSourceEdit const> edits,
            array_view<uint8_t const> replacements);
        Markdown::MarkdownSourceState GetSourceState() const;
        Markdown::MarkdownMeasurementStatistics GetMeasurementStatistics() const;
        Markdown::MarkdownOutline GetOutline() const;
        void NavigateToHeading(guid document, uint64_t revision, uint64_t textGeneration, uint32_t headingId);
        Markdown::MarkdownFragmentResult NavigateToFragment(guid document, uint64_t revision, uint64_t textGeneration,
                                                             hstring const &destination);
        Markdown::MarkdownFragmentResult NavigateToPreviewPosition(guid document, uint64_t revision, uint64_t textGeneration,
                                                                    uint64_t position);
        Markdown::MarkdownFragmentResult NavigateToHeadingNode(guid document, uint64_t revision, uint64_t textGeneration,
                                                                uint32_t node);
        Markdown::MarkdownFragmentResult NavigateToHeadingTitle(guid document, uint64_t revision, uint64_t textGeneration,
                                                                 hstring const &title);
        Windows::Foundation::Collections::IVectorView<Markdown::MarkdownNoteEmbedSlot> GetVisibleNoteEmbeds();
        bool SetNoteEmbedChild(guid document, uint64_t revision, uint64_t textGeneration, uint32_t objectId,
                               Markdown::MarkdownView const &child);
        void RestrictNoteEmbedToHeading(hstring const &title);
        bool TaskInteractionEnabled() const;
        void TaskInteractionEnabled(bool enabled);
        event_token TaskToggleRequested(
            Windows::Foundation::TypedEventHandler<Markdown::MarkdownView, Markdown::MarkdownTaskToggleRequestedEventArgs> const
                &handler);
        void TaskToggleRequested(event_token const &token) noexcept;
        static fire_and_forget final_release(std::unique_ptr<MarkdownView> self) noexcept;
        Markdown::MarkdownSelection GetSelection() const;
        void SelectRange(guid document, uint64_t revision, uint64_t textGeneration, uint64_t start, uint64_t length);
        bool CopySelection();
        Windows::Foundation::IAsyncOperation<Markdown::MarkdownMappedPosition> MapSourcePositionAsync(
            guid document, uint64_t revision, uint64_t textGeneration, uint64_t offset,
            Markdown::MarkdownPositionAffinity affinity);
        Windows::Foundation::IAsyncOperation<Markdown::MarkdownMappedPosition> MapPreviewPositionAsync(
            guid document, uint64_t revision, uint64_t textGeneration, uint64_t offset,
            Markdown::MarkdownPositionAffinity affinity);
        void RevealRange(guid document, uint64_t revision, uint64_t textGeneration, uint64_t start, uint64_t length,
                         bool alignTop, bool expandDisclosures);
        Windows::Foundation::IAsyncOperation<Markdown::MarkdownMappedSpan> MapSourceSpanAsync(
            guid document, uint64_t revision, uint64_t textGeneration, uint64_t start, uint64_t end,
            Markdown::MarkdownPositionAffinity affinity);
        Windows::Foundation::IAsyncOperation<Markdown::MarkdownMappedSpan> MapPreviewSpanAsync(
            guid document, uint64_t revision, uint64_t textGeneration, uint64_t start, uint64_t end,
            Markdown::MarkdownPositionAffinity affinity);
        bool SetViewportBetweenPositions(guid document, uint64_t revision, uint64_t textGeneration,
                                         uint64_t layoutGeneration, uint64_t start, uint64_t end, double progress);
        bool SetFollowViewportBetweenPositions(guid document, uint64_t revision, uint64_t textGeneration,
                                                uint64_t layoutGeneration, uint64_t start, uint64_t end,
                                                double progress, double topBlend, double bottomBlend);
        bool TryRestoreFollowViewport(guid document, uint64_t revision, uint64_t textGeneration,
                                      uint64_t layoutGeneration, uint64_t start, uint64_t end,
                                      double progress, double topBlend, double bottomBlend);
        bool IsSmoothScrollingEnabled() const;
        void IsSmoothScrollingEnabled(bool enabled);
        void SetScrollSettingsProvider(Markdown::IMarkdownScrollSettingsProvider const &provider);
        void CancelScrollAnimation();
        Markdown::MarkdownViewportAnchor GetViewportAnchor() const;
        event_token ViewportChanged(
            Windows::Foundation::TypedEventHandler<Markdown::MarkdownView, Markdown::MarkdownViewportAnchor> const &handler);
        void ViewportChanged(event_token const &token) noexcept;
        double VerticalScrollProgress() const;
        bool TrySetVerticalScrollProgress(double progress);
        event_token VerticalScrollStarting(
            Windows::Foundation::TypedEventHandler<Markdown::MarkdownView, Windows::Foundation::IInspectable> const &handler);
        void VerticalScrollStarting(event_token const &token) noexcept;
        event_token VerticalScrollChanged(
            Windows::Foundation::TypedEventHandler<Markdown::MarkdownView, Windows::Foundation::IInspectable> const &handler);
        void VerticalScrollChanged(event_token const &token) noexcept;
        void SetImageProvider(Markdown::IMarkdownImageProvider const &provider, uint64_t generation);
        void SetTheme(Markdown::MarkdownTheme const &theme);
        event_token SelectionChanged(
            Windows::Foundation::TypedEventHandler<Markdown::MarkdownView, Markdown::MarkdownSelection> const &handler);
        void SelectionChanged(event_token const &token) noexcept;
        event_token CodeCopied(
            Windows::Foundation::TypedEventHandler<Markdown::MarkdownView, Windows::Foundation::IInspectable> const &handler);
        void CodeCopied(event_token const &token) noexcept;
        event_token LinkRequested(
            Windows::Foundation::TypedEventHandler<Markdown::MarkdownView, Markdown::MarkdownLinkRequestedEventArgs> const
                &handler);
        void LinkRequested(event_token const &token) noexcept;
        bool ShowInactiveCaretMarker() const { CheckUiThread(); return _showInactiveCaretMarker; }
        void ShowInactiveCaretMarker(bool value);
        bool CacheDocumentOnUnload() const { CheckUiThread(); return _cacheDocumentOnUnload; }
        void CacheDocumentOnUnload(bool enabled) { CheckUiThread(); _cacheDocumentOnUnload = enabled; }
        bool IsResourceRenderingDeferred() const { CheckUiThread(); return _resourceRenderingDeferred; }
        void IsResourceRenderingDeferred(bool value);
        bool HasCachedDocument() const { CheckUiThread(); return static_cast<bool>(_cachedPreviewModel); }
        bool IsCachedPresentationSuspended() const { CheckUiThread(); return _cachedPresentationSuspended; }
        void SuspendCachedDocument();
        void DiscardCachedDocument();
        uint64_t GetRetainedByteEstimate();
        Windows::Foundation::IAsyncOperation<Markdown::DocumentUpdateResult> ResumeCachedDocumentAsync(guid document,
                                                                                                        uint64_t revision);
        void Cancel();
        void Refresh();
        void ApplyTheme(Windows::UI::Color const &foreground, Windows::UI::Color const &background,
                        hstring const &fontFamily, double fontSize, double lineHeight, double padding,
                        uint64_t generation);
        uint32_t AbiVersion() const noexcept
        {
            return notepads_markdown_core_abi_version();
        }
        uint64_t AcceptedRevision() const
        {
            CheckUiThread();
            return _acceptedRevision;
        }
        guid DisplayedDocumentId() const
        {
            CheckUiThread();
            return _displayedDocument;
        }
        uint64_t DisplayedRevision() const
        {
            CheckUiThread();
            return _displayedRevision;
        }
        uint64_t DisplayedGeneration() const
        {
            CheckUiThread();
            return _displayedGeneration;
        }
        hstring DisplayedText() const;
        uint64_t DisplayedTextLength() const;
        Windows::Foundation::IAsyncOperation<Markdown::MarkdownFindResult> FindTextAsync(guid document, uint64_t revision,
                                                                                    uint64_t textGeneration,
                                                                                    hstring const &query,
                                                                                    uint64_t start, uint64_t length,
                                                                                    bool backward, bool ignoreCase);
        uint64_t ThemeGeneration() const
        {
            CheckUiThread();
            return _themeGeneration;
        }
        Markdown::RenderingStatus LastStatus() const
        {
            CheckUiThread();
            return _lastStatus;
        }
        bool IsSurfaceReady() const
        {
            CheckUiThread();
            return _surfaceReady;
        }
        event_token InitialViewportChanged(
            Windows::Foundation::TypedEventHandler<Markdown::MarkdownView, Windows::Foundation::IInspectable> const &handler)
        {
            return _initialViewportChanged.add(handler);
        }
        void InitialViewportChanged(event_token const &token) noexcept { _initialViewportChanged.remove(token); }
        double MeasuredWidth() const
        {
            CheckUiThread();
            return _measuredWidth;
        }
        double MeasuredHeight() const
        {
            CheckUiThread();
            return _measuredHeight;
        }
        hstring RenderingError() const
        {
            CheckUiThread();
            return _renderingError;
        }

        uint32_t ReadingImageState(uint32_t object) const noexcept;
        hstring ReadingImageDescription(uint32_t object) const;
        hstring ReadingMathDescription(uint32_t object, uint32_t syntax) const;
        hstring ReadingDiagramDescription(uint32_t object) const;
        bool ReadingDiagramEnabled(TextSnapshot const &snapshot, size_t index) const;
        bool ReadingDiagramFocused(TextSnapshot const &snapshot, size_t index) const;
        void FocusReadingDiagram(TextSnapshot const &snapshot, size_t index);
        void InvokeReadingDiagram(TextSnapshot const &snapshot, size_t index);
        struct ReadingDiagramInspection
        {
            ::Notepads::Markdown::Native::DiagramInspectionTransform transform;
            ::Notepads::Markdown::Native::DiagramInspectionPlacement placement;
        };
        std::optional<ReadingDiagramInspection> DiagramInspectionInfo(TextSnapshot const &snapshot, size_t index) const;
        void SetReadingDiagramInspection(TextSnapshot const &snapshot, size_t index,
            ::Notepads::Markdown::Native::DiagramInspectionTransform const &transform);
        void OnApplyTemplate();
        Windows::UI::Xaml::Automation::Peers::AutomationPeer OnCreateAutomationPeer();
        std::shared_ptr<TextSnapshot const> AutomationSnapshot() const;
        void CheckLiveRange(TextSnapshot const &snapshot) const;
        bool ReadingAvailable() const noexcept;
        std::pair<uint32_t, uint32_t> SelectionBounds() const noexcept;
        ::Notepads::Markdown::Native::ReadingNavigationIndex const &ReadingNavigation(TextSnapshot const &snapshot) const;
        void SelectReadingRange(TextSnapshot const &snapshot, uint32_t start, uint32_t end, bool notify = true);
        void RemoveReadingSelection(TextSnapshot const &snapshot, uint32_t start, uint32_t end);
        ::Notepads::Markdown::Native::ReadingVector<::Notepads::Markdown::Native::ReadingRect> ReadingRectangles(
            TextSnapshot const &snapshot, uint32_t start, uint32_t end) const;
        ::Notepads::Markdown::Native::ReadingVector<::Notepads::Markdown::Native::ReadingSpan> VisibleReadingRanges(
            TextSnapshot const &snapshot) const;
        uint32_t ReadingPositionFromScreen(TextSnapshot const &snapshot, Windows::Foundation::Point point) const;
        Windows::Foundation::Rect ReadingScreenRectangle(::Notepads::Markdown::Native::ReadingRect const &rectangle) const;
        void RevealReadingRange(TextSnapshot const &snapshot, uint32_t start, uint32_t end, bool alignTop,
                                bool externalScroll = false, bool expandDisclosures = false);
        void InvokeReadingLink(TextSnapshot const &snapshot, size_t index);
        void FocusReadingLink(TextSnapshot const &snapshot, size_t index);
        bool ReadingTaskEnabled(TextSnapshot const &snapshot, size_t index) const;
        void InvokeReadingTask(TextSnapshot const &snapshot, size_t index);
        void FocusReadingTask(TextSnapshot const &snapshot, size_t index);
        bool ReadingTaskFocused(TextSnapshot const &snapshot, size_t index) const;
        bool ReadingLinkFocused(TextSnapshot const &snapshot, size_t index) const;
        bool ReadingDisclosureFocused(TextSnapshot const &snapshot, size_t index) const;
        bool ReadingDisclosureExpanded(TextSnapshot const &snapshot, size_t index) const;
        void SetReadingDisclosure(TextSnapshot const &snapshot, size_t index, std::optional<bool> expanded = {});
        void FocusReadingDisclosure(TextSnapshot const &snapshot, size_t index);
        bool ReadingChildVisible(TextSnapshot const &snapshot, ::Notepads::Markdown::Native::ReadingSpan span) const;
        std::optional<bool> ReadingRangeHidden(TextSnapshot const &snapshot, ::Notepads::Markdown::Native::ReadingSpan span) const;
        Windows::UI::Xaml::Controls::ScrollViewer ReadingScrollViewer() const;
        bool BeginReadingScroll(Windows::UI::Xaml::Controls::ScrollViewer const &scroll, bool movesVertically);
        Windows::UI::Xaml::Automation::Provider::IRawElementProviderSimple ReadingObjectProvider(
            std::shared_ptr<TextSnapshot const> const &snapshot, ::Notepads::Markdown::Native::ReadingChildKind kind, size_t index);
        std::optional<::Notepads::Markdown::Native::ReadingSpan> ReadingNoteChildSpan(
            TextSnapshot const &snapshot, Markdown::MarkdownView const &child) const;
        Windows::Foundation::Collections::IVector<Windows::UI::Xaml::Automation::Peers::AutomationPeer>
        ReadingChildren();
        Windows::Foundation::Collections::IVector<Windows::UI::Xaml::Automation::Peers::AutomationPeer>
        ReadingTableChildren(std::shared_ptr<TextSnapshot const> const &snapshot, size_t index, bool captionOnly);
        Windows::UI::Xaml::Automation::Peers::AutomationPeer ReadingCaptionPeer(
            std::shared_ptr<TextSnapshot const> const &snapshot, size_t index);
        Windows::Foundation::Rect ReadingTableRectangle(TextSnapshot const &snapshot, uint32_t node) const;
        // Diagnostic bridge only; these are not MarkdownView WinRT members.
        uint32_t SearchContextCount() const;
        uint32_t SearchOperationCount() const;
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        com_array<uint64_t> ResourceWorkSnapshotForDiagnostics() const;
        com_array<double> StartupHeadingGeometryForDiagnostics(hstring const &title) const;
        bool StartupHighlightsPendingForDiagnostics() const;
        com_array<uint64_t> ViewportHighlightDiagnostics() const;
        void ArmPreviewDeviceLossForDiagnostics(bool initialViewport);
        void CopyCodeBlockForDiagnostics(bool clipboardSucceeds);
        bool HasNoteEmbedParentPinForDiagnostics() const
        {
            if (!_owningDispatcher || !_owningDispatcher.HasThreadAccess())
                throw hresult_wrong_thread();
            return static_cast<bool>(_embedBudgetParent);
        }
#endif

      private:
        // Outlive this view's private graphics references.
        std::shared_ptr<ID3D11Device> _d3dLease;
        ::Notepads::Markdown::Native::ReadingSearchGate _readingSearch;
        std::array<std::weak_ptr<void>, ::Notepads::Markdown::Native::ReadingSearchMaximumInFlight> _readingSearchContexts;
        Markdown::MarkdownExtensions _acceptedExtensions{Markdown::MarkdownExtensions::None};
        ::Notepads::Markdown::Native::MathCoordinatorHandle _mathCoordinator;
        ::Notepads::Markdown::Native::MathPresentations _mathPresentations;
        uint64_t _mathAttempt{1};
        uint64_t _mathGeneration{1};
        uint64_t _mathFactoryEpoch{1};
        uint64_t _publishedMathGeneration{};
        uint64_t _publishedMathLayoutGeneration{};
        bool _mathBound{};
        bool _derivedLayoutDeviceRecovery{};
        Windows::UI::Xaml::DispatcherTimer _derivedLayoutRefreshTimer{nullptr};
        Windows::UI::Xaml::DispatcherTimer::Tick_revoker _derivedLayoutRefreshTickRevoker;
        std::optional<uint64_t> _offscreenDerivedPendingAt;
        void EnsureGraphicsFactory();
        void BindMath() noexcept;
        uint32_t DerivedResidentAllowance(::Notepads::Markdown::Native::ReadingObjectKind family) const noexcept;
        void RetireMath() noexcept;
        void QueueDerivedLayoutRefresh();
        void RefreshDerivedLayout();
        ::Notepads::Markdown::Native::NativeMathKey CaptureMathKey() const;
        ::Notepads::Markdown::Native::PreviewMathSnapshot CaptureMath() const;
        bool CurrentMathFrame(::Notepads::Markdown::Native::PreviewFrame const &frame) const noexcept;
        static void AcceptMathDelivery(Windows::Foundation::IInspectable const &target,
                                       ::Notepads::Markdown::Native::MathPresentationHandle const &payload) noexcept;
        void EnsureDerivedControl();
        ::Notepads::Markdown::Native::DiagramCoordinatorHandle _diagramCoordinator;
        ::Notepads::Markdown::Native::DiagramPresentations _diagramPresentations;
        struct DerivedEditReuse
        {
            std::shared_ptr<::Notepads::Markdown::Native::SemanticDescriptors const> descriptors;
            decltype(_mathPresentations) math;
            decltype(_diagramPresentations) diagrams;
        };
        std::optional<DerivedEditReuse> _derivedEditReuse;
        std::optional<DerivedEditReuse> TakeDerivedEditReuse() noexcept;
        bool RestoreDerivedForEdit(::Notepads::Markdown::Native::PreviewFrame const &frame) noexcept;
        ::Notepads::Markdown::Native::NativeDiagramKey _diagramBoundKey{};
        struct DiagramInspection
        {
            ::Notepads::Markdown::Native::NativeDiagramKey key;
            ::Notepads::Markdown::Native::DiagramInspectionTransform transform;
            bool operator==(DiagramInspection const &) const noexcept = default;
        };
        struct PresentedDiagramInspection
        {
            DiagramInspection state;
            ::Notepads::Markdown::Native::DiagramInspectionPlacement placement;
        };
        std::optional<DiagramInspection> _diagramInspection;
        std::optional<::Notepads::Markdown::Native::NativeDiagramKey> _pendingDiagramInspection;
        static ::Notepads::Markdown::Native::PreviewDiagramVisual const *FindDiagramVisual(
            ::Notepads::Markdown::Native::PreviewFrame const &frame,
            ::Notepads::Markdown::Native::NativeDiagramKey const &key, bool requireVisible = false) noexcept;
        std::optional<::Notepads::Markdown::Native::ReadingRect> DiagramInspectionReadingRect(TextSnapshot const &snapshot) const;
        bool ApplyDiagramInspectionKey(Windows::System::VirtualKey key);
        static bool DrawDiagramInspection(DiagramInspection const &inspection,
            ::Notepads::Markdown::Native::PreviewFrame const &frame, ID2D1DeviceContext *context,
            ::Notepads::Markdown::Native::ViewportGeometry const &geometry,
            ::Notepads::Markdown::Native::DiagramInspectionPlacement &placement) noexcept;
        void NotifyDiagramInspectionPublished(std::optional<PresentedDiagramInspection> const &previous) noexcept;
        uint64_t _diagramAttempt{1};
        uint64_t _diagramGeneration{1};
        uint64_t _diagramFontEpoch{1};
        uint64_t _publishedDiagramGeneration{};
        bool _diagramBound{};
        com_ptr<IDWriteFontCollection> _diagramFontCollection;
        com_ptr<IDWriteFontFallback> _diagramFontFallback;
        std::array<uint16_t, 85> _diagramLocale{};
        void EnsureDiagramFonts();
        void BindDiagram() noexcept;
        void RetireDiagram() noexcept;
        void InvalidateDiagramConfiguration();
        notepads_markdown_diagram_style CaptureDiagramStyle() const;
        ::Notepads::Markdown::Native::NativeDiagramKey CaptureDiagramKey() const;
        ::Notepads::Markdown::Native::PreviewDiagramSnapshot CaptureDiagram() const;
        bool CurrentDiagramFrame(::Notepads::Markdown::Native::PreviewFrame const &frame) const noexcept;
        static void AcceptDiagramDelivery(Windows::Foundation::IInspectable const &target,
                                          ::Notepads::Markdown::Native::DiagramPresentationHandle const &payload) noexcept;
        struct ImageAnchor
        {
            uint64_t offset;
            double lineOffset;
            uint64_t textGeneration;
            uint64_t scrollGeneration;
            uint64_t sourceGeneration;
            guid document;
            uint64_t revision;
            std::shared_ptr<::Notepads::Markdown::Native::MainTextSnapshot const> main;
            double padding;
            bool atomic;
            bool atTop;
            bool atBottom;
        };
        Markdown::IMarkdownImageProvider _imageProvider{nullptr};
        uint64_t _imageProviderGeneration{};
        uint64_t _imageTargetGeneration{};
        uint64_t _imageFontGeneration{1};
        uint64_t _imageFontLoadedGeneration{};
        uint64_t _imageFontWatchVersion{};
        uint64_t _imageFontLayoutBarrier{};
        ImageFontWatch *_imageFontWatch{};
        std::shared_ptr<::Notepads::Markdown::Native::ImageFontControlTestHooks> _imageFontControlTests;
        bool _imageFontRefreshPending{};
        bool _imageFontWatchStartFailed{};
        bool _imageFontReconcilePending{};
        uint64_t _imageScrollGeneration{};
        bool _imageAnchorUpdatingExtent{};
        bool _imageAnchorExtentPending{};
        std::optional<double> _imageUserScrollY;
        double _lastPreviewScrollOffset{};
        uint64_t _lastPreviewScrollAt{};
        std::optional<double> _imageRestoringY;
        std::optional<ImageAnchor> _imageAnchor;
        std::shared_ptr<::Notepads::Markdown::Native::ReadingNavigationIndex const> _imageAnchorNavigation;
        std::shared_ptr<::Notepads::Markdown::Native::ImageCoordinator> _images;
        std::shared_ptr<::Notepads::Markdown::Native::ImagePresentationSnapshot const> _imagePresentation;
        uint64_t _imageGeometryGeneration{};
        std::shared_ptr<::Notepads::Markdown::Native::PreviewFrame> ProjectImageFrame(
            std::shared_ptr<::Notepads::Markdown::Native::PreviewFrame> const &frame) const;
        bool RefreshImageFrame();
        std::shared_ptr<::Notepads::Markdown::Native::ImagePresentationSnapshot const> _publishedImagePresentation;
        ::Notepads::Markdown::Native::ImageDrawCache _imageDrawCache;
        ::Notepads::Markdown::Native::NativeTextBitmapCache _textBitmapCache;
        Windows::UI::Xaml::DispatcherTimer _imageRefreshTimer{nullptr};
        Windows::UI::Xaml::DispatcherTimer::Tick_revoker _imageRefreshTickRevoker;
        Windows::UI::Xaml::DispatcherTimer _imageAnimationTimer{nullptr};
        Windows::UI::Xaml::DispatcherTimer::Tick_revoker _imageAnimationTickRevoker;
        void ResetImageAnimation() noexcept;
        bool ImageAnimationVisible() const;
        void ImageAnimationVisibilityChanged() noexcept;
        void QueueImageAnimation() noexcept;
        std::optional<int64_t> _imageVisibilityToken;
        std::optional<std::chrono::steady_clock::time_point> _imageAnimationHiddenAt;
        Windows::UI::Core::CoreWindow::VisibilityChanged_revoker _imageWindowVisibilityRevoker;
        void AdvanceImageAnimation() noexcept;
        void BindImages();
        bool RestoreCompletedImages(::Notepads::Markdown::Native::PreviewFrame const &frame,
            std::span<::Notepads::Markdown::Native::PreviewImageWidth const> provenWidths = {});
        friend struct ImageFontWatch;
        friend struct ImageFontControlTestAccess;
        friend struct ImageReuseControlTestAccess;
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        friend struct ReadingOutlineControlTestAccess;
        friend struct PreviewPublicationTestAccess;
#endif
        ::Notepads::Markdown::Native::ReadingAllocationFactory ImagePresentationAllocation() noexcept;
        void FreezeImagePresentation(std::shared_ptr<::Notepads::Markdown::Native::ImagePresentationSnapshot> next = {});
        void LoadImageFonts() noexcept;
        void StopImageFonts(bool destroy = false) noexcept;
        void RequestImageFontRefresh() noexcept;
        void EnsureImageFontWatch() noexcept;
        void ReconcileImageFonts();
        void ReconcilePictureSources();
        bool _pictureDark{};
        bool SvgImagesEnabled() const noexcept;
        bool AcceptImageFontChange(ImageFontWatch *watch, uint64_t loadedGeneration, uint64_t version) noexcept;
        void RetireImages(bool keepReuse = false) noexcept;
        void PumpImages();
        void QueueResourceRendering();
        ::Notepads::Markdown::Native::NativeSvgImageTarget CaptureImageTarget(uint32_t object,
                                                                       notepads_markdown_image_target const &previous = {});
        double ImageAvailableWidth(uint32_t object, double contentWidth, double previousWidth) const;
        void QueueImageRefresh();
        void CaptureImageAnchorBeforeChange();
        bool CurrentImageAnchor(ImageAnchor const &anchor) const noexcept;
        bool ImageAnchorPending(::Notepads::Markdown::Native::PreviewFrame const &frame) const noexcept;
        double ImageAnchorTarget(::Notepads::Markdown::Native::PreviewFrame const &frame,
                                 ImageAnchor const &anchor, double maximum) const;
        void CompleteImageAnchor(::Notepads::Markdown::Native::PreviewFrame const &frame);
        void RefreshImageLayout();
        void UpdateImageAnchorForScroll(double y);
        bool CurrentPreviewGeometry(::Notepads::Markdown::Native::PreviewFrame const &frame) const;
        bool ApplyFollowViewport(guid document, uint64_t revision, uint64_t textGeneration,
                                 uint64_t layoutGeneration, uint64_t start, uint64_t end,
                                 double progress, double topBlend, double bottomBlend, bool requirePresented);
        bool CurrentImageFrame(::Notepads::Markdown::Native::PreviewFrame const &frame) const noexcept;
        bool RestoreImageAnchor(std::shared_ptr<::Notepads::Markdown::Native::PreviewFrame> const &frame);
        void NotifyImageFramePublished();
        void CompleteImage(std::shared_ptr<::Notepads::Markdown::Native::ImageLoadTask> const &task,
                           std::shared_ptr<::Notepads::Markdown::Native::NativeImageDecodeResult> const &result,
                           uint64_t textGeneration);
        static fire_and_forget ObserveImageCancellation(Windows::UI::Core::CoreDispatcher dispatcher,
                                                        std::shared_ptr<::Notepads::Markdown::Native::ImageLoadTask> task);
        static fire_and_forget ObserveImage(weak_ref<MarkdownView> owner, Windows::UI::Core::CoreDispatcher dispatcher,
                                            std::shared_ptr<::Notepads::Markdown::Native::ImageLoadTask> task,
                                            std::shared_ptr<::Notepads::Markdown::Native::ReadingMemory> memory,
                                            uint64_t textGeneration, ::Notepads::Markdown::Native::NativeSvgImageTarget target,
                                            bool reuseSvgEncoded = false,
                                            std::shared_ptr<::Notepads::Markdown::Native::NativeImageReuseCache> rasterReuse = {},
                                            uint64_t reuseScope = 0);
        Windows::Foundation::IAsyncOperation<Markdown::MarkdownMappedPosition> MapPositionAsync(
            guid document, uint64_t revision, uint64_t textGeneration, uint64_t offset,
            Markdown::MarkdownPositionAffinity affinity, bool fromSource);
        Windows::Foundation::IAsyncOperation<Markdown::MarkdownMappedSpan> MapSpanAsync(
            guid document, uint64_t revision, uint64_t textGeneration, uint64_t start, uint64_t end,
            Markdown::MarkdownPositionAffinity affinity, bool fromSource);
        event<Windows::Foundation::TypedEventHandler<Markdown::MarkdownView, Markdown::MarkdownViewportAnchor>> _viewportChanged;
        void RaiseViewportChanged();
        event<Windows::Foundation::TypedEventHandler<Markdown::MarkdownView, Windows::Foundation::IInspectable>>
            _verticalScrollStarting, _verticalScrollChanged;
        ::Notepads::Markdown::Native::ReadingRect ClipReadingRectangle(
            ::Notepads::Markdown::Native::ReadingRect rect) const noexcept;
        com_ptr<implementation::ReadingObjectProvider> ReadingPeer(std::shared_ptr<TextSnapshot const> const &snapshot,
                                                                   ::Notepads::Markdown::Native::ReadingChildKind kind, size_t index);
        std::shared_ptr<ReadingPeerCache> _readingPeers;
        using DisclosureOverrides = ::Notepads::Markdown::Native::ReadingVector<notepads_markdown_preview_disclosure_override>;
        std::shared_ptr<DisclosureOverrides const> _disclosureOverrides;
        void ApplyDisclosureOverride(DisclosureOverrides &overrides, uint32_t node, bool expanded);
        bool ExpandReadingDisclosures(::Notepads::Markdown::Native::ReadingSpan span, uint32_t node = 0);
        void InstallDisclosureOverrides(std::shared_ptr<DisclosureOverrides const> overrides);
        struct PendingReadingReveal
        {
            uint64_t serial;
            uint64_t textGeneration;
            ::Notepads::Markdown::Native::ReadingSpan span;
            uint32_t node;
            bool alignTop;
            bool externalScroll;
            bool scrollStartAnnounced{};
        };
        uint64_t _readingRevealSerial{};
        std::optional<PendingReadingReveal> _pendingReadingReveal;
        bool _applyingReadingReveal{};
        void RequestReadingReveal(uint64_t textGeneration, ::Notepads::Markdown::Native::ReadingSpan span,
                                  uint32_t node, bool alignTop, bool externalScroll, bool expandDisclosures);
        void TryApplyPendingReadingReveal();
        struct ReadingAction
        {
            std::shared_ptr<::Notepads::Markdown::Native::PreviewFrame> frame;
            std::shared_ptr<TextSnapshot const> snapshot;
            uint64_t presentation;
            ReadingAction(std::shared_ptr<::Notepads::Markdown::Native::PreviewFrame> value,
                          std::shared_ptr<TextSnapshot const> text, uint64_t serial)
                : frame(std::move(value)), snapshot(std::move(text)), presentation(serial)
            {
            }
        };
        void NotifyDisclosureChanges(ReadingAction const &action, ::Notepads::Markdown::Native::ReadingVisibility const &previous);
        ReadingAction CaptureReadingAction() const;
        Markdown::MarkdownFragmentResult TryScrollToReadingTarget(ReadingAction const &action, double target, bool externalScroll = false);
        Markdown::MarkdownFragmentResult NavigateToHeadingTarget(guid document, uint64_t revision, uint64_t textGeneration,
                                                                  uint32_t node, std::wstring_view title);
        std::optional<Windows::Foundation::Rect> NoteEmbedBounds(::Notepads::Markdown::Native::ReadingSpan span) const;
        void PositionNoteEmbed(Markdown::MarkdownView const &child, Windows::Foundation::Rect const &bounds);
        void UpdateNoteEmbedExtent(Markdown::MarkdownView const &child) noexcept;
        void UpdateNoteEmbeds();
        void ClearNoteEmbeds() noexcept;
        bool PrepareNoteEmbedsForEdit(std::span<notepads_markdown_core_edit_row const> edits) noexcept;
        bool RestoreNoteEmbeds(::Notepads::Markdown::Native::PreviewFrame const &frame);
        Windows::UI::Xaml::Automation::Peers::AutomationPeer ReadingNotePeer(
            TextSnapshot const &snapshot, ::Notepads::Markdown::Native::ReadingChildKind kind, size_t index) const;
        void RaiseNoteEmbedStructureChanged() noexcept;
        bool CurrentReadingAction(ReadingAction const &action) const;
        void InvalidateVerticalNavigation() noexcept;
        void OnTaskInteractionChanged();
        void FocusReadingAction(TextSnapshot const &snapshot, ::Notepads::Markdown::Native::ReadingActionTarget target);
        std::optional<::Notepads::Markdown::Native::ReadingActionTarget> HitReadingAction(
            ::Notepads::Markdown::Native::ReadingHit const &hit, Windows::Foundation::Point point) const;
        enum class SelectionMotion
        {
            Other,
            Vertical
        };
        bool UpdateSelection(uint32_t anchor, uint32_t active, bool notify = true,
                             SelectionMotion motion = SelectionMotion::Other, bool trailingCaret = false);
        void RaiseReadingTextChanged();
        void UpdateEffectiveTheme();
        void ApplyEffectiveColors();
        void OnReadingPointerPressed(Windows::Foundation::IInspectable const &,
                                     Windows::UI::Xaml::Input::PointerRoutedEventArgs const &);
        void OnReadingPointerMoved(Windows::Foundation::IInspectable const &,
                                   Windows::UI::Xaml::Input::PointerRoutedEventArgs const &);
        bool IsReadingPointerOnChrome(Windows::Foundation::IInspectable const &source) const;
        std::optional<size_t> UpdateReadingLinkCursor() noexcept;
        void SetReadingLinkCursor(bool overLink) noexcept;
        void TrackReadingHover(Windows::Foundation::Point point, std::optional<size_t> link);
        void ShowReadingHover();
        void RefreshReadingHover();
        void ClearReadingHover() noexcept;
        bool CodeCopyViewportCurrent() const;
        void UpdateCodeCopyButton(bool preferCaret = false) noexcept;
        void HideCodeCopyButton() noexcept;
        void RetireCodeCopyButton() noexcept;
        void OnCodeCopyClick(Windows::Foundation::IInspectable const &, Windows::UI::Xaml::RoutedEventArgs const &);
        void CopyCodeBlock(Windows::Foundation::IInspectable const &sender, void (*writeClipboard)(std::wstring_view));
        struct ReadingHoverTarget
        {
            enum class Kind { Fallback, LinkTitle, ImageTitle };
            Kind kind;
            size_t index;
            bool operator==(ReadingHoverTarget const &) const = default;
        };
        std::optional<ReadingHoverTarget> ReadingHoverAt(Windows::Foundation::Point point, std::optional<size_t> link) const;
        void OnReadingPointerReleased(Windows::Foundation::IInspectable const &,
                                      Windows::UI::Xaml::Input::PointerRoutedEventArgs const &);
        void OnReadingPointerCaptureLost(Windows::Foundation::IInspectable const &,
                                         Windows::UI::Xaml::Input::PointerRoutedEventArgs const &);
        void OnReadingKeyDown(Windows::Foundation::IInspectable const &,
                              Windows::UI::Xaml::Input::KeyRoutedEventArgs const &);
        void OnReadingFocusChanged(Windows::Foundation::IInspectable const &,
                                   Windows::UI::Xaml::RoutedEventArgs const &);
        bool ContinueReadingDrag();
        void StopReadingDrag();
        static std::optional<size_t> HitReadingLink(::Notepads::Markdown::Native::PreviewFrame const &frame,
                                                    ::Notepads::Markdown::Native::ReadingHit const &hit);
        void DetachReadingScrollEvents() noexcept;
        void MoveReadingCaret(uint32_t position, bool extend, bool trailingCaret = false);
        bool MoveReadingCaretVertically(ReadingAction const &action, size_t next, bool extend);
        void FinishPendingCaret();
        using PreviewRequest = ::Notepads::Markdown::Native::PreviewRenderRequest;
        void BeginPreviewSource(std::shared_ptr<PreviewUpdate> const &update);
        void PreviewParsed(::Notepads::Markdown::Native::CoreCompletionTicket const &ticket,
                           std::shared_ptr<PreviewUpdate> const &update);
        void StartPreviewLayout();
        bool HasActivePreviewLayout() const;
        void StartPreviewFrame();
        void InstallPreviewFrame(std::shared_ptr<PreviewRequest> const &request);
        void InstallInitialViewport(std::shared_ptr<PreviewRequest> const &request,
            std::shared_ptr<::Notepads::Markdown::Native::PreviewFrame> const &frame) noexcept;
        void ClearInitialViewport() noexcept;
        void StopPreview(Markdown::DocumentUpdateStatus status, bool stopController = true, bool wakeOwner = true,
                         bool keepImageReuse = false, bool keepNoteEmbeds = false, bool keepDerivedReuse = false);
        void OnPreviewLayoutUpdated(Windows::Foundation::IInspectable const &,
                                    Windows::Foundation::IInspectable const &);
        void OnPreviewViewChanged(Windows::Foundation::IInspectable const &,
                                  Windows::UI::Xaml::Controls::ScrollViewerViewChangedEventArgs const &);
        bool DrawPreview(std::shared_ptr<::Notepads::Markdown::Native::PreviewFrame> projected = {});
        struct PresentedViewport
        {
            std::shared_ptr<::Notepads::Markdown::Native::PreviewFrame> frame;
            ::Notepads::Markdown::Native::ViewportGeometry geometry;
            uint64_t serial{};
            std::optional<size_t> hoveredLink;
            std::optional<PresentedDiagramInspection> inspection;
        };
        std::optional<size_t> ReadingLinkAtPointer(PresentedViewport const &viewport) const noexcept;
        bool DrawPreview(PresentedViewport &candidate, bool showLinkHover = true);
        bool CurrentViewportFrame(::Notepads::Markdown::Native::PreviewFrame const &frame,
                                  ::Notepads::Markdown::Native::ViewportGeometry const &geometry) const;
        double EffectiveRasterScale() const;
        bool CaptureViewportGeometry();
        void InvalidateViewportTemplate(bool rediscover = true) noexcept;
        void DetachViewportTemplate() noexcept;
        void AttachViewportTemplate();
        void FailViewportTemplate(wchar_t const *reason);
        bool CanScheduleViewportFrame() noexcept;
        void ScheduleViewportRendering();
        void CancelViewportRendering() noexcept;
        struct ScrollIntent
        {
            Windows::UI::Xaml::Controls::ScrollViewer scroll{nullptr};
            Windows::UI::Xaml::Controls::Canvas extent{nullptr};
            uint64_t source{}, layout{}, geometry{}, token{}, serial{};
        };
        ScrollIntent CaptureScrollIntent() const;
        ScrollIntent BeginImmediateScroll();
        bool BeginVerticalReadingScroll(Windows::UI::Xaml::Controls::ScrollViewer const &scroll,
                                        std::optional<uint64_t> revealSerial = {});
        bool CurrentScrollIntent(ScrollIntent const &intent) const noexcept;
        bool ScrollExtentCurrent(ScrollIntent const &intent, double frameHeight) const;
        uint64_t CancelWheelScroll() noexcept;
        void AttachWheelScrollEvents();
        void DetachWheelScrollEvents() noexcept;
        void ObserveWheelScroll() noexcept;
        void OnPreviewWheel(Windows::Foundation::IInspectable const &,
                            Windows::UI::Xaml::Input::PointerRoutedEventArgs const &);
        void DispatchViewportFrame();
        void FinishViewportRequest(std::shared_ptr<PreviewRequest> const &request) noexcept;
        bool FailedViewportTarget(::Notepads::Markdown::Native::ViewportTarget const &target) const noexcept;
        Windows::Foundation::Point ControlToViewport(Windows::Foundation::Point point) const noexcept;
        Windows::Foundation::IAsyncOperation<Markdown::DocumentUpdateResult> AwaitPreview(
            std::shared_ptr<PreviewUpdate> update);
        static fire_and_forget ObservePreviewParse(weak_ref<MarkdownView> owner,
                                                   Windows::UI::Core::CoreDispatcher dispatcher,
                                                   ::Notepads::Markdown::Native::CoreCompletionTicket ticket,
                                                   std::shared_ptr<PreviewUpdate> update);
        static fire_and_forget ObservePreviewFrame(weak_ref<MarkdownView> owner,
                                                   Windows::UI::Core::CoreDispatcher dispatcher,
                                                   std::shared_ptr<PreviewRequest> request);
        static fire_and_forget DrainPreview(weak_ref<MarkdownView> owner, Windows::UI::Core::CoreDispatcher dispatcher,
                                            std::unique_ptr<::Notepads::Markdown::Native::CoreController> controller,
                                            std::shared_ptr<PreviewDrain> drain, uint64_t serial, bool wakeOwner);

        void ScheduleCurrent();
        void OnLoaded(Windows::Foundation::IInspectable const &, Windows::UI::Xaml::RoutedEventArgs const &);
        void OnUnloaded(Windows::Foundation::IInspectable const &, Windows::UI::Xaml::RoutedEventArgs const &);
        bool ReopenCachedPresentation();
        void OnSizeChanged(Windows::Foundation::IInspectable const &, Windows::UI::Xaml::SizeChangedEventArgs const &);
        void OnDpiChanged(Windows::Graphics::Display::DisplayInformation const &,
                          Windows::Foundation::IInspectable const &);
        bool DrawCurrent();
        void RetryDeferredRender();
        void ReleaseGraphics() noexcept;
        void CheckUiThread() const;
        bool _documentAccepted{};
        std::shared_ptr<::Notepads::Markdown::Native::ReadingSourceState> _readingSource;
        struct HighlightSelection
        {
            std::array<uint32_t, NOTEPADS_MARKDOWN_HIGHLIGHT_MAX_BLOCKS> nodes{};
            uint32_t count{};

            std::span<uint32_t const> Nodes() const noexcept { return {nodes.data(), count}; }
            bool Contains(uint32_t node) const noexcept
            {
                return std::binary_search(nodes.begin(), nodes.begin() + count, node);
            }
            bool operator==(HighlightSelection const &other) const noexcept
            {
                return count == other.count && std::equal(nodes.begin(), nodes.begin() + count, other.nodes.begin());
            }
        };
        HighlightSelection VisibleHighlightBlocks() const noexcept;
        bool PreviewHighlightsCover(HighlightSelection const &selection) const noexcept;
        bool _previewViewportHighlights{true};
        bool _previewLegacyHighlightsAttempted{};
        std::optional<HighlightSelection> _previewHighlightFailed;
        std::weak_ptr<PreviewHighlightWork> _previewHighlightWork;
        std::shared_ptr<::Notepads::Markdown::Native::NativeHighlightCache> _previewHighlightCache;
        friend struct PreviewHighlightWork;
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        struct
        {
            uint64_t requested{}, started{}, completed{}, applied{}, rejected{};
            uint64_t reusedBlocks{}, newlyCompletedBlocks{}, workTicks{};
        } _previewHighlightCounts;
#endif
        std::shared_ptr<::Notepads::Markdown::Native::NativeHighlightSnapshot const> _previewHighlights;
        void QueuePreviewHighlights() noexcept;
        static fire_and_forget ObservePreviewHighlights(weak_ref<MarkdownView> owner,
            Windows::UI::Core::CoreDispatcher dispatcher, std::shared_ptr<PreviewHighlightWork> work);
        ::Notepads::Markdown::Native::PreviewTheme _readingTheme;
        uint64_t _effectiveThemeGeneration{1};
        uint32_t _selectionAnchor{};
        uint32_t _selectionActive{};
        bool _readingCaretTrailing{};
        ::Notepads::Markdown::Native::ReadingActionTarget _focusedAction{};
        bool _taskInteractionEnabled{};
        uint64_t _taskRequestId{};
        uint64_t _taskInteractionGeneration{1};
        bool _readingFocused{};
        bool _showInactiveCaretMarker{};
        bool _dragging{};
        bool _dragExceeded{};
        std::optional<uint32_t> _readingPointerId;
        uint64_t _readingDragGeneration{};
        Windows::Foundation::Point _dragStart{};
        Windows::Foundation::Point _lastDragPoint{};
        ::Notepads::Markdown::Native::ReadingActionTarget _pressedAction{};
        std::weak_ptr<::Notepads::Markdown::Native::PreviewFrame> _readingPressedFrame;
        uint64_t _readingPressedTaskGeneration{};
        uint64_t _readingPressedPresentation{};
        float _preferredCaretX{};
        bool _hasPreferredCaretX{};
        std::optional<size_t> _pendingCaretLine;
        bool _pendingCaretFrameRequested{};
        bool _pendingCaretExtend{};
        uint64_t _pendingCaretText{};
        uint64_t _pendingCaretLayout{};
        Windows::UI::Xaml::DispatcherTimer _readingDragTimer{nullptr};
        Windows::UI::Xaml::DispatcherTimer::Tick_revoker _readingDragTickRevoker;
        Windows::UI::Xaml::DispatcherTimer _readingHoverTimer{nullptr};
        Windows::UI::Xaml::DispatcherTimer::Tick_revoker _readingHoverTickRevoker;
        Windows::UI::Xaml::Controls::ToolTip _readingHoverToolTip{nullptr};
        Windows::Foundation::Point _readingHoverPoint{};
        std::optional<ReadingHoverTarget> _readingHover;
        std::optional<Windows::Foundation::Point> _readingLinkPoint;
        bool _linkHoverDrawQueued{};
        Windows::Foundation::Point _readingLinkViewportOrigin{};
        Windows::UI::Core::CoreCursor _readingLinkCursor{nullptr};
        Windows::UI::Core::CoreCursor _previousReadingCursor{nullptr};
        Windows::UI::ViewManagement::AccessibilitySettings _accessibility{nullptr};
        Windows::UI::ViewManagement::UISettings _systemColors{nullptr};
        Windows::UI::ViewManagement::AccessibilitySettings::HighContrastChanged_revoker _highContrastRevoker;
        Windows::UI::ViewManagement::UISettings::ColorValuesChanged_revoker _systemColorsRevoker;
        Windows::UI::ViewManagement::UISettings::AnimationsEnabledChanged_revoker _imageMotionRevoker;
        Windows::UI::Xaml::UIElement::PointerPressed_revoker _readingPressedRevoker;
        Windows::UI::Xaml::UIElement::PointerMoved_revoker _readingMovedRevoker;
        Windows::UI::Xaml::UIElement::PointerEntered_revoker _readingEnteredRevoker;
        Windows::UI::Xaml::UIElement::PointerExited_revoker _readingExitedRevoker;
        Windows::UI::Xaml::UIElement::PointerReleased_revoker _readingReleasedRevoker;
        Windows::UI::Xaml::UIElement::PointerCaptureLost_revoker _readingCaptureRevoker;
        Windows::UI::Xaml::UIElement::KeyDown_revoker _readingKeyRevoker;
        Windows::UI::Xaml::UIElement::GotFocus_revoker _readingGotFocusRevoker;
        Windows::UI::Xaml::UIElement::LostFocus_revoker _readingLostFocusRevoker;
        event<Windows::Foundation::TypedEventHandler<Markdown::MarkdownView, Markdown::MarkdownSelection>> _selectionChanged;
        event<Windows::Foundation::TypedEventHandler<Markdown::MarkdownView, Windows::Foundation::IInspectable>> _codeCopied;
        event<Windows::Foundation::TypedEventHandler<Markdown::MarkdownView, Markdown::MarkdownLinkRequestedEventArgs>>
            _linkRequested;
        event<Windows::Foundation::TypedEventHandler<Markdown::MarkdownView, Markdown::MarkdownTaskToggleRequestedEventArgs>>
            _taskToggleRequested;
        bool _previewModelReady{};
        bool _cacheDocumentOnUnload{};
        bool _cachedPresentationSuspended{};
        bool _resourceRenderingDeferred{};
        bool _previewDeviceRetry{};
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        std::optional<bool> _failNextPreviewDrawOnInitialViewport;
#endif
        uint64_t _previewSourceGeneration{};
        uint64_t _previewSubmittedSourceGeneration{};
        uint64_t _previewLayoutGeneration{};
        uint64_t _previewEditPreparationSourceGeneration{};
        uint64_t _previewScheduledToken{};
        double _previewRequestedWidth{};
        double _previewRequestedHeight{};
        double _previewRequestedY{};
        std::unique_ptr<::Notepads::Markdown::Native::CoreController> _previewController;
        // Mounting consumes a child even after its parent-model pin retires;
        // later source work must not fall back to a standalone budget.
        bool _isEmbeddedChild{};
        ::Notepads::Markdown::Native::CoreModelPin _embedBudgetParent;
        weak_ref<MarkdownView> _noteParentView;
        hstring _noteSectionTitle;
        struct MountedNoteEmbed
        {
            uint32_t object{};
            uint64_t textGeneration{};
            Markdown::MarkdownView child{nullptr};
            std::optional<std::pair<uint64_t, uint64_t>> sourceSpan;
            float fittedHeight{};
            uint32_t previousObject{};
            uint64_t previousTextGeneration{};
        };
        void ClearNoteEmbed(MountedNoteEmbed &slot) noexcept;
        std::array<MountedNoteEmbed, 8> _noteEmbeds;
        bool _noteEmbedsPending{};
        // Retain fitted extents for this accepted source across offscreen unmounts.
        std::shared_ptr<::Notepads::Markdown::Native::PreviewNoteEmbedExtents const> _noteEmbedExtents;
        ::Notepads::Markdown::Native::CoreModelPin _cachedPreviewModel;
        struct CachedViewport
        {
            uint32_t offset;
            double withinLine;
            double y;
            bool atBottom;
            bool atomic{};
        };
        std::optional<CachedViewport> _lastViewport;
        std::optional<CachedViewport> _viewportToRestore;
        bool RestoreCachedViewport(::Notepads::Markdown::Native::PreviewFrame const &frame);
        void CancelViewportRestore(std::optional<double> target = {}) noexcept;
        std::shared_ptr<PreviewDrain> _previewDrain;
        std::shared_ptr<PreviewDrain> _previewDrainStorage;
        ::Notepads::Markdown::Native::CoreCompletionTicket _previewTicket;
        std::shared_ptr<PreviewUpdate> _previewUpdate;
        std::shared_ptr<::Notepads::Markdown::Native::NativeMeasurementCache> _measurementCache;
        bool _measurementCacheAttempted{};
        std::shared_ptr<::Notepads::Markdown::Native::NativeMeasurementCache> EnsureMeasurementCache() noexcept;
        void ClearMeasurementCache() noexcept;
        bool IsPreviewLoaded() const
        {
            return _loaded && !_cachedPresentationSuspended && IsLoaded();
        }
        std::shared_ptr<::Notepads::Markdown::Native::PreviewCancellation> _previewCancel;
        std::shared_ptr<::Notepads::Markdown::Native::NativeDocumentRenderer> _previewRenderer;
        PresentedViewport _presented;
        ::Notepads::Markdown::Native::ViewportGeometry _viewportGeometry;
        std::optional<::Notepads::Markdown::Native::ViewportTarget> _viewportDesired;
        std::optional<::Notepads::Markdown::Native::ViewportTarget> _viewportFailed;
        uint64_t _viewportFailedToken{};
        std::shared_ptr<PreviewRequest> _viewportActive;
        uint64_t _viewportSerial{};
        uint64_t _viewportGeometryEpoch{};
        enum class ViewportTemplateState
        {
            Waiting,
            Ready,
            Unsupported
        };
        ViewportTemplateState _viewportTemplateState{ViewportTemplateState::Waiting};
        bool _viewportRediscover{true};
        bool _viewportRecovery{};
        Windows::UI::Xaml::Controls::Canvas _viewportOverlay{nullptr};
        Windows::UI::Xaml::Controls::ScrollContentPresenter _viewportPresenter{nullptr};
        Windows::UI::Xaml::DependencyObject _viewportVisualRoot{nullptr};
        Windows::UI::Xaml::DependencyObject _viewportAwaitingVisualRoot{nullptr};
        Windows::UI::Xaml::Controls::ControlTemplate _viewportInnerTemplate{nullptr};
        int64_t _viewportTemplateCallback{}, _viewportContentCallback{};
        bool _viewportCallbacks{};
        Windows::UI::Xaml::FrameworkElement::Loaded_revoker _viewportScrollLoaded;
        Windows::UI::Xaml::FrameworkElement::Unloaded_revoker _viewportScrollUnloaded;
        Windows::UI::Xaml::XamlRoot _viewportRoot{nullptr};
        Windows::UI::Xaml::XamlRoot::Changed_revoker _viewportRootChanged;
        Windows::UI::Xaml::Media::CompositionTarget::Rendering_revoker _viewportRendering;
        std::shared_ptr<::Notepads::Markdown::Native::PreviewRenderQueue> _previewQueue;
        std::shared_ptr<std::atomic<uint64_t>> _previewToken{std::make_shared<std::atomic<uint64_t>>(1)};
        Windows::UI::Xaml::Controls::ScrollViewer _previewScroll{nullptr};
        Windows::UI::Xaml::Controls::Canvas _previewExtent{nullptr};
        bool _smoothScrollingEnabled{true};
        Markdown::IMarkdownScrollSettingsProvider _scrollSettingsProvider{nullptr};
        ::Notepads::Markdown::Native::NativeWheelScroll _wheelScroll;
        double _wheelScrollableHeight{-1};
        Windows::UI::Xaml::UIElement::PointerWheelChanged_revoker _wheelChanged;
        Windows::UI::Xaml::UIElement::PointerWheelChanged_revoker _codeCopyWheel;
        Windows::Foundation::IInspectable _scrollPointerPressed{nullptr};
        Windows::UI::Xaml::UIElement::PreviewKeyDown_revoker _scrollKeyDown;
        Windows::UI::Xaml::Controls::ScrollViewer::ViewChanged_revoker _previewScrollRevoker;
        Windows::UI::Core::CoreDispatcher _owningDispatcher{nullptr};
        std::shared_ptr<std::vector<uint8_t> const> _acceptedSource;
        notepads_markdown_core_submit_result _previewSourceState{};
        bool _editChannelNeedsResync{true};
        void RecordPreviewSourceState(::Notepads::Markdown::Native::CoreSubmitResult const &result) noexcept;
        guid _acceptedDocument{};
        uint64_t _acceptedRevision{};
        uint64_t _generation{};
        uint64_t _themeGeneration{};
        guid _displayedDocument{};
        uint64_t _displayedRevision{};
        uint64_t _displayedGeneration{};
        mutable std::shared_ptr<TextSnapshot const> _automationSnapshot;
        bool _loaded{};
        bool _surfaceReady{};
        uint64_t _initialViewportToken{};
        event<Windows::Foundation::TypedEventHandler<Markdown::MarkdownView, Windows::Foundation::IInspectable>>
            _initialViewportChanged;
        bool _deferredRender{};
        Markdown::RenderingStatus _lastStatus{Markdown::RenderingStatus::NotLoaded};
        hstring _renderingError;
        double _measuredWidth{};
        double _measuredHeight{};
        Windows::UI::Color _foreground{255, 30, 30, 30};
        Windows::UI::Color _background{255, 255, 255, 255};
        hstring _fontFamily{L"Segoe UI"};
        float _fontSize{18.0f};
        float _lineHeight{27.0f};
        float _padding{16.0f};

        struct CodeCopyTarget
        {
            uint64_t textGeneration{};
            uint32_t node{}, start{}, end{};
            bool operator==(CodeCopyTarget const &) const = default;
        };
        std::optional<Windows::Foundation::Point> _codeCopyPointer;
        std::optional<CodeCopyTarget> _codeCopyTarget;
        uint64_t _codeCopyPresentation{};
        Windows::UI::Xaml::Controls::Button _codeCopyButton{nullptr};
        Windows::UI::Xaml::DataTemplate _codeCopyTemplate{nullptr};
        Windows::UI::Xaml::Controls::Primitives::ButtonBase::Click_revoker _codeCopyClick;
        Windows::UI::Xaml::FrameworkElement::Loaded_revoker _codeCopyFocusLoaded;
        Windows::UI::Xaml::Controls::Border _surfaceTarget{nullptr};
        Windows::UI::Xaml::Media::ImageBrush _surfaceBrush{nullptr};
        Windows::UI::Xaml::Media::Imaging::SurfaceImageSource _surface{nullptr};
        com_ptr<ISurfaceImageSourceNativeWithD2D> _surfaceNative;
        com_ptr<ID3D11Device> _d3dDevice;
        com_ptr<ID2D1Factory1> _d2dFactory;
        com_ptr<ID2D1Device> _d2dDevice;
        com_ptr<IDWriteFactory> _writeFactory;
        Windows::Graphics::Display::DisplayInformation _displayInfo{nullptr};
        Windows::Graphics::Display::DisplayInformation::DpiChanged_revoker _dpiRevoker;
        Windows::UI::Xaml::FrameworkElement::Loaded_revoker _loadedRevoker;
        Windows::UI::Xaml::FrameworkElement::Unloaded_revoker _unloadedRevoker;
        Windows::UI::Xaml::FrameworkElement::SizeChanged_revoker _sizeRevoker;
        Windows::UI::Xaml::FrameworkElement::LayoutUpdated_revoker _previewLayoutRevoker;
        Windows::UI::Core::CoreWindow::Closed_revoker _closedRevoker;
        Windows::UI::Xaml::Media::CompositionTarget::SurfaceContentsLost_revoker _surfaceContentsLostRevoker;
        std::shared_ptr<PreviewRasterBandState> _previewRasterBand;
        uint64_t _previewSurfaceSerial{};
    };
}

namespace winrt::Notepads::Markdown::factory_implementation
{
    struct MarkdownView : MarkdownViewT<MarkdownView, implementation::MarkdownView>
    {
    };
}
