// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Core/CoreController.h"
#include "Reading/NativeReadingNavigation.h"
#include "Resources/Images/NativeImagePresentation.h"
#include "Reading/NativeHighlightSnapshot.h"
#include "Resources/Math/NativeMathPresentation.h"
#include "Resources/Diagrams/NativeDiagramRenderer.h"
#include "Core/ModuleLifetime.h"
#include "notepads/markdown/preview.h"
#include <d2d1_1.h>
#include <dwrite.h>
#include <winrt/Windows.UI.h>
#include <algorithm>
#include <array>
#include <atomic>
#include <functional>
#include <string>
#include <vector>

namespace Notepads::Markdown::Native
{
    class NativeTextBitmapCache;
    struct NativeTextPrewarm;
    constexpr bool IsPreviewTaskDecoration(uint32_t kind) noexcept
    {
        return kind == NOTEPADS_MARKDOWN_PREVIEW_DECORATION_TASK || kind == NOTEPADS_MARKDOWN_PREVIEW_DECORATION_STATIC_HTML_CHECKBOX;
    }
    // Strict same-build wire validation before any marker measurement or paint.
    void ValidatePreviewDecoration(notepads_markdown_preview_decoration const &decoration);
    void ValidatePreviewHtmlCell(notepads_markdown_preview_cell const &cell, notepads_markdown_preview_html_cell const &metadata);
    void ValidatePreviewHtmlTable(std::span<notepads_markdown_preview_cell const> cells,
                                  std::span<notepads_markdown_preview_html_cell const> metadata,
                                  uint32_t rows, uint32_t columns, std::span<uint32_t> owners);
    class NativeScriptLayout;
    class NativeScriptDrawing;
    class NativeScriptFonts;
    class NativeScriptFontBinding;
    struct CodeBlockStyle
    {
        static constexpr float panelOpacity = 0.6f;
        static constexpr float padding = 14.0f;
        static constexpr float radius = 6.0f;
        static constexpr float buttonSize = 22.0f;
        static constexpr float buttonInset = 7.0f;
        static constexpr float buttonGutter = buttonSize + 2 * buttonInset;

        static constexpr float TextBaseline(float fontSize, float lineHeight) noexcept
        {
            // Center the ink of typical code glyphs in the line box.
            return (lineHeight + 0.75f * fontSize) * 0.5f;
        }

        static constexpr float LeftPadding(float width) noexcept
        {
            return std::min(padding, width * 0.25f);
        }

        static constexpr float RightPadding(float width) noexcept
        {
            const auto left = LeftPadding(width);
            // Keep at least 22 DIPs for text before growing the button gutter.
            return std::clamp(width - left - 22.0f, left, buttonGutter);
        }

        static constexpr bool HasCopySpace(float width) noexcept
        {
            return RightPadding(width) >= buttonSize + buttonInset + 2.0f;
        }
    };

    void *AllocatePreview(notepads_markdown_preview_layout *layout, size_t bytes);
    void FreePreview(void *memory) noexcept;

    template <typename T> struct PreviewAllocator
    {
        using value_type = T;
        notepads_markdown_preview_layout *layout{};
        PreviewAllocator() noexcept = default;
        explicit PreviewAllocator(notepads_markdown_preview_layout *owner) noexcept : layout(owner)
        {
        }
        template <typename U> PreviewAllocator(PreviewAllocator<U> const &other) noexcept : layout(other.layout)
        {
        }
        T *allocate(size_t count)
        {
            if (count > SIZE_MAX / sizeof(T))
                throw std::bad_alloc();
            return static_cast<T *>(AllocatePreview(layout, count * sizeof(T)));
        }
        void deallocate(T *memory, size_t) noexcept
        {
            FreePreview(memory);
        }
        template <typename U> bool operator==(PreviewAllocator<U> const &other) const noexcept
        {
            return layout == other.layout;
        }
    };
    template <typename T> using PreviewVector = std::vector<T, PreviewAllocator<T>>;
    using PreviewString = std::basic_string<wchar_t, std::char_traits<wchar_t>, PreviewAllocator<wchar_t>>;

    struct PreviewFailure : NativeFailure
    {
        wchar_t const *stage{L"preview"};
        explicit PreviewFailure(int32_t value, wchar_t const *where = L"preview") noexcept
            : NativeFailure(value), stage(where)
        {
        }
    };

    struct PreviewTheme
    {
        std::wstring fontFamily{L"Segoe UI"};
        float fontSize{18.0f};
        float lineHeight{27.0f};
        float padding{16.0f};
        // Captured with the viewport; used only for evolving image block occupancy.
        double physicalScale{1.0};
        winrt::Windows::UI::Color foreground{255, 30, 30, 30};
        winrt::Windows::UI::Color background{255, 255, 255, 255};
        winrt::Windows::UI::Color linkForeground{255, 8, 102, 200};
        winrt::Windows::UI::Color selectionForeground{255, 255, 255, 255};
        winrt::Windows::UI::Color selectionBackground{255, 0, 99, 177};
        winrt::Windows::UI::Color borderColor{255, 203, 208, 216};
        winrt::Windows::UI::Color panelBackground{255, 243, 245, 247};
        winrt::Windows::UI::Color focusColor{255, 8, 102, 200};
        // Plain is always foreground; syntax roles use the captured code palette.
        std::array<winrt::Windows::UI::Color, NOTEPADS_MARKDOWN_HIGHLIGHT_ROLE_COUNT> codeForeground{{{255, 30, 30, 30},
                                                                 {255, 90, 112, 81},
                                                                 {255, 163, 21, 21},
                                                                 {255, 9, 134, 88},
                                                                 {255, 0, 0, 180},
                                                                 {255, 38, 127, 153},
                                                                 {255, 121, 94, 38},
                                                                 {255, 17, 99, 41},
                                                                 {255, 130, 7, 30}}};
        std::wstring codeFontFamily{L"Consolas"};
        bool highContrast{};

        static constexpr bool DiffPanelIsDark(winrt::Windows::UI::Color background,
                                              winrt::Windows::UI::Color panel) noexcept
        {
            const auto backgroundBrightness = background.R + background.G + background.B;
            const auto panelBrightness = panel.R + panel.G + panel.B;
            // The host supplies its caret-line tint as a translucent panel.
            const auto alpha = panel.A * CodeBlockStyle::panelOpacity;
            return backgroundBrightness * (255.0f - alpha) + panelBrightness * alpha < 384.0f * 255.0f;
        }

        void ApplyDiffColors() noexcept
        {
            using winrt::Windows::UI::Color;
            bool const dark = DiffPanelIsDark(background, panelBackground);
            codeForeground[NOTEPADS_MARKDOWN_HIGHLIGHT_ADDED] = dark ? Color{255, 126, 231, 135} : Color{255, 17, 99, 41};
            codeForeground[NOTEPADS_MARKDOWN_HIGHLIGHT_REMOVED] = dark ? Color{255, 255, 161, 152} : Color{255, 130, 7, 30};
        }
    };

    static_assert(PreviewTheme::DiffPanelIsDark({255, 30, 30, 30}, {16, 255, 255, 255}));
    static_assert(PreviewTheme::DiffPanelIsDark({255, 30, 30, 30}, {128, 255, 255, 255}));
    static_assert(!PreviewTheme::DiffPanelIsDark({255, 255, 255, 255}, {16, 0, 0, 0}));

    // The UI captures this value together with the ordinary preview request.
    // object/syntax are filled per occurrence; all remaining key fields are
    // immutable base identity. Attempts are Math-only source-order ordinals.
    template <typename Key, typename Handle> struct DerivedPresentations
    {
        struct Entry
        {
            uint32_t object{};
            Handle handle;
        };

        std::array<Entry, NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT> entries;
        Key key{};
        size_t count{};

        Handle const *Find(uint32_t object) const noexcept
        {
            for (size_t i = 0; i < count; ++i)
                if (entries[i].object == object)
                    return &entries[i].handle;
            return nullptr;
        }
        Handle const *Find(Key const &source, uint32_t object) const noexcept
        {
            return key == source ? Find(object) : nullptr;
        }
        void Add(Key const &source, uint32_t object, Handle const &handle) noexcept
        {
            if (!count)
                key = source;
            entries[count++] = {object, handle};
        }
    };

    using MathPresentations = DerivedPresentations<NativeMathKey, MathPresentationHandle>;
    using DiagramPresentations = DerivedPresentations<NativeDiagramKey, DiagramPresentationHandle>;

    struct PreviewMathSnapshot
    {
        MathPresentations presentations;
        NativeMathKey key{};
        uint64_t generation{};
        bool enabled{};
        std::array<NativeMathState, NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT> attempts{};
    };

    struct PreviewDiagramSnapshot
    {
        DiagramPresentations presentations;
        NativeDiagramKey key{};
        uint64_t generation{};
        bool enabled{};
        std::array<NativeDiagramState, NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT> attempts{};
    };

    class PreviewCancellation
    {
      public:
        explicit PreviewCancellation(notepads_markdown_core_model *model);
        ~PreviewCancellation();
        PreviewCancellation(PreviewCancellation const &) = delete;
        PreviewCancellation &operator=(PreviewCancellation const &) = delete;
        void Cancel() noexcept;
        notepads_markdown_core_cancel *Get() const noexcept
        {
            return _cancel;
        }

      private:
        ModuleLifetime _module;
        notepads_markdown_core_cancel *_cancel{};
    };

    struct ListMarkerProbeStatistics
    {
        uint64_t batches{};
        uint64_t cacheHits{};
        uint64_t layouts{};
        uint64_t frameLayouts{};
    };

    struct ReadingHighlightResult
    {
        std::shared_ptr<NativeHighlightSnapshot const> snapshot;
        int32_t status{NOTEPADS_MARKDOWN_CORE_OK};
        uint32_t reusedBlocks{};
    };

    struct ReadingSourceState
    {
        // Constructed on UI before enqueue; identity/owners never change.
        std::shared_ptr<ReadingMemory> const memory;
        std::shared_ptr<PreviewCancellation> const cancel;
        uint64_t const sourceGeneration{};
        uint64_t const textGeneration{};
        bool const deferHighlights{};
        // Synchronous highlight state belongs to the render worker. The
        // viewport worker builds a local result for UI-only publication.
        int32_t fault{};
        std::shared_ptr<NativeHighlightSnapshot const> highlights;
        bool highlightingAttempted{};
        int32_t highlightingStatus{NOTEPADS_MARKDOWN_CORE_OK};
        // Source-worker only; the bounded metric cache also reuses groups across sources.
        bool listMarkerMetricsValid{};
        std::shared_ptr<NativeTextPrewarm> textPrewarm; // Serialized source worker; Frames publish immutable handles.
        ReadingString listMarkerFont;
        ReadingWordSegments wordSegments; // Serialized source worker.
        float listMarkerFontSize{}, listMarkerLineHeight{}, listMarkerAdvance{}, listMarkerLeft{}, listMarkerRight{};
        ReadingSourceState(std::shared_ptr<ReadingMemory> owner, std::shared_ptr<PreviewCancellation> flag,
                           uint64_t source, uint64_t text, bool defer = false)
            : memory(std::move(owner)), cancel(std::move(flag)), sourceGeneration(source), textGeneration(text),
              deferHighlights(defer),
              listMarkerFont(ReadingAllocator<wchar_t>{memory}), wordSegments(memory)
        {
        }
        static std::shared_ptr<ReadingSourceState> Create(CoreModelPin model, uint64_t sourceGeneration,
                                                          bool deferHighlights = false);
        void EnsureMain();
        ReadingHighlightResult BuildHighlights(std::span<uint32_t const> nodes = {},
                                               NativeHighlightCache *cache = nullptr,
                                               PreviewCancellation const *workCancel = nullptr) const noexcept;
        void EnsureHighlights() noexcept;
        // Resource APIs can run on UI before the first Frame is published.
        // Only pointer publication is locked; a null snapshot is still pending.
        std::shared_ptr<MainTextSnapshot const> Main() const noexcept
        {
            winrt::slim_shared_lock_guard guard{_publicationLock};
            return _main;
        }
        std::shared_ptr<SemanticDescriptors const> Descriptors() const noexcept
        {
            winrt::slim_shared_lock_guard guard{_publicationLock};
            return _descriptors;
        }
        // Single source worker (or isolated setup before enqueue) publishes
        // only complete, immutable snapshots. Retire previous owners after
        // unlocking, so destruction cannot reenter the publication lock.
        void PublishMain(std::shared_ptr<MainTextSnapshot const> value) noexcept
        {
            winrt::slim_lock_guard guard{_publicationLock};
            _main.swap(value);
        }
        void PublishDescriptors(std::shared_ptr<SemanticDescriptors const> value) noexcept
        {
            winrt::slim_lock_guard guard{_publicationLock};
            _descriptors.swap(value);
        }
        void Cancel() noexcept
        {
            cancel->Cancel();
        }

      private:
        // SRW storage fits ReadingAllocator's ordinary alignment on every
        // target; atomic<shared_ptr> requires stronger alignment on ARM64.
        mutable winrt::slim_mutex _publicationLock;
        std::shared_ptr<MainTextSnapshot const> _main;
        std::shared_ptr<SemanticDescriptors const> _descriptors;
    };

    struct PreviewLinkVisual
    {
        uint32_t node{NOTEPADS_MARKDOWN_PREVIEW_NONE};
        uint32_t start{};
        uint32_t length{};
        PreviewString destination;
        explicit PreviewLinkVisual(notepads_markdown_preview_layout *owner) : destination(PreviewAllocator<wchar_t>{owner})
        {
        }
    };
    struct PreviewImageVisual
    {
        uint32_t offset{};
        uint32_t object{};
        float x{};
        float y{};
        float width{};
        float height{};
        float baseline{};
        float iconSize{};
        float inset{};
        bool linked{};
        bool pending{};
        bool hasPixels{};
        bool note{};
        winrt::com_ptr<IDWriteTextLayout> label;
    };
    struct ReadingImageHit
    {
        uint32_t offset{};
        bool ready{};
    };

    struct PreviewMathVisual
    {
        uint32_t offset{};
        uint32_t object{};
        uint32_t syntax{};
        NativeMathState state{NativeMathState::Pending};
        // Borrowed only while the renderer/Frame owns its immutable head.
        NativeMathPresentation const *presentation{};
        MathPlacement placement;
        MathTextAnchor anchor;
        float x{};
        float y{};
        float width{};
        float height{};
        float baseline{};
        winrt::com_ptr<IDWriteTextLayout> label;
    };
    struct PreviewDiagramVisual
    {
        uint32_t offset{};
        uint32_t object{};
        uint32_t ordinal{};
        NativeDiagramState state{NativeDiagramState::Pending};
        DiagramPresentationHandle presentation;
        DiagramPlacement placement;
        float x{}, y{}, width{}, height{}, baseline{};
        bool hasInline{};
        winrt::com_ptr<IDWriteTextLayout> label;
        // Premeasured on the layout worker, including Ready -> Draw failures.
        // Paint selects Failed, Limited or Unavailable without changing text
        // formats or allocating a caption layout on the UI thread.
        std::array<winrt::com_ptr<IDWriteTextLayout>, 3> drawFailureLabels;
        mutable DiagramDrawCache drawing;
    };
    struct PreviewInlineCodeVisual
    {
        // Genuine zero-advance text retains its range but paints no box.
        uint32_t start{};
        uint32_t length{};
        D2D1_RECT_F bounds{};
    };
    struct PreviewTextVisual
    {
        winrt::com_ptr<IDWriteTextLayout> layout;
        std::shared_ptr<NativeScriptLayout const> scripts;
        std::shared_ptr<NativeScriptDrawing const> scriptDrawing;
        std::shared_ptr<NativeScriptFontBinding const> scriptFonts;
        ReadingMetricToken metricToken;
        uint64_t logicalStart{};
        uint64_t logicalEnd{};
        uint32_t utf16Length{};
        uint32_t terminalBreak{};
        bool matchesMainText{};
        bool bitmapCandidate{};
        bool prewarmCandidate{};
        float x{};
        float y{};
        float width{};
        float height{};
        PreviewVector<PreviewLinkVisual> links;
        PreviewVector<DWRITE_TEXT_RANGE> authoredUnderlines;
        PreviewVector<PreviewImageVisual> images;
        PreviewVector<PreviewMathVisual> math;
        PreviewVector<PreviewDiagramVisual> diagrams;
        PreviewVector<PreviewInlineCodeVisual> inlineCode;
        PreviewVector<D2D1_RECT_F> highlights;
        explicit PreviewTextVisual(notepads_markdown_preview_layout *owner)
            : links(PreviewAllocator<PreviewLinkVisual>{owner}), authoredUnderlines(PreviewAllocator<DWRITE_TEXT_RANGE>{owner}),
              images(PreviewAllocator<PreviewImageVisual>{owner}),
              math(PreviewAllocator<PreviewMathVisual>{owner}), diagrams(PreviewAllocator<PreviewDiagramVisual>{owner}),
              inlineCode(PreviewAllocator<PreviewInlineCodeVisual>{owner}),
              highlights(PreviewAllocator<D2D1_RECT_F>{owner})
        {
        }
    };
    struct PreviewTableCellVisual
    {
        uint32_t node{};
        uint32_t row{};
        uint32_t column{};
        uint32_t columnSpan{};
        uint32_t rowSpan{};
        uint32_t textIndex{NOTEPADS_MARKDOWN_PREVIEW_NONE};
        uint32_t batchTextIndex{};
        uint32_t flags{};
        D2D1_RECT_F bounds{}; // Block-local, reused unchanged by Frame::Reposition.
    };
    enum class ListMarkerDisplay : uint8_t
    {
        Full,
        Trimmed,
        Clipped
    };
    struct PreviewListMarker
    {
        winrt::com_ptr<IDWriteTextLayout> layout;
        float x{}, y{}, clipWidth{};
        ListMarkerDisplay display{ListMarkerDisplay::Full};
        explicit operator bool() const noexcept
        {
            return static_cast<bool>(layout);
        }
        IDWriteTextLayout *get() const noexcept
        {
            return layout.get();
        }
    };
    struct PreviewCodeLine
    {
        float top{}, bottom{};
        uint32_t number{};
        bool highlighted{};
    };

    struct PreviewBlockVisual
    {
        notepads_markdown_preview_block block{};
        double measuredHeight{};
        // Origin of the nearest owning list item, before any child quote inset.
        double markerX{};
        float taskMarkerTop{};
        float disclosureInset{};
        float codeGutterWidth{};
        PreviewVector<PreviewCodeLine> codeLines;
        PreviewVector<PreviewTextVisual> texts;
        PreviewVector<notepads_markdown_preview_decoration> decorations;
        PreviewVector<float> rowHeights;
        PreviewVector<float> columnEdges;
        bool htmlTable{};
        float tableWidth{};
        PreviewVector<PreviewTableCellVisual> tableCells;
        PreviewVector<uint32_t> tableOwners;
        PreviewVector<uint32_t> tableRowFlags;
        PreviewVector<ReadingTableCaretAlias> tableAliases;
        PreviewVector<PreviewListMarker> markers;
        explicit PreviewBlockVisual(notepads_markdown_preview_layout *owner)
            : codeLines(PreviewAllocator<PreviewCodeLine>{owner}),
              texts(PreviewAllocator<PreviewTextVisual>{owner}),
              decorations(PreviewAllocator<notepads_markdown_preview_decoration>{owner}), rowHeights(PreviewAllocator<float>{owner}),
              columnEdges(PreviewAllocator<float>{owner}),
              tableCells(PreviewAllocator<PreviewTableCellVisual>{owner}),
              tableOwners(PreviewAllocator<uint32_t>{owner}),
              tableRowFlags(PreviewAllocator<uint32_t>{owner}),
              tableAliases(PreviewAllocator<ReadingTableCaretAlias>{owner}),
              markers(PreviewAllocator<PreviewListMarker>{owner})
        {
        }
    };
    struct DiagramDrawOutcome
    {
        // Reposition shares the measured visuals and their UI drawing cache.
        // A new Frame starts with no result from that cache.
        std::atomic<HRESULT> status{S_FALSE};
        DiagramDrawOutcome() noexcept = default;
        DiagramDrawOutcome(DiagramDrawOutcome const &) noexcept {}
        DiagramDrawOutcome &operator=(DiagramDrawOutcome const &other) noexcept
        {
            if (this != &other)
                status.store(S_FALSE, std::memory_order_release);
            return *this;
        }
    };
    struct PreviewFrame
    {
        notepads_markdown_preview_key key{};
        uint64_t token{};
        double viewportY{};
        double viewportHeight{};
        double totalHeight{};
        uint64_t totalBlocks{};
        uint64_t firstBlock{};
        PreviewTheme theme;
        PreviewMathSnapshot math;
        PreviewDiagramSnapshot diagram;
        std::array<NativeMathState, NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT> mathStates{};
        std::array<NativeDiagramState, NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT> diagramStates{};
        mutable std::array<DiagramDrawOutcome, NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT> diagramDrawOutcomes{};
        std::shared_ptr<ImagePresentationSnapshot const> images;
        std::shared_ptr<NativeHighlightSnapshot const> highlights;
        int32_t highlightingStatus{NOTEPADS_MARKDOWN_CORE_OK};
        std::shared_ptr<MainTextSnapshot const> main;
        std::shared_ptr<NativeTextPrewarm> textPrewarm;
        std::shared_ptr<SemanticDescriptors const> descriptors;
        std::shared_ptr<ReadingVisibility const> visibility;
        std::optional<ReadingSection> section;
        std::shared_ptr<ReadingNavigationIndex const> navigation;
        std::shared_ptr<PreviewVector<PreviewBlockVisual> const> measuredBlocks;
        // Complete container geometry survives frames containing only an internal gap.
        std::shared_ptr<PreviewVector<notepads_markdown_preview_quote> const> quotes;
        double measuredTop{};
        double measuredBottom{};

        std::shared_ptr<PreviewFrame> Reposition(double y, double height) const;
        std::shared_ptr<PreviewFrame> WithImages(std::shared_ptr<ImagePresentationSnapshot const> const &snapshot) const;
        std::shared_ptr<ImagePixels const> const &ImagePixelsFor(uint32_t object) const noexcept;
        uint32_t ImageFrameIndex(uint32_t object) const noexcept;
        PreviewVector<PreviewBlockVisual> const &Blocks() const noexcept
        {
            return *measuredBlocks;
        }
        NativeMathState MathState(uint32_t object) const noexcept;
        NativeDiagramState DiagramState(uint32_t object) const noexcept;
        // UI thread only. Move successful SVG resources between separately measured
        // frames; never share a mutable drawing cache with the layout worker.
        void TakeDiagramDrawingFrom(PreviewFrame const &previous) const noexcept;
    };

    struct PreviewNoteEmbedExtent
    {
        uint32_t object{};
        float height{};
    };
    using PreviewNoteEmbedExtents = ReadingVector<PreviewNoteEmbedExtent>;

    struct PreviewImageWidth
    {
        uint32_t object{};
        double width{};
    };
    using PreviewImageWidths = ReadingVector<PreviewImageWidth>;

    // An edit can rebind retained resources after exact semantic projection,
    // before any full-document DirectWrite measurement. This is not a Frame.
    struct PreviewEditResources
    {
        std::shared_ptr<MainTextSnapshot const> main;
        std::shared_ptr<SemanticDescriptors const> descriptors;
        // Null means at least one image needs actual container measurement.
        std::shared_ptr<PreviewImageWidths const> imageWidths;
    };

    struct NoteEmbedStyle
    {
        static constexpr float maximumHeight = 224.0f;
    };

    // Calls are serialized on background workers; UI only consumes immutable Frames.
    class NativeDocumentRenderer
    {
      public:
        struct InitialViewport
        {
            double height;
            uint64_t token;
            std::atomic<uint64_t> const &latestToken;
            std::function<void(std::shared_ptr<PreviewFrame>)> publish;
        };

        // A nonnull editResources requests semantic preparation only. Its
        // returned draft cannot create Frames; ordinary Create measures it.
        static std::shared_ptr<NativeDocumentRenderer> Create(
            CoreModelPin model, std::shared_ptr<PreviewCancellation> cancel, notepads_markdown_preview_key const &key,
            PreviewTheme theme, std::shared_ptr<ReadingSourceState> reading = {},
            std::shared_ptr<ImagePresentationSnapshot const> images = {}, PreviewMathSnapshot math = {},
            PreviewDiagramSnapshot diagram = {}, std::shared_ptr<NativeMeasurementCache> cache = {},
            InitialViewport const *initialViewport = nullptr,
            std::span<notepads_markdown_preview_disclosure_override const> overrides = {},
            std::wstring_view sectionTitle = {},
            std::shared_ptr<PreviewNoteEmbedExtents const> noteEmbeds = {},
            PreviewEditResources *editResources = nullptr);
        NativeDocumentRenderer(CoreModelPin model, std::shared_ptr<PreviewCancellation> cancel,
                               notepads_markdown_preview_layout *layout, notepads_markdown_preview_key const &key, PreviewTheme theme,
                               std::shared_ptr<ReadingSourceState> reading,
                               std::shared_ptr<ImagePresentationSnapshot const> images, PreviewMathSnapshot math = {},
                               PreviewDiagramSnapshot diagram = {}, std::shared_ptr<NativeMeasurementCache> cache = {});
        ~NativeDocumentRenderer();
        NativeDocumentRenderer(NativeDocumentRenderer const &) = delete;
        NativeDocumentRenderer &operator=(NativeDocumentRenderer const &) = delete;
        std::shared_ptr<PreviewFrame> CreateFrame(double y, double height, uint64_t token,
                                                  std::atomic<uint64_t> const &latestToken);
        notepads_markdown_preview_key const &Key() const noexcept
        {
            return _key;
        }
        notepads_markdown_preview_extent const &Extent() const noexcept
        {
            return _extent;
        }
        ListMarkerProbeStatistics const &ListMarkerStatistics() const noexcept
        {
            return _listMarkerStats;
        }
        void Cancel() noexcept
        {
            _cancel->Cancel();
        }
        // Optional paintClip must conservatively contain all visible paint in local DIPs
        // before the context transform. Default callers retain unrestricted Draw behavior.
        static void Draw(PreviewFrame const &frame, ID2D1DeviceContext *context, float originX, float originY,
                         uint32_t selectionStart = 0, uint32_t selectionEnd = 0, ReadingActionTarget focusedAction = {},
                         bool focused = false, ImageDrawCache *imageCache = nullptr, bool markInactiveCaret = false,
                         std::optional<size_t> hoveredLink = {}, std::optional<D2D1_RECT_F> paintClip = {},
                         NativeTextBitmapCache *textCache = nullptr, std::function<void()> const *repaint = nullptr,
                         bool allowPendingText = true,
                         std::shared_ptr<NativeHighlightSnapshot const> const &startupHighlights = {},
                         bool trailingCaret = false);
        static std::optional<ReadingImageHit> ImageAt(PreviewFrame const &frame, float x, float y) noexcept;
        static std::optional<uint32_t> MathFallbackAt(PreviewFrame const &frame, float x, float y) noexcept;
        static std::optional<ReadingHit> HitTest(PreviewFrame const &frame, float x, float y);
        static std::optional<uint32_t> DisclosureAt(PreviewFrame const &frame, float x, float y) noexcept;
        static std::optional<ReadingHit> HitTestLine(PreviewFrame const &frame, size_t lineIndex, float preferredX);
        static ReadingVector<ReadingRect> Rectangles(PreviewFrame const &frame, uint32_t start, uint32_t end,
                                                    bool trailingCaret = false);
        static ReadingVector<ReadingRect> LineRectangles(PreviewFrame const &frame, ReadingLine const &line);

      private:
        static ReadingVector<ReadingRect> RangeRectangles(PreviewFrame const &frame, uint32_t start, uint32_t end,
                                                         std::span<PreviewBlockVisual const> blocks,
                                                         uint32_t textIndex = NOTEPADS_MARKDOWN_PREVIEW_NONE);
        static std::optional<ReadingHit> HitTestWithin(PreviewFrame const &frame, float x, float y,
                                                       ReadingLine const *line);
        ModuleLifetime _module;
        CoreModelPin _model;
        std::optional<ReadingSection> _section;
        std::shared_ptr<PreviewCancellation> _cancel;
        notepads_markdown_preview_layout *_layout{};
        notepads_markdown_preview_key _key{};
        notepads_markdown_preview_extent _extent{};
        ListMarkerProbeStatistics _listMarkerStats;
        float _markerLeftGuard{}, _markerRightGuard{};
        winrt::com_ptr<IDWriteTextFormat> _markerFormat;
        winrt::com_ptr<IDWriteInlineObject> _markerEllipsis;
        std::optional<float> _codeDigitWidth;
        PreviewTheme _theme;
        std::shared_ptr<ImagePresentationSnapshot const> _images;
        PreviewMathSnapshot _math;
        PreviewDiagramSnapshot _diagram;
        std::shared_ptr<PreviewNoteEmbedExtents const> _noteEmbeds;
        std::array<uint32_t, NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT> _mathObjects{};
        std::array<NativeMathState, NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT> _mathStates{};
        std::array<winrt::com_ptr<IDWriteInlineObject>, NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT> _mathInlines;
        uint32_t _mathObjectCount{};
        std::array<uint32_t, NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT> _diagramObjects{};
        std::array<NativeDiagramState, NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT> _diagramStates{};
        std::array<winrt::com_ptr<IDWriteInlineObject>, NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT> _diagramInlines;
        uint32_t _diagramObjectCount{};
        bool _measuring{};
        bool _initialViewportAttempted{};
        std::shared_ptr<PreviewFrame> _initialFrame;
        winrt::com_ptr<IDWriteFactory> _factory;
        struct TextFormatKey
        {
            IDWriteFactory *factory{};
            DWRITE_FONT_WEIGHT weight{};
            DWRITE_TEXT_ALIGNMENT alignment{};
            float fontSize{}, spacing{}, baseline{};
            bool code{}, noWrap{};

            bool operator==(TextFormatKey const &) const = default;
        };
        struct TextFormatEntry
        {
            TextFormatKey key;
            winrt::com_ptr<IDWriteTextFormat> format;
        };
        // The theme and factories are immutable for this renderer. The worker
        // uses these formats for both measurement and new viewport frames.
        std::array<TextFormatEntry, 16> _textFormats{};
        size_t _nextTextFormat{};
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        uint64_t _textFormatCreated{}, _textFormatReused{};
#endif
        winrt::com_ptr<IDWriteTextFormat> TextFormat(TextFormatKey const &key);
        std::shared_ptr<NativeScriptFonts> _scriptFonts;
        std::shared_ptr<ReadingSourceState> _reading;
        // Captured once on the serialized worker; hot measurement/frame paths
        // reuse their immutable pins without additional atomic loads.
        std::shared_ptr<MainTextSnapshot const> _main;
        std::shared_ptr<SemanticDescriptors const> _descriptors;
        std::shared_ptr<NativeMeasurementCache> _measurementCache;
        uint64_t _measurementEpoch{};
        std::shared_ptr<ReadingVisibility const> _visibility;
        std::shared_ptr<ReadingNavigationIndex> _navigation;
        std::shared_ptr<PreviewVector<notepads_markdown_preview_quote> const> _quotes;
        std::shared_ptr<SemanticDescriptors> _semanticBuilder;
        struct Presentation;
        void CollectNavigation(notepads_markdown_preview_block const &block, PreviewBlockVisual const &visual, uint32_t blockIndex);
        void CollectSemantics(Presentation const &batch, notepads_markdown_preview_block const &block);
        void ReadVisibility();
        bool ReadUnfilteredSemantics(PreviewImageWidths *imageWidths = nullptr);
        void FinishSemantics();
        void PrepareEditResources(PreviewEditResources &resources);
        void Measure(InitialViewport const *initialViewport);
        std::shared_ptr<PreviewFrame> BuildFrame(double y, double height, uint64_t token,
            std::atomic<uint64_t> const &latestToken, bool prefix);
        void ReadQuotes();
        Presentation Read(notepads_markdown_preview_summary const &summary, notepads_markdown_preview_layout *layout = nullptr);
        PreviewBlockVisual MeasureBlock(Presentation const &batch, notepads_markdown_preview_block const &block);
        void PlaceListMarkers(PreviewBlockVisual &visual);
        void PlaceCodeLines(PreviewBlockVisual &visual, std::span<std::array<uint32_t, 2> const> highlights);
        void MeasureHtmlTable(Presentation const &batch, notepads_markdown_preview_block const &block, PreviewBlockVisual &result);
        PreviewTextVisual MeasureText(Presentation const &batch, notepads_markdown_preview_text const &text, float width,
                                      float fontSize, bool code, bool header, uint32_t alignment,
                                      bool codeBlock = false, bool headingBlock = false, bool noWrap = false,
                                      bool centerLineInk = false);
        uint32_t MathOrdinal(uint32_t object);
        uint32_t DiagramOrdinal(uint32_t object);
        void SetMathFallback(PreviewMathVisual &visual, IDWriteTextLayout *layout, IDWriteTextFormat *format,
                             std::wstring_view content, std::wstring_view input, float width, float fontSize);
        void SetDiagramFallback(PreviewDiagramVisual &visual, IDWriteTextLayout *layout, IDWriteTextFormat *format,
                                std::wstring_view content, float width, float fontSize);
        winrt::com_ptr<IDWriteTextLayout> DiagramCaption(NativeDiagramState state, IDWriteTextLayout *layout,
                                                       IDWriteTextFormat *format, uint32_t offset, float width,
                                                       float height, float fontSize);
    };
}
