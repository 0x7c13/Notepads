// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "NativeReadingSnapshotTests.h"
#include "NativeControlConcurrencyTests.h"
#include "Reading/NativeReadingNavigation.h"
#include "Reading/NativeReadingSnapshot.h"
#include "Rendering/NativeDocumentRenderer.h"
#include "Rendering/NativeMeasurementCache.h"
#include "Text/NativeScriptLayout.h"
#include "NativeMathGeometryTests.h"
#include "NativeMathWorkerTests.h"
#include "NativeMathCoordinatorTests.h"
#include "NativeDocumentRendererTests.h"
#include "NativeInlineCodeTests.h"
#include "NativeScriptTests.h"
#include "NativeDiagramRendererTests.h"
#include "NativeMeasurementCacheTests.h"
#include "NativeTextBitmapCacheTests.h"
#include "NativeFinalSurfaceThreadTests.h"
#include "NativeReadingSearchTests.h"
#include "NativeReadingOutlineTests.h"
#include "NativeTaskInteractionTests.h"
#include "Reading/NativeSourceNavigation.h"
#include "Control/MarkdownOutline.h"
#include "Control/MarkdownView.h"
#include "Control/PreviewUpdate.h"
#include "Automation/TextRangeProvider.h"
#include "Automation/ReadingObjectProvider.h"
#include <algorithm>
#include <array>
#include <chrono>
#include <cstring>
#include <functional>
#include <string_view>
#include <thread>
#include <type_traits>
#include <utility>

namespace winrt::Notepads::Markdown::implementation
{
    struct PreviewPublicationTestAccess
    {
        static std::wstring ViewportWaitDetail(Markdown::MarkdownView const &view)
        {
            auto owner = get_self<MarkdownView>(view);
            auto id = [](auto const &value) { return static_cast<uint64_t>(reinterpret_cast<uintptr_t>(get_abi(value))); };
            auto update = owner->_previewUpdate;
            const auto updateState = update ? update->state.load(std::memory_order_acquire) : UINT32_MAX;
            std::wstring text = L"upd=" + std::to_wstring(updateState) + L"/" +
                std::to_wstring(updateState == 3 ? static_cast<int>(update->status) : -1) + L"/" +
                std::to_wstring(updateState == 3 ? update->error : 0) +
                L" src=" + std::to_wstring(owner->_previewSubmittedSourceGeneration) + L"/" +
                std::to_wstring(owner->_previewSourceGeneration) +
                L" parts=" + std::to_wstring(static_cast<bool>(owner->_previewScroll)) +
                std::to_wstring(static_cast<bool>(owner->_previewExtent)) +
                std::to_wstring(static_cast<bool>(owner->_viewportOverlay)) +
                std::to_wstring(static_cast<bool>(owner->_surfaceTarget)) +
                L" SIS=" + std::to_wstring(id(owner->_surface)) +
                L" ready=" + std::to_wstring(owner->_surfaceReady) + L" loaded=" + std::to_wstring(owner->_loaded) +
                L" accepted=" + std::to_wstring(owner->_documentAccepted) +
                L" model=" + std::to_wstring(owner->_previewModelReady) +
                L" frame=" + std::to_wstring(static_cast<bool>(owner->_presented.frame)) +
                L" active=" + std::to_wstring(static_cast<bool>(owner->_viewportActive)) +
                L" rendering=" + std::to_wstring(static_cast<bool>(owner->_viewportRendering)) +
                L" template=" + std::to_wstring(static_cast<int>(owner->_viewportTemplateState)) +
                L" actual=" + std::to_wstring(view.ActualWidth()) + L"x" + std::to_wstring(view.ActualHeight()) +
                L" error=" + std::wstring{owner->_renderingError.c_str()} +
                L" serial=" + std::to_wstring(owner->_previewSurfaceSerial) +
                L" D2D=" + std::to_wstring(reinterpret_cast<uintptr_t>(owner->_d2dDevice.get())) +
                L" viewport=" + std::to_wstring(owner->_viewportGeometry.pixelWidth) + L"x" +
                std::to_wstring(owner->_viewportGeometry.pixelHeight);
            return text;
        }
        static void ViewportWaitRequire(bool condition, Markdown::MarkdownView const &view, wchar_t const *stage)
        {
            if (!condition)
                throw hresult_error(E_FAIL, std::wstring{L"Preview receipt "} + stage + L": " + ViewportWaitDetail(view));
        }
        static bool ViewportWaitSettled(Markdown::MarkdownView const &view)
        {
            auto owner = get_self<MarkdownView>(view);
            owner->CheckUiThread();
            return owner->_loaded && view.IsLoaded() && owner->_surfaceReady && owner->_presented.frame &&
                !owner->_viewportActive && !owner->_viewportRendering &&
                owner->CurrentViewportFrame(*owner->_presented.frame, owner->_presented.geometry);
        }
        static std::shared_ptr<PreviewUpdate> RefreshIntoViewportWait(Markdown::MarkdownView const &view)
        {
            auto owner = get_self<MarkdownView>(view);
            owner->CheckUiThread();
            ViewportWaitRequire(ViewportWaitSettled(view), view, L"NotLoaded refresh regression starts current");
            view.Refresh();
            // No UI yield/COM call between returning from Refresh and retiring
            // its pending receipt. Parse observation always posts back to UI.
            auto retired = owner->_previewUpdate;
            ViewportWaitRequire(retired && retired->Pending() && !retired->fullSynchronization &&
                !owner->_previewModelReady && !owner->_presented.frame && owner->_previewTicket.Serial() &&
                retired->sourceGeneration == owner->_previewSourceGeneration &&
                owner->_previewSubmittedSourceGeneration == owner->_previewSourceGeneration,
                view, L"Refresh has an actual pending async parse before UI yield");
            retired->CompleteViewportWait();
            ViewportWaitRequire(retired->state.load(std::memory_order_acquire) == 3 &&
                retired->status == Markdown::DocumentUpdateStatus::NotLoaded && !retired->published && !retired->error &&
                !retired->blocks && !retired->layoutGeneration.load(std::memory_order_acquire) &&
                ::WaitForSingleObject(retired->completed.get(), 0) == WAIT_OBJECT_0,
                view, L"geometry wait retires the pending refresh receipt as NotLoaded");
            return retired;
        }
        static bool ViewportWaitRecovered(Markdown::MarkdownView const &view, std::shared_ptr<PreviewUpdate> const &retired)
        {
            auto owner = get_self<MarkdownView>(view);
            owner->CheckUiThread();
            ViewportWaitRequire(retired && retired->state.load(std::memory_order_acquire) == 3 &&
                retired->status == Markdown::DocumentUpdateStatus::NotLoaded && !retired->published && !retired->error &&
                !retired->blocks && !retired->layoutGeneration.load(std::memory_order_acquire),
                view, L"old NotLoaded receipt remains terminal and unchanged during recovery");
            auto current = owner->_previewUpdate;
            if (!current || current == retired || !ViewportWaitSettled(view))
                return false;
            const auto state = current->state.load(std::memory_order_acquire);
            if (state != 3)
                return false;
            ViewportWaitRequire(current->status == Markdown::DocumentUpdateStatus::Ready && current->published &&
                !current->fullSynchronization && owner->_previewModelReady &&
                current->document == retired->document && current->revision == retired->revision &&
                current->sourceGeneration == retired->sourceGeneration &&
                current->sourceGeneration == owner->_previewSourceGeneration &&
                view.DisplayedDocumentId() == retired->document && view.DisplayedRevision() == retired->revision,
                view, L"successful parse establishes a new current internal receipt without rewriting NotLoaded");
            return true;
        }
        static std::wstring ViewportWaitState(Markdown::MarkdownView const &view)
        {
            auto owner = get_self<MarkdownView>(view);
            const auto installed = owner->_previewController ? owner->_previewController->Snapshot().lastInstalled : 0;
            return L"ticket=" + std::to_wstring(owner->_previewTicket.Serial()) + L"/" +
                std::to_wstring(owner->_previewTicket.Status()) + L" installed=" + std::to_wstring(installed) +
                L" " + ViewportWaitDetail(view);
        }

        static void DeliverUnload(Markdown::MarkdownView const &view)
        {
            get_self<MarkdownView>(view)->OnUnloaded(view, nullptr);
        }

        static void DeliverLoad(Markdown::MarkdownView const &view)
        {
            get_self<MarkdownView>(view)->OnLoaded(view, nullptr);
        }

        static bool NativeLoaded(Markdown::MarkdownView const &view)
        {
            return get_self<MarkdownView>(view)->_loaded;
        }

        static uint64_t Generation(Markdown::MarkdownView const &view)
        {
            return get_self<MarkdownView>(view)->_generation;
        }

        static uint64_t MathGeneration(Markdown::MarkdownView const &view)
        {
            auto const &frame = get_self<MarkdownView>(view)->_presented.frame;
            return frame ? frame->math.generation : 0;
        }

        static com_ptr<ReadingObjectProvider> MathPeer(Markdown::MarkdownView const &view, size_t ordinal)
        {
            using namespace ::Notepads::Markdown::Native;
            auto owner = get_self<MarkdownView>(view);
            if (!owner->ReadingAvailable())
                return {};
            auto snapshot = owner->AutomationSnapshot();
            auto const &objects = snapshot->descriptors->objects;
            for (size_t index = 0; index < objects.size(); ++index)
                if (objects[index].kind == ReadingObjectKind::Math && ordinal-- == 0)
                    return owner->ReadingPeer(snapshot, ReadingChildKind::Object, index);
            return {};
        }
    };
}

namespace Notepads::Markdown::Native::ReadingSnapshotTests
{
    namespace
    {
        enum class Case
        {
            Inline,
            LineEndings,
            Structured,
            Empty,
            MultipleBatches,
            Ownership,
            Math,
            Diagram,
            Measurement,
            Search,
            Outline,
            OutlineEmpty,
            TaskInteraction,
            InlineCode,
            Scripts,
            Ruby,
            StaticHtmlCheckbox,
            HtmlTables,
            TextBitmap,
            StaticTextBitmap,
            ListMarkers,
            TablePageNavigation,
            WrappedTablePageNavigation,
            SpanningCellNavigation,
            SpanningCellPageNavigation,
            WrappedPointerCaret,
            WideTableLineIndex,
            MathFallbackLineBreak,
            ScriptLineIndex,
            EditImageWidths,
            FinalSurfaceThread
        };

        constexpr std::array Cases{Case::Inline,          Case::LineEndings, Case::Structured, Case::Empty,
                                    Case::MultipleBatches, Case::Ownership,   Case::Math,       Case::Diagram,
                                    Case::Measurement, Case::Search, Case::Outline, Case::OutlineEmpty, Case::TaskInteraction,
                                    Case::InlineCode, Case::Scripts, Case::Ruby, Case::StaticHtmlCheckbox, Case::HtmlTables, Case::TextBitmap,
                                    Case::StaticTextBitmap, Case::ListMarkers, Case::TablePageNavigation,
                                    Case::WrappedTablePageNavigation, Case::SpanningCellNavigation,
                                    Case::SpanningCellPageNavigation, Case::WrappedPointerCaret, Case::WideTableLineIndex,
                                    Case::MathFallbackLineBreak, Case::ScriptLineIndex, Case::EditImageWidths, Case::FinalSurfaceThread};

        struct Fixture
        {
            wchar_t const *name;
            std::string source;
            std::wstring expected;
            uint32_t extensions{};
        };

        struct AssertionFailure
        {
            int32_t status;
            std::wstring detail;
        };

        Fixture Input(Case kind)
        {
            switch (kind)
            {
            case Case::EditImageWidths:
                return {L"reading_edit_image_width_proof_matches_measured_tables",
                        "![plain](plain)\n\n"
                        "| A | B | C |\n|---|---|---|\n| ![md0](md0) | ![md1](md1) | ![md2](md2) |\n\n"
                        "<table><tr><td colspan=2><img src='span'></td><td rowspan=2><img src='row'></td></tr>"
                        "<tr><td><img src='cell'></td><td>Tail</td></tr></table>\n\n"
                        "<p><img src='html'></p>",
                        L"\xfffc\nA\tB\tC\n\xfffc\t\xfffc\t\xfffc\n\xfffc\t\t\xfffc\n\xfffc\tTail\t\n\xfffc",
                        NOTEPADS_MARKDOWN_CORE_EXTENSIONS_HTML_BLOCKS | NOTEPADS_MARKDOWN_CORE_EXTENSIONS_HTML_INLINE};
            case Case::TablePageNavigation:
                return {L"reading_table_page_navigation_document_edges",
                        "| H0 | H1 | H2 |\n|---|---|---|\n| A0 | A1 | A2 |\n| B0 | B1 | B2 |\n",
                        L"H0\tH1\tH2\nA0\tA1\tA2\nB0\tB1\tB2"};
            case Case::SpanningCellNavigation:
                return {L"reading_table_colspan_navigation_across_cell_padding",
                        "<table><tr><td colspan='2'>iiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiii</td></tr>"
                        "<tr><td>A</td><td>B</td></tr></table>",
                        L"iiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiii\t\nA\tB",
                        NOTEPADS_MARKDOWN_CORE_EXTENSIONS_HTML_BLOCKS};
            case Case::SpanningCellPageNavigation:
                return {L"reading_table_page_fallback_ranks_cells_on_same_row",
                        "<table><tr><td colspan='2'>Header</td><td rowspan='2'>" + std::string(100, 'M') +
                            "</td></tr><tr><td>Left</td><td>Right</td></tr></table>",
                        L"Header\t\t" + std::wstring(100, L'M') + L"\nLeft\tRight\t",
                        NOTEPADS_MARKDOWN_CORE_EXTENSIONS_HTML_BLOCKS};
            case Case::WrappedPointerCaret:
                return {L"reading_pointer_hit_preserves_shared_wrap_side", std::string(36, 'M'), std::wstring(36, L'M')};
            case Case::WideTableLineIndex:
            {
                Fixture value{L"reading_wide_table_lines_address_their_materialized_cell", {}, {}};
                for (uint32_t row = 0; row < 64; ++row)
                {
                    for (uint32_t column = 0; column < 16; ++column)
                    {
                        value.source += "| x ";
                        if (column)
                            value.expected += L'\t';
                        value.expected += L'x';
                    }
                    value.source += "|\n";
                    if (row == 0)
                    {
                        for (uint32_t column = 0; column < 16; ++column)
                            value.source += "| --- ";
                        value.source += "|\n";
                    }
                    if (row + 1 < 64)
                        value.expected += L'\n';
                }
                return value;
            }
            case Case::MathFallbackLineBreak:
                return {L"reading_math_fallback_normalizes_mandatory_line_breaks",
                        "Before $a\u0085b$ after\n\nNext",
                        L"Before \xfffc after\nNext",
                        NOTEPADS_MARKDOWN_CORE_EXTENSIONS_MATH};
            case Case::ScriptLineIndex:
                return {L"reading_visible_line_index_includes_nested_script_geometry",
                        "Before <sub><sub><sub><sub><sub><sub><sub><sub>W</sub></sub></sub></sub></sub></sub></sub></sub>\n\n"
                        "Middle <sup><sup><sup><sup><sup><sup><sup><sup>W</sup></sup></sup></sup></sup></sup></sup></sup>\n\nAfter",
                        L"Before W\nMiddle W\nAfter",
                        NOTEPADS_MARKDOWN_CORE_EXTENSIONS_HTML_INLINE};
            case Case::WrappedTablePageNavigation:
                return {L"reading_table_page_navigation_wrapped_final_row",
                        "| H0 | H1 | H2 |\n|---|---|---|\n| A0 | A1 | A2 |\n"
                        "| B0 | B1 wrap wrap wrap wrap wrap wrap wrap wrap wrap wrap wrap wrap wrap wrap wrap wrap | B2 |\n",
                        L"H0\tH1\tH2\nA0\tA1\tA2\nB0\t"
                        L"B1 wrap wrap wrap wrap wrap wrap wrap wrap wrap wrap wrap wrap wrap wrap wrap wrap\tB2"};
            case Case::ListMarkers:
            {
                Fixture value{L"reading_list_marker_gutter_batch_cache_and_narrow_restore", "99. row\n", L"row"};
                for (size_t i = 1; i < 513; ++i)
                {
                    value.source += "1. row\n";
                    value.expected += L"\nrow";
                }
                value.source += "\nseparator\n\n999. carry\n1. carry\n\nseparator\n\n"
                                "999999999. high\n1. high\n\nseparator\n\n"
                                "1. # Heading\n\n2. > quote\n\n3. ```text\n   code\n   ```\n\n"
                                "separator\n\n> 1. outer\n>    1. inner\n\n- [x] task\n";
                value.expected += L"\nseparator\ncarry\ncarry\nseparator\nhigh\nhigh\nseparator\n"
                                  L"Heading\nquote\ncode\n\nseparator\nouter\ninner\n\xfffc task";
                return value;
            }
            case Case::TextBitmap:
                return {L"reading_text_bitmap_cache_pixels_and_lifetime",
                        std::string{TextBitmapCacheTests::FixtureSource}, std::wstring{TextBitmapCacheTests::FixtureMain}};
            case Case::FinalSurfaceThread:
                return {L"reading_ui_begun_worker_resumed_surface_pixels_and_cleanup",
                        std::string{FinalSurfaceThreadTests::TablePacketFixtureSource},
                        std::wstring{FinalSurfaceThreadTests::TablePacketFixtureMain}};
            case Case::StaticTextBitmap:
                return {L"reading_static_text_bitmap_cache_pixels_and_lifetime",
                        std::string{TextBitmapCacheTests::StaticFixtureSource}, std::wstring{TextBitmapCacheTests::StaticFixtureMain}};
            case Case::HtmlTables:
                return {L"reading_html_table_spans_caption_geometry_and_lifetime",
                        std::string{InlineCodeTests::HtmlTableFixtureSource},
                        std::wstring{InlineCodeTests::HtmlTableFixtureMain}, NOTEPADS_MARKDOWN_CORE_EXTENSIONS_HTML_BLOCKS};
            case Case::StaticHtmlCheckbox:
                return {L"reading_static_html_checkbox_geometry_mapping_and_lifetime",
                        std::string{TaskInteractionTests::StaticFixtureSource},
                        std::wstring{TaskInteractionTests::StaticFixtureMain}, NOTEPADS_MARKDOWN_CORE_EXTENSIONS_HTML_BLOCKS};
            case Case::Ruby:
                return {L"reading_ruby_annotation_rendering_and_child_geometry",
                        "<p>Start <a href='#target'><del><ins><ruby>漢字<rp>(</rp><rt>read</rt><rp>)</rp>"
                        "</ruby></ins></del></a> tail <ruby>文<rt>text</rt>字<rt>ji</rt></ruby> ending. "
                        "<a href='#target'><img src='icon.png' alt='Linked icon' width='24' height='18'></a></p>",
                        L"Start 漢字read tail 文text字ji ending. \xfffc",
                        NOTEPADS_MARKDOWN_CORE_EXTENSIONS_HTML_BLOCKS | NOTEPADS_MARKDOWN_CORE_EXTENSIONS_HTML_INLINE};
            case Case::Scripts:
                return {L"reading_script_shaping_scopes_objects_cache_and_lifetime",
                        std::string{ScriptTests::FixtureSource} + std::string{ScriptTests::NearbySuffixSource},
                        std::wstring{ScriptTests::FixtureMain} + std::wstring{ScriptTests::NearbySuffixMain},
                        NOTEPADS_MARKDOWN_CORE_EXTENSIONS_HTML_INLINE | NOTEPADS_MARKDOWN_CORE_EXTENSIONS_MATH};
            case Case::InlineCode:
                return {L"reading_inline_code_badges_geometry_pixels_and_selection",
                        std::string{InlineCodeTests::FixtureSource}, std::wstring{InlineCodeTests::FixtureMain},
                        NOTEPADS_MARKDOWN_CORE_EXTENSIONS_HIGHLIGHT | NOTEPADS_MARKDOWN_CORE_EXTENSIONS_HTML_INLINE};
            case Case::TaskInteraction:
                return {L"reading_task_action_transport_cluster_focus_and_lifetime",
                        std::string{TaskInteractionTests::FixtureSource},
                        std::wstring{TaskInteractionTests::FixtureMain}};
            case Case::Outline:
                return {L"reading_outline_metadata_fragments_budget_and_lifetime",
                        std::string{ReadingOutlineTests::FixtureSource},
                        std::wstring{ReadingOutlineTests::FixtureMain}};
            case Case::OutlineEmpty:
                return {L"reading_outline_empty_headings_current_block_geometry",
                        std::string{ReadingOutlineTests::FixtureEmptySource},
                        std::wstring{ReadingOutlineTests::FixtureEmptyMain}};
            case Case::Measurement:
                return {L"reading_measurement_cache_geometry_and_lifetime",
                        std::string{MeasurementCacheTests::FixtureSource},
                        std::wstring{MeasurementCacheTests::FixtureMain}};
            case Case::Search:
                return {L"reading_search_literal_budget_and_permits",
                        "**中&amp;😀** é 👩🏽‍💻 שלום ~~done~~ [label](dest) <b>x</b> ![alt](img)",
                        L"中&😀 é 👩🏽‍💻 שלום done label <b>x</b> \xfffc"};
            case Case::Inline:
            case Case::Ownership:
                return {kind == Case::Inline ? L"reading_snapshot_inline_main" : L"reading_snapshot_faults_and_owners",
                        "**中&amp;😀** é 👩🏽‍💻 שלום ~~done~~ [label](dest) <b>x</b> ![alt](img)",
                        L"中&😀 é 👩🏽‍💻 שלום done label <b>x</b> \xfffc"};
            case Case::LineEndings:
                return {L"reading_snapshot_cr_lf_crlf", "a\r\nb  \r\nc\rd\ne", L"a b\nc d e"};
            case Case::Structured:
                return {L"reading_snapshot_table_task_image",
                        "| A | B |\r\n|---|---|\r\n|x|y|\r\n\r\n- [x] \r\n- [ ] label\r\n\r\n![alt](img)\r\n\r\n"
                        "- parent\r\n\r\n  continuation\r\n\r\n  ```\r\n  code\r\n  ```\r\n\r\n  > list quote 中😀\r\n\r\n"
                        "- [x] \r\n\r\n  > task quote\r\n\r\n- > quote first\r\n\r\n"
                        "> outer\r\n>\r\n> - list\r\n>   > nested\r\n>\r\n> tail\r\n\r\n"
                        "> - [ ] \r\n>\r\n> finish\r\n\r\n> sibling\r\n",
                        L"A\tB\nx\ty\n\xfffc\n\xfffc label\n\xfffc\nparent\ncontinuation\ncode\n\n"
                        L"list quote 中😀\n\xfffc\ntask quote\nquote first\nouter\nlist\nnested\ntail\n"
                        L"\xfffc\nfinish\nsibling"};
            case Case::Empty:
                return {L"reading_snapshot_empty_main", "[ref]: /target\r\n", L""};
            case Case::Math:
                return {L"reading_snapshot_math_source_export", "pre $a\r\nb$ post", L"pre \xfffc post",
                        NOTEPADS_MARKDOWN_CORE_EXTENSIONS_MATH};
            case Case::Diagram:
                return {L"reading_snapshot_diagram_native_artifact",
                        "```mermaid\r\nflowchart LR\r\nA[Start] --> B[Done]\r\n```\r\n", L"\xfffc",
                        NOTEPADS_MARKDOWN_CORE_EXTENSIONS_MERMAID};
            case Case::MultipleBatches:
            {
                Fixture value{L"reading_snapshot_scalar_batch_boundary", {}, {}};
                for (size_t index = 0; index != 12000; ++index)
                {
                    value.source += "a😀中";
                    value.expected += L"a😀中";
                }
                return value;
            }
            }
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
        }

        template <typename T> T Pod() noexcept
        {
            T value{};
            value.header = {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, static_cast<uint32_t>(sizeof(T))};
            return value;
        }

        void Require(bool condition, int32_t status = NOTEPADS_MARKDOWN_CORE_INVALID_STATE)
        {
            if (!condition)
                throw AssertionFailure{status, L"Reading snapshot fixed assertion failed."};
        }

        using ReadingRange = winrt::Windows::UI::Xaml::Automation::Provider::ITextRangeProvider;
        using RangeProvider = winrt::Notepads::Markdown::implementation::TextRangeProvider;
        using RangeEndpoint = winrt::Windows::UI::Xaml::Automation::Text::TextPatternRangeEndpoint;
        using RangeUnit = winrt::Windows::UI::Xaml::Automation::Text::TextUnit;
        using RawReadingProvider = winrt::Windows::UI::Xaml::Automation::Provider::IRawElementProviderSimple;

        struct RangeReentrancyState
        {
            ReadingRange sole{nullptr};
            winrt::weak_ref<RangeProvider> weak;
            bool called{}, aliveDuringCallout{};
        };

        // A separate legitimate Range/identity object. Its identity call releases
        // the caller's last victim reference; E_FAIL avoids dereferencing the
        // old implementation's freed State after recording that early death.
        struct ReleasingRange : winrt::implements<ReleasingRange, ReadingRange, IRangeIdentity>
        {
            explicit ReleasingRange(RangeReentrancyState &state) noexcept : _state(state) {}
            HRESULT __stdcall GetRangeIdentity(uint64_t *token, int32_t *start, int32_t *end) noexcept override
            {
                if (!token || !start || !end)
                    return E_POINTER;
                *token = 0;
                *start = *end = 0;
                _state.called = true;
                _state.sole = nullptr;
                _state.aliveDuringCallout = static_cast<bool>(_state.weak.get());
                return E_FAIL;
            }
            ReadingRange Clone() { return nullptr; }
            bool Compare(ReadingRange const &) { return false; }
            int32_t CompareEndpoints(RangeEndpoint, ReadingRange const &, RangeEndpoint) { return 0; }
            void ExpandToEnclosingUnit(RangeUnit) {}
            ReadingRange FindAttribute(int32_t, winrt::Windows::Foundation::IInspectable const &, bool) { return nullptr; }
            ReadingRange FindText(winrt::hstring const &, bool, bool) { return nullptr; }
            winrt::Windows::Foundation::IInspectable GetAttributeValue(int32_t) { return nullptr; }
            void GetBoundingRectangles(winrt::com_array<double> &value) { value = {}; }
            RawReadingProvider GetEnclosingElement() { return nullptr; }
            winrt::hstring GetText(int32_t) { return {}; }
            int32_t Move(RangeUnit, int32_t) { return 0; }
            int32_t MoveEndpointByUnit(RangeEndpoint, RangeUnit, int32_t) { return 0; }
            void MoveEndpointByRange(RangeEndpoint, ReadingRange const &, RangeEndpoint) {}
            void Select() {}
            void AddToSelection() {}
            void RemoveFromSelection() {}
            void ScrollIntoView(bool) {}
            winrt::com_array<RawReadingProvider> GetChildren() { return {}; }

          private:
            RangeReentrancyState &_state;
        };

        // A legitimate Boolean IPropertyValue passed through FindAttribute's
        // public ABI. Its getter may reenter the caller and release a range.
        struct ReleasingProperty : winrt::implements<ReleasingProperty, winrt::Windows::Foundation::IPropertyValue>
        {
            explicit ReleasingProperty(RangeReentrancyState &state)
                : _state(state), _boxed(winrt::Windows::Foundation::PropertyValue::CreateBoolean(true).as<winrt::Windows::Foundation::IPropertyValue>()) {}
            winrt::Windows::Foundation::PropertyType Type() { return _boxed.Type(); }
            bool IsNumericScalar() { return _boxed.IsNumericScalar(); }
            bool GetBoolean()
            {
                _state.called = true;
                _state.sole = nullptr;
                _state.aliveDuringCallout = static_cast<bool>(_state.weak.get());
                // Preserve a normal COM failure and avoid Clone() touching the
                // old implementation's freed State after recording early death.
                throw winrt::hresult_error(E_FAIL);
            }
            uint8_t GetUInt8() { return _boxed.GetUInt8(); }
            int16_t GetInt16() { return _boxed.GetInt16(); }
            uint16_t GetUInt16() { return _boxed.GetUInt16(); }
            int32_t GetInt32() { return _boxed.GetInt32(); }
            uint32_t GetUInt32() { return _boxed.GetUInt32(); }
            int64_t GetInt64() { return _boxed.GetInt64(); }
            uint64_t GetUInt64() { return _boxed.GetUInt64(); }
            float GetSingle() { return _boxed.GetSingle(); }
            double GetDouble() { return _boxed.GetDouble(); }
            char16_t GetChar16() { return _boxed.GetChar16(); }
            winrt::hstring GetString() { return _boxed.GetString(); }
            winrt::guid GetGuid() { return _boxed.GetGuid(); }
            winrt::Windows::Foundation::DateTime GetDateTime() { return _boxed.GetDateTime(); }
            winrt::Windows::Foundation::TimeSpan GetTimeSpan() { return _boxed.GetTimeSpan(); }
            winrt::Windows::Foundation::Point GetPoint() { return _boxed.GetPoint(); }
            winrt::Windows::Foundation::Size GetSize() { return _boxed.GetSize(); }
            winrt::Windows::Foundation::Rect GetRect() { return _boxed.GetRect(); }
            void GetUInt8Array(winrt::com_array<uint8_t> &value) { _boxed.GetUInt8Array(value); }
            void GetInt16Array(winrt::com_array<int16_t> &value) { _boxed.GetInt16Array(value); }
            void GetUInt16Array(winrt::com_array<uint16_t> &value) { _boxed.GetUInt16Array(value); }
            void GetInt32Array(winrt::com_array<int32_t> &value) { _boxed.GetInt32Array(value); }
            void GetUInt32Array(winrt::com_array<uint32_t> &value) { _boxed.GetUInt32Array(value); }
            void GetInt64Array(winrt::com_array<int64_t> &value) { _boxed.GetInt64Array(value); }
            void GetUInt64Array(winrt::com_array<uint64_t> &value) { _boxed.GetUInt64Array(value); }
            void GetSingleArray(winrt::com_array<float> &value) { _boxed.GetSingleArray(value); }
            void GetDoubleArray(winrt::com_array<double> &value) { _boxed.GetDoubleArray(value); }
            void GetChar16Array(winrt::com_array<char16_t> &value) { _boxed.GetChar16Array(value); }
            void GetBooleanArray(winrt::com_array<bool> &value) { _boxed.GetBooleanArray(value); }
            void GetStringArray(winrt::com_array<winrt::hstring> &value) { _boxed.GetStringArray(value); }
            void GetGuidArray(winrt::com_array<winrt::guid> &value) { _boxed.GetGuidArray(value); }
            void GetDateTimeArray(winrt::com_array<winrt::Windows::Foundation::DateTime> &value) { _boxed.GetDateTimeArray(value); }
            void GetTimeSpanArray(winrt::com_array<winrt::Windows::Foundation::TimeSpan> &value) { _boxed.GetTimeSpanArray(value); }
            void GetPointArray(winrt::com_array<winrt::Windows::Foundation::Point> &value) { _boxed.GetPointArray(value); }
            void GetSizeArray(winrt::com_array<winrt::Windows::Foundation::Size> &value) { _boxed.GetSizeArray(value); }
            void GetRectArray(winrt::com_array<winrt::Windows::Foundation::Rect> &value) { _boxed.GetRectArray(value); }
            void GetInspectableArray(winrt::com_array<winrt::Windows::Foundation::IInspectable> &value) { _boxed.GetInspectableArray(value); }

          private:
            RangeReentrancyState &_state;
            winrt::Windows::Foundation::IPropertyValue _boxed;
        };


        void CheckAgileRangeEndpointPairs(
            winrt::Notepads::Markdown::implementation::MarkdownView &owner,
            std::shared_ptr<winrt::Notepads::Markdown::implementation::TextSnapshot const> const &text)
        {
            Require(text->View().size() >= 8);
            auto victim = winrt::make<RangeProvider>(owner.get_weak(), RawReadingProvider{nullptr}, text,
                                                    text->token, 2, 2);
            auto left = winrt::make<RangeProvider>(owner.get_weak(), RawReadingProvider{nullptr}, text,
                                                  text->token, 2, 2);
            auto right = winrt::make<RangeProvider>(owner.get_weak(), RawReadingProvider{nullptr}, text,
                                                   text->token, 8, 8);
            Require(static_cast<bool>(victim.try_as<winrt::impl::IAgileObject>()));
            winrt::agile_ref<ReadingRange> crossing{victim};
            std::atomic<HRESULT> failure{S_OK};
            std::atomic<uint32_t> reads{};
            winrt::handle readerReady{CreateEventW(nullptr, TRUE, FALSE, nullptr)};
            Require(static_cast<bool>(readerReady));
            std::jthread reader([&](std::stop_token stop)
            {
                try
                {
                    winrt::init_apartment(winrt::apartment_type::multi_threaded);
                    struct Uninitialize
                    {
                        ~Uninitialize() noexcept { winrt::uninit_apartment(); }
                    } uninitialize;
                    auto range = crossing.get();
                    auto identity = range.as<IRangeIdentity>();
                    while (!stop.stop_requested())
                    {
                        uint64_t token{};
                        int32_t start{}, end{};
                        winrt::check_hresult(identity->GetRangeIdentity(&token, &start, &end));
                        if (token != text->token || start < 0 || end < start ||
                            static_cast<size_t>(end) > text->View().size() || !range.GetText(-1).empty() ||
                            !range.Clone().GetText(-1).empty())
                            winrt::throw_hresult(E_UNEXPECTED);
                        ++reads;
                        SetEvent(readerReady.get());
                    }
                }
                catch (...)
                {
                    failure.store(winrt::to_hresult(), std::memory_order_release);
                    SetEvent(readerReady.get());
                }
            });
            // These are ordinary current-view range API calls on the UI thread;
            // the MTA only uses the deliberately agile snapshot-reading methods.
            Require(WaitForSingleObject(readerReady.get(), 10000) == WAIT_OBJECT_0 &&
                    SUCCEEDED(failure.load(std::memory_order_acquire)));
            for (uint32_t iteration = 0; iteration < 10000; ++iteration)
            {
                victim.MoveEndpointByRange(RangeEndpoint::Start, right, RangeEndpoint::Start);
                victim.MoveEndpointByRange(RangeEndpoint::End, left, RangeEndpoint::End);
            }
            reader.request_stop();
            reader.join();
            Require(reads.load() > 0 && SUCCEEDED(failure.load(std::memory_order_acquire)));
            Require(victim.Compare(victim) && victim.CompareEndpoints(RangeEndpoint::Start, victim,
                                                                    RangeEndpoint::End) == 0);
        }

        winrt::Windows::Foundation::IAsyncAction CheckRangePropertyCalloutLifetime(
            winrt::Windows::UI::Core::CoreDispatcher dispatcher, std::shared_ptr<CoreValidation::Context> context,
            std::exception_ptr &recordedFailure)
        {
            try
            {
                using namespace winrt::Windows::UI::Xaml;
                using namespace winrt::Windows::UI::Xaml::Controls;
                using winrt::Windows::Foundation::AsyncStatus;
                // Append only our own child to the isolated diagnostic app's Panel;
                // preserve its Content and remove this child on success and failure.
                auto window = Window::Current();
                Require(window != nullptr);
                auto originalContent = window.Content();
                auto parent = window.Content().try_as<Panel>();
                bool replaceContent = !parent;
                if (replaceContent)
                    parent = Grid{};
                Grid host;
                host.Width(400);
                host.Height(200);
                auto view = winrt::make<winrt::Notepads::Markdown::implementation::MarkdownView>();
                view.Width(400);
                view.Height(200);
                host.Children().Append(view);
                parent.Children().Append(host);
                struct DetachChild
                {
                    Window window{nullptr};
                    UIElement originalContent{nullptr};
                    Panel parent{nullptr};
                    Grid host{nullptr};
                    bool replaceContent{};
                    ~DetachChild() noexcept
                    {
                        try
                        {
                            uint32_t index{};
                            if (parent && parent.Children().IndexOf(host, index))
                                parent.Children().RemoveAt(index);
                            if (host)
                                host.Children().Clear();
                            if (replaceContent)
                                window.Content(originalContent);
                        }
                        catch (...) {}
                    }
                } detach{window, originalContent, parent, host, replaceContent};
                if (replaceContent)
                {
                    window.Content(parent);
                    window.Activate();
                }
                std::exception_ptr failure;
                winrt::Windows::Foundation::IAsyncOperation<winrt::Notepads::Markdown::DocumentUpdateResult> update{nullptr};
                try
                {
                    auto deadline = GetTickCount64() + 10000;
                    while (!view.IsLoaded())
                    {
                        context->Arbitrate();
                        Require(GetTickCount64() < deadline);
                        co_await winrt::resume_after(std::chrono::milliseconds(10));
                        co_await winrt::resume_foreground(dispatcher);
                    }
                    constexpr std::array<uint8_t, 11> source{'r', 'a', 'n', 'g', 'e', ' ', 'v', 'a', 'l', 'u', 'e'};
                    winrt::guid document{0xC1059246, 0xBE13, 0x4E75, {0xBA, 0xDD, 0x52, 0x2B, 0xE9, 0x00, 0x02, 0x21}};
                    update = view.SetDocumentAsync(document, 1, source);
                    while (update.Status() == AsyncStatus::Started)
                    {
                        context->Arbitrate();
                        Require(GetTickCount64() < deadline);
                        co_await winrt::resume_after(std::chrono::milliseconds(10));
                        co_await winrt::resume_foreground(dispatcher);
                    }
                    auto receipt = update.GetResults();
                    Require(receipt.Status() == winrt::Notepads::Markdown::DocumentUpdateStatus::Ready && receipt.Published());
                    auto owner = winrt::get_self<winrt::Notepads::Markdown::implementation::MarkdownView>(view);
                    std::shared_ptr<winrt::Notepads::Markdown::implementation::TextSnapshot const> text;
                    while (!text)
                    {
                        try { text = owner->AutomationSnapshot(); }
                        catch (winrt::hresult_error const &error)
                        {
                            if (error.code() != UIA_E_ELEMENTNOTAVAILABLE)
                                throw;
                        }
                        if (!text)
                        {
                            context->Arbitrate();
                            Require(GetTickCount64() < deadline);
                            co_await winrt::resume_after(std::chrono::milliseconds(10));
                            co_await winrt::resume_foreground(dispatcher);
                        }
                    }
                    CheckAgileRangeEndpointPairs(*owner, text);
                    {
                        RangeReentrancyState moveState;
                        auto range = winrt::make_self<RangeProvider>(owner->get_weak(), RawReadingProvider{nullptr}, text);
                        moveState.weak = range->get_weak();
                        moveState.sole = range.as<ReadingRange>();
                        range = nullptr;
                        auto other = winrt::make<ReleasingRange>(moveState);
                        auto rawMove = static_cast<winrt::impl::abi_t<ReadingRange> *>(winrt::get_abi(moveState.sole));
                        auto status = rawMove->MoveEndpointByRange(static_cast<int32_t>(RangeEndpoint::Start),
                            winrt::get_abi(other), static_cast<int32_t>(RangeEndpoint::End));
                        Require(moveState.called && moveState.aliveDuringCallout && status == E_INVALIDARG &&
                            !moveState.sole && !moveState.weak.get());
                    }
                    RangeReentrancyState state;
                    auto victim = winrt::make_self<RangeProvider>(owner->get_weak(), RawReadingProvider{nullptr}, text);
                    state.weak = victim->get_weak();
                    state.sole = victim.as<ReadingRange>();
                    victim = nullptr;
                    auto foreign = winrt::make<ReleasingProperty>(state);
                    auto raw = static_cast<winrt::impl::abi_t<ReadingRange> *>(winrt::get_abi(state.sole));
                    void *result{};
                    auto status = raw->FindAttribute(
                        static_cast<int32_t>(winrt::Windows::UI::Xaml::AutomationTextAttributesEnum::IsReadOnlyAttribute),
                        winrt::get_abi(foreign), false, &result);
                    ReadingRange found{result, winrt::take_ownership_from_abi};
                    if (!state.called || !state.aliveDuringCallout)
                        throw AssertionFailure{NOTEPADS_MARKDOWN_CORE_INVALID_STATE,
                            L"Actual TextRange FindAttribute died during foreign property callout."};
                    Require(status == E_FAIL && !found && !state.sole && !state.weak.get());

                    // Independent Main expectations cover both ends of the
                    // document: omitted caption LF and terminal colspan TSV TAB.
                    struct DocumentUnitFixture
                    {
                        std::string_view source;
                        std::wstring_view expected;
                        bool leadingAlias;
                    };
                    constexpr std::array documentUnitFixtures{
                        DocumentUnitFixture{InlineCodeTests::HtmlTableFixtureSource,
                                            InlineCodeTests::HtmlTableFixtureMain, true},
                        DocumentUnitFixture{
                            "<table><thead><tr><th>A</th><th>B</th></tr></thead>"
                            "<tbody><tr><td colspan=\"2\">Tail</td></tr></tbody></table>",
                            L"A\tB\nTail\t", false}};
                    uint64_t revision = 1;
                    for (auto const &fixture : documentUnitFixtures)
                    {
                        auto first = reinterpret_cast<uint8_t const *>(fixture.source.data());
                        update = view.SetDocumentWithExtensionsAsync(document, ++revision,
                            winrt::array_view<uint8_t const>{first, first + fixture.source.size()},
                            winrt::Notepads::Markdown::MarkdownExtensions::HtmlBlocks);
                        deadline = GetTickCount64() + 10000;
                        while (update.Status() == AsyncStatus::Started)
                        {
                            context->Arbitrate();
                            Require(GetTickCount64() < deadline);
                            co_await winrt::resume_after(std::chrono::milliseconds(10));
                            co_await winrt::resume_foreground(dispatcher);
                        }
                        auto updateResult = update.GetResults();
                        Require(updateResult.Status() == winrt::Notepads::Markdown::DocumentUpdateStatus::Ready && updateResult.Published());
                        auto snapshot = owner->AutomationSnapshot();
                        Require(snapshot->View() == fixture.expected && view.DisplayedText() == fixture.expected);
                        auto range = winrt::make<RangeProvider>(owner->get_weak(), RawReadingProvider{nullptr}, snapshot);
                        for (auto unit : {RangeUnit::Document, RangeUnit::Page})
                        {
                            range.ExpandToEnclosingUnit(unit);
                            Require(range.GetText(-1) == fixture.expected);
                            uint64_t token{};
                            int32_t start{}, end{};
                            winrt::check_hresult(range.as<IRangeIdentity>()->GetRangeIdentity(&token, &start, &end));
                            Require(token == snapshot->token && start == 0 && end == fixture.expected.size());
                        }
                        auto const &navigation = owner->ReadingNavigation(*snapshot);
                        if (fixture.leadingAlias)
                            Require(fixture.expected.front() == L'\n' && navigation.CanonicalCaret(0) == 1 &&
                                    navigation.Snap(0, false) == 1 && navigation.Snap(0, true) == 1);
                        else
                            Require(fixture.expected.back() == L'\t' &&
                                    navigation.CanonicalCaret(static_cast<uint32_t>(fixture.expected.size())) == fixture.expected.size() - 1);
                    }
                    co_await ReadingOutlineTests::RunFollowDisclosureNavigationAsync(view, dispatcher, context);
                }
                catch (...)
                {
                    failure = std::current_exception();
                }
                // Do not replace the existing content or let assertion failure leave
                // our live control attached in a later native-reading case.
                if (update && update.Status() == AsyncStatus::Started)
                    update.Cancel();
                view.Cancel();
                uint32_t index{};
                if (parent.Children().IndexOf(host, index))
                    parent.Children().RemoveAt(index);
                host.Children().Clear();
                if (replaceContent)
                    window.Content(originalContent);
                view = nullptr;
                if (failure)
                    std::rethrow_exception(failure);
            }
            catch (...)
            {
                // AssertionFailure is deliberately not an ABI exception. Keep it
                // inside the owning test instead of crossing IAsyncAction.
                recordedFailure = std::current_exception();
            }
        }

        void CheckRangeCalloutLifetime(std::shared_ptr<MainTextSnapshot const> const &main)
        {
            auto text = std::allocate_shared<winrt::Notepads::Markdown::implementation::TextSnapshot>(
                ReadingAllocator<winrt::Notepads::Markdown::implementation::TextSnapshot>{main->Memory()});
            text->token = main->TextGeneration();
            text->main = main;
            for (bool endpoints : {false, true})
            {
                RangeReentrancyState state;
                auto victim = winrt::make_self<RangeProvider>(
                    winrt::weak_ref<winrt::Notepads::Markdown::implementation::MarkdownView>{},
                    RawReadingProvider{nullptr}, text);
                state.weak = victim->get_weak();
                state.sole = victim.as<ReadingRange>();
                victim = nullptr;
                auto foreign = winrt::make<ReleasingRange>(state);
                // Invoke the real production provider through its actual ABI,
                // with no projected temporary retaining the victim interface.
                auto raw = static_cast<winrt::impl::abi_t<ReadingRange> *>(winrt::get_abi(state.sole));
                bool equal{true};
                int32_t distance{};
                auto status = endpoints
                    ? raw->CompareEndpoints(static_cast<int32_t>(RangeEndpoint::Start), winrt::get_abi(foreign),
                                            static_cast<int32_t>(RangeEndpoint::End), &distance)
                    : raw->Compare(winrt::get_abi(foreign), &equal);
                if (!state.called || !state.aliveDuringCallout)
                    throw AssertionFailure{NOTEPADS_MARKDOWN_CORE_INVALID_STATE,
                        endpoints ? L"Actual TextRange CompareEndpoints died during foreign identity callout."
                                  : L"Actual TextRange Compare died during foreign identity callout."};
                Require(endpoints ? status == E_INVALIDARG : status == S_OK && !equal);
                Require(!state.sole && !state.weak.get());
            }
        }

        void CheckNativeFailureConversion()
        {
            constexpr std::array cases{
                std::pair<int32_t, HRESULT>{NOTEPADS_MARKDOWN_CORE_INVALID_BOUNDARY, E_INVALIDARG},
                std::pair<int32_t, HRESULT>{NOTEPADS_MARKDOWN_CORE_INVALID_RANGE, E_INVALIDARG},
                std::pair<int32_t, HRESULT>{NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY, E_OUTOFMEMORY},
                std::pair<int32_t, HRESULT>{NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT, HRESULT_FROM_WIN32(ERROR_NOT_ENOUGH_QUOTA)},
                std::pair<int32_t, HRESULT>{NOTEPADS_MARKDOWN_CORE_CANCELLED, HRESULT_FROM_WIN32(ERROR_CANCELLED)},
                std::pair<int32_t, HRESULT>{NOTEPADS_MARKDOWN_CORE_DEADLINE, HRESULT_FROM_WIN32(ERROR_TIMEOUT)},
                std::pair<int32_t, HRESULT>{NOTEPADS_MARKDOWN_CORE_INVALID_STATE, E_FAIL},
                std::pair<int32_t, HRESULT>{E_ACCESSDENIED, E_ACCESSDENIED}};
            for (auto [status, expected] : cases)
            {
                auto mapped = false;
                try
                {
                    throw ReadingFailure{status};
                }
                catch (...)
                {
                    mapped = winrt::to_hresult().value == expected;
                }
                Require(mapped);
                ReadingFailure typed{status};
                Require(typed.status == status && typed.code().value == expected);
            }
            static_assert(!std::is_base_of_v<ReadingFailure, PreviewFailure>);
            auto mappedPreview = false;
            try
            {
                throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_CANCELLED, L"conversion-control"};
            }
            catch (PreviewFailure const &failure)
            {
                mappedPreview = failure.status == NOTEPADS_MARKDOWN_CORE_CANCELLED &&
                                std::wstring_view{failure.stage} == L"conversion-control" &&
                                winrt::to_hresult().value == HRESULT_FROM_WIN32(ERROR_CANCELLED);
            }
            Require(mappedPreview);
        }

        void CheckLineHits(std::shared_ptr<ReadingMemory> const &memory,
                           std::shared_ptr<MainTextSnapshot const> const &snapshot, Case kind)
        {
            auto cancel = std::make_shared<PreviewCancellation>(memory->ModelOwner().Get());
            auto reading = std::allocate_shared<ReadingSourceState>(
                ReadingAllocator<ReadingSourceState>{memory}, memory, cancel,
                snapshot->SourceGeneration(), snapshot->TextGeneration());
            reading->PublishMain(snapshot);
            notepads_markdown_preview_key key{};
            key.header = {NOTEPADS_MARKDOWN_PREVIEW_ABI_VERSION, sizeof(key)};
            key.model = snapshot->Identity();
            key.layout_generation = key.theme_generation = key.measurement_generation = 1;
            key.width = 240;
            if (kind == Case::Inline || kind == Case::WrappedPointerCaret)
                key.width = 120;
            if (kind == Case::SpanningCellNavigation || kind == Case::SpanningCellPageNavigation)
                key.width = 396;
            if (kind == Case::WideTableLineIndex)
                key.width = 960;
            key.indent = 24;
            key.gap = 12;
            auto renderer = NativeDocumentRenderer::Create({}, cancel, key, PreviewTheme{}, reading);
            std::atomic<uint64_t> latest{1};
            auto frame = renderer->CreateFrame(0, 4000, 1, latest);
            Require(frame && frame->navigation && frame->navigation->LineCount() > 1);
            auto const &navigation = *frame->navigation;
            if (kind == Case::ScriptLineIndex || kind == Case::Scripts || kind == Case::Ruby)
            {
                auto checkVisible = [&](PreviewFrame const &candidate)
                {
                    auto const &index = *candidate.navigation;
                    for (size_t i = 0; i < index.LineCount(); ++i)
                    {
                        auto const &line = index.Line(i);
                        for (auto const &rect : NativeDocumentRenderer::LineRectangles(candidate, line))
                        {
                            if (rect.width <= 0 || rect.height <= 0)
                                continue;
                            for (double top : {static_cast<double>(rect.y) + 0.01,
                                               static_cast<double>(rect.y + rect.height) - 0.01})
                            {
                                auto visible = index.VisibleLines(top, top + 0.01);
                                if (std::find(visible.begin(), visible.end(), static_cast<uint32_t>(i)) == visible.end())
                                    throw AssertionFailure{NOTEPADS_MARKDOWN_CORE_INVALID_STATE,
                                        L"Visible line omitted: line=" + std::to_wstring(i) + L", nominal=" +
                                        std::to_wstring(line.y) + L".." + std::to_wstring(line.y + line.height) +
                                        L", rectangle=" + std::to_wstring(rect.y) + L".." +
                                        std::to_wstring(rect.y + rect.height) + L", viewportTop=" + std::to_wstring(top)};
                                Require(index.VisibleLineTop(line) < top + 0.01 && index.VisibleLineBottom(line) > top);
                            }
                        }
                    }
                };
                checkVisible(*frame);
                if (kind != Case::Ruby) // Ruby inline objects deliberately bypass metric caching.
                {
                    auto cache = NativeMeasurementCache::Create();
                    Require(static_cast<bool>(cache));
                    for (uint32_t pass = 0; pass < 2; ++pass)
                    {
                        auto before = cache->Stats().hits;
                        ++key.layout_generation;
                        auto cached = NativeDocumentRenderer::Create({}, cancel, key, PreviewTheme{}, reading, {}, {}, {}, cache);
                        auto cachedFrame = cached->CreateFrame(0, 4000, 1, latest);
                        Require(cachedFrame && cachedFrame->navigation);
                        checkVisible(*cachedFrame);
                        if (pass)
                            Require(cache->Stats().hits > before);
                    }
                }
                return;
            }
            if (kind == Case::WideTableLineIndex)
            {
                Require(frame->Blocks().size() == 1 && frame->Blocks().front().texts.size() == 16 * 64);
                Require(navigation.LineCount() == 16 * 64);
                for (size_t i : {0, 15, 16 * 63, 16 * 64 - 1})
                {
                    auto const &line = navigation.Line(i);
                    Require(line.textIndex == i && line.block == frame->firstBlock);
                    auto direct = NativeDocumentRenderer::LineRectangles(*frame, line);
                    auto full = NativeDocumentRenderer::Rectangles(*frame, line.text.start, line.text.end);
                    Require(direct.size() == 1 && full.size() == 1 && direct.front().start == line.text.start &&
                            direct.front().x == full.front().x && direct.front().y == full.front().y &&
                            direct.front().width == full.front().width && direct.front().height == full.front().height);
                }
                return;
            }
            bool sawWrappedCaret{}, sawSameRow{}, sawTask{};
            double envelope = -std::numeric_limits<double>::infinity();
            for (size_t index = 0; index < navigation.LineCount(); ++index)
            {
                auto const &line = navigation.Line(index);
                envelope = std::max(envelope, line.y);
                Require(std::isfinite(line.followY) && line.followY == envelope);
                Require(navigation.RevealY(line.text.start) == navigation.Line(navigation.LineAt(line.text.start)).y);
                // Independent invariant, rather than the previous global
                // HitTest result: every candidate remains in the chosen row's
                // Main interval, even far outside either physical text edge.
                for (auto x : {-1000.0f, line.x + 15.0f, 1000.0f})
                {
                    auto hit = NativeDocumentRenderer::HitTestLine(*frame, index, x);
                    Require(hit && hit->position >= line.text.start && hit->position <= line.text.end);
                    Require(snapshot->IsBoundary(hit->position));
                }
                if (index)
                {
                    auto const &previous = navigation.Line(index - 1);
                    if (previous.wrappedEnd && previous.text.end == line.text.start &&
                        navigation.LineAt(line.text.start) == index)
                    {
                        Require(navigation.LineAt(line.text.start, true) == index - 1);
                        auto leading = NativeDocumentRenderer::Rectangles(*frame, line.text.start, line.text.start);
                        auto trailing = NativeDocumentRenderer::Rectangles(*frame, line.text.start, line.text.start, true);
                        Require(leading.size() == 1 && trailing.size() == 1 && trailing.front().y < leading.front().y);
                        sawWrappedCaret = true;
                    }
                    sawSameRow |= std::abs(previous.y - line.y) < 0.01 && previous.text.end < line.text.start;
                }
                if (line.text.end == line.text.start + 1 && snapshot->Text()[line.text.start] == L'\xfffc')
                {
                    auto left = NativeDocumentRenderer::HitTestLine(*frame, index, -1000);
                    auto right = NativeDocumentRenderer::HitTestLine(*frame, index, 1000);
                    Require(left && right && left->position == line.text.start && right->position == line.text.end);
                    sawTask = true;
                }
            }
            if (kind == Case::Inline)
            {
                Require(sawWrappedCaret); // real narrow DWrite soft-wrap, mixed RTL/emoji Main
                Require(!reading->wordSegments.spans.empty());
                auto const *segments = reading->wordSegments.spans.data();
                auto wideKey = key;
                wideKey.width = 360;
                ++wideKey.layout_generation;
                auto wide = NativeDocumentRenderer::Create({}, cancel, wideKey, PreviewTheme{}, reading);
                auto wideFrame = wide->CreateFrame(0, 4000, 1, latest);
                Require(wideFrame && wideFrame->navigation && wideFrame->navigation->LineCount() < navigation.LineCount());
                Require(reading->wordSegments.spans.data() == segments);
                using Unit = winrt::Windows::UI::Xaml::Automation::Text::TextUnit;
                for (uint32_t position = 0; position <= snapshot->Length(); ++position)
                {
                    auto a = navigation.Unit(position, Unit::Word);
                    auto b = wideFrame->navigation->Unit(position, Unit::Word);
                    Require(a.start == b.start && a.end == b.end);
                }
            }
            if (kind == Case::SpanningCellNavigation)
            {
                Require(navigation.LineCount() == 3);
                auto const &left = navigation.Line(1);
                auto const &right = navigation.Line(2);
                auto gap = right.x - (left.x + left.width);
                Require(gap > 1 && left.y == right.y);
                auto preferred = right.x - gap * 0.25f;
                Require(navigation.VisualLine(0, true, 0, preferred) == 2);
                Require(navigation.VisualLine(0, true, frame->totalHeight + 100, preferred) == 2);
                Require(navigation.VisualLine(2, false, 0, preferred) == 0);
            }
            if (kind == Case::SpanningCellPageNavigation)
            {
                auto leftIndex = navigation.LineAt(static_cast<uint32_t>(snapshot->Text().find(L"Left")));
                auto rightIndex = navigation.LineAt(static_cast<uint32_t>(snapshot->Text().find(L"Right")));
                auto const &header = navigation.Line(0);
                auto const &left = navigation.Line(leftIndex);
                auto const &right = navigation.Line(rightIndex);
                auto caret = NativeDocumentRenderer::Rectangles(*frame, 1, 1);
                Require(caret.size() == 1 && left.y == right.y && left.x < caret.front().x &&
                        caret.front().x < left.x + left.width);
                constexpr double page = 200;
                Require(left.y < header.y + page && header.y + page < envelope);
                Require(navigation.VisualLine(0, true, page, caret.front().x) == leftIndex);
                Require(navigation.VisualLine(0, true, page, right.x + 1) == rightIndex);
            }
            if (kind == Case::WrappedPointerCaret)
            {
                auto const &row = navigation.Line(0);
                auto boundary = row.text.end;
                Require(row.wrappedEnd && boundary > 0 && navigation.Line(1).text.start == boundary);
                auto const &block = frame->Blocks().front();
                auto const &text = block.texts.front();
                auto insideGlyph = [&](uint32_t offset, float fraction)
                {
                    FLOAT x{}, y{};
                    DWRITE_HIT_TEST_METRICS glyph{};
                    winrt::check_hresult(text.layout->HitTestTextPosition(offset, FALSE, &x, &y, &glyph));
                    return NativeDocumentRenderer::HitTest(*frame,
                        static_cast<float>(block.block.x) + text.x + glyph.left + glyph.width * fraction,
                        static_cast<float>(block.block.y - frame->viewportY) + text.y + glyph.top + glyph.height * 0.5f);
                };
                auto trailing = insideGlyph(boundary - 1, 0.75f);
                auto leading = insideGlyph(boundary, 0.25f);
                Require(trailing && leading && trailing->inside && leading->inside &&
                        trailing->position == boundary && leading->position == boundary &&
                        trailing->trailingCaret && !leading->trailingCaret);
                auto end = NativeDocumentRenderer::HitTestLine(*frame, 0, 1000);
                Require(end && end->position == boundary && end->trailingCaret);
            }
            if (kind == Case::MathFallbackLineBreak)
            {
                Require(!frame->Blocks().empty() && !frame->Blocks().front().texts.empty());
                auto const &math = frame->Blocks().front().texts.front().math;
                Require(math.size() == 1 && math.front().label);
                DWRITE_LINE_METRICS line{};
                UINT32 count{};
                winrt::check_hresult(math.front().label->GetLineMetrics(&line, 1, &count));
                Require(count == 1);
            }
            if (kind == Case::Structured)
                Require(sawSameRow && sawTask); // real table cells and empty task item
            if (kind == Case::TablePageNavigation)
            {
                // The authored table is the entire document. A page that
                // overshoots either end must still inspect all three columns.
                Require(navigation.LineCount() == 9);
                for (size_t column = 0; column != 3; ++column)
                {
                    auto const &middle = navigation.Line(3 + column);
                    Require(middle.tableCell && middle.text.start == 9 + 3 * column);
                    auto x = middle.x + 1;
                    Require(navigation.VisualLine(3 + column, false, frame->totalHeight + 100, x) == column);
                    Require(navigation.VisualLine(3 + column, true, frame->totalHeight + 100, x) == 6 + column);
                    Require(navigation.VisualLine(3 + column, false, 0, x) == column);
                    Require(navigation.VisualLine(3 + column, true, 0, x) == 6 + column);
                    Require(navigation.VisualLine(column, false, frame->totalHeight + 100, x) == column);
                    Require(navigation.VisualLine(6 + column, true, frame->totalHeight + 100, x) == 6 + column);
                }
            }
            if (kind == Case::WrappedTablePageNavigation)
            {
                Require(navigation.LineCount() > 9);
                auto finalLeft = navigation.LineAt(18);
                auto finalRight = navigation.LineAt(snapshot->Length() - 2);
                for (auto [origin, target] : {std::pair<size_t, size_t>{navigation.LineAt(9), finalLeft},
                                             {navigation.LineAt(15), finalRight}})
                {
                    auto const &line = navigation.Line(origin);
                    auto const &last = navigation.Line(target);
                    Require(line.tableCell && last.tableCell && line.block == last.block && last.y > line.y + 0.25);
                    Require(last.y + 0.25 < envelope);
                    auto x = line.x + 1;
                    auto withinWrappedCell = (last.y + envelope) / 2 - line.y;
                    Require(navigation.VisualLine(origin, true, frame->totalHeight + 100, x) == target);
                    Require(navigation.VisualLine(origin, true, withinWrappedCell, x) == target);
                    Require(navigation.VisualLine(origin, true, 0, x) == target);
                    Require(navigation.VisualLine(target, true, 0, x) == target);
                    Require(navigation.VisualLine(target, true, frame->totalHeight + 100, x) == target);
                    Require(navigation.VisualLine(target, false, 0, x) == origin);
                }
            }
            double previous = navigation.FollowY(0);
            for (uint32_t position = 0; position <= snapshot->Length(); ++position)
            {
                auto next = navigation.FollowY(position);
                Require(next >= previous);
                previous = next;
            }
            Require(navigation.FollowY(snapshot->Length()) == envelope);

            // Same navigation/source/layout, but the selected row has no
            // materialized visual in this Frame. Never substitute another row.
            auto missing = renderer->CreateFrame(frame->totalHeight + 100, 10, 1, latest);
            Require(missing && missing->Blocks().empty());
            Require(!NativeDocumentRenderer::HitTestLine(*missing, 0, 1000));
        }

        std::vector<uint32_t> ReadingPixels(PreviewFrame const &frame, std::span<POINT const> points,
                                           float originY = 16, std::optional<D2D1_RECT_F> paintClip = {})
        {
            winrt::com_ptr<ID2D1Factory1> factory;
            winrt::check_hresult(D2D1CreateFactory(D2D1_FACTORY_TYPE_MULTI_THREADED, factory.put()));
            winrt::com_ptr<ID3D11Device> d3d;
            winrt::com_ptr<ID3D11DeviceContext> d3dContext;
            D3D_FEATURE_LEVEL level{};
            winrt::check_hresult(D3D11CreateDevice(nullptr, D3D_DRIVER_TYPE_WARP, nullptr,
                                                   D3D11_CREATE_DEVICE_BGRA_SUPPORT, nullptr, 0, D3D11_SDK_VERSION,
                                                   d3d.put(), &level, d3dContext.put()));
            winrt::com_ptr<ID2D1Device> device;
            winrt::check_hresult(factory->CreateDevice(d3d.as<IDXGIDevice>().get(), device.put()));
            winrt::com_ptr<ID2D1DeviceContext> context;
            winrt::check_hresult(device->CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE, context.put()));
            constexpr UINT32 width = 512, height = 128;
            auto properties = D2D1::BitmapProperties1(D2D1_BITMAP_OPTIONS_TARGET,
                D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_PREMULTIPLIED));
            winrt::com_ptr<ID2D1Bitmap1> target;
            winrt::check_hresult(context->CreateBitmap(D2D1::SizeU(width, height), nullptr, 0, properties, target.put()));
            context->SetTarget(target.get());
            auto background = frame.theme.background;
            context->BeginDraw();
            context->Clear(D2D1::ColorF(background.R / 255.0f, background.G / 255.0f,
                                       background.B / 255.0f, 1.0f));
            try
            {
                NativeDocumentRenderer::Draw(frame, context.get(), 16, originY, 0, 0,
                                             {}, false, nullptr, false, {}, paintClip);
            }
            catch (...)
            {
                (void)context->EndDraw();
                throw;
            }
            winrt::check_hresult(context->EndDraw());
            context->SetTarget(nullptr);
            properties.bitmapOptions = D2D1_BITMAP_OPTIONS_CPU_READ | D2D1_BITMAP_OPTIONS_CANNOT_DRAW;
            winrt::com_ptr<ID2D1Bitmap1> readback;
            winrt::check_hresult(context->CreateBitmap(D2D1::SizeU(width, height), nullptr, 0, properties, readback.put()));
            winrt::check_hresult(readback->CopyFromBitmap(nullptr, target.get(), nullptr));
            D2D1_MAPPED_RECT mapped{};
            winrt::check_hresult(readback->Map(D2D1_MAP_OPTIONS_READ, &mapped));
            std::vector<uint32_t> pixels(points.size());
            for (size_t index = 0; index < points.size(); ++index)
            {
                auto point = points[index];
                // All points are authored inside this bounded target.
                if (point.x < 0 || point.y < 0 || point.x >= width || point.y >= height)
                {
                    (void)readback->Unmap();
                    throw AssertionFailure{NOTEPADS_MARKDOWN_CORE_INVALID_RANGE, L"Pixel probe lies outside its WARP target."};
                }
                std::memcpy(&pixels[index], mapped.bits + static_cast<size_t>(point.y) * mapped.pitch + point.x * 4, 4);
            }
            winrt::check_hresult(readback->Unmap());
            return pixels;
        }

        std::array<uint32_t, 3> QuotePixels(PreviewFrame const &frame, std::array<POINT, 3> const &points)
        {
            auto values = ReadingPixels(frame, points);
            return {values[0], values[1], values[2]};
        }

        void CheckQuoteGeometry(std::shared_ptr<ReadingMemory> const &memory,
                                std::shared_ptr<MainTextSnapshot const> const &snapshot, Fixture const &fixture)
        {
            auto cancel = std::make_shared<PreviewCancellation>(memory->ModelOwner().Get());
            auto reading = std::allocate_shared<ReadingSourceState>(ReadingAllocator<ReadingSourceState>{memory},
                memory, cancel, snapshot->SourceGeneration(), snapshot->TextGeneration());
            reading->PublishMain(snapshot);
            notepads_markdown_preview_key key{};
            key.header = {NOTEPADS_MARKDOWN_PREVIEW_ABI_VERSION, sizeof(key)};
            key.model = snapshot->Identity();
            key.layout_generation = key.theme_generation = key.measurement_generation = 1;
            key.width = 400;
            key.indent = 24;
            key.gap = 12;
            auto renderer = NativeDocumentRenderer::Create({}, cancel, key, PreviewTheme{}, reading);
            std::atomic<uint64_t> latest{1};
            auto full = renderer->CreateFrame(0, 4000, 1, latest);
            Require(full && full->quotes && full->quotes->size() == 7 &&
                    full->quotes->get_allocator().layout != nullptr);
            auto blockAt = [&](std::wstring_view token) -> PreviewBlockVisual const &
            {
                auto position = snapshot->Text().find(token);
                Require(position != std::wstring_view::npos &&
                        snapshot->Text().find(token, position + 1) == std::wstring_view::npos);
                auto found = std::find_if(full->Blocks().begin(), full->Blocks().end(), [&](auto const &visual)
                {
                    return visual.block.logical_start <= position && position < visual.block.logical_end;
                });
                Require(found != full->Blocks().end());
                return *found;
            };
            auto const &parent = blockAt(L"parent");
            auto const &continuation = blockAt(L"continuation");
            auto const &code = blockAt(L"code");
            auto const &listQuote = blockAt(L"list quote");
            auto const &taskQuote = blockAt(L"task quote");
            auto const &quoteFirst = blockAt(L"quote first");
            auto const &outer = blockAt(L"outer");
            auto const &nested = blockAt(L"nested");
            auto const &tail = blockAt(L"tail");
            auto const &finish = blockAt(L"finish");
            auto const &sibling = blockAt(L"sibling");
            Require(parent.block.x == 24 && continuation.block.x == parent.block.x &&
                    code.block.kind == 3 && code.block.x == parent.block.x);
            Require(listQuote.block.x == 39 && taskQuote.block.x == 39 && quoteFirst.block.x == 39 &&
                    listQuote.markerX == 0 && taskQuote.markerX == 0 && quoteFirst.markerX == 0 &&
                    !quoteFirst.decorations.empty());
            Require(outer.block.x == 15 && nested.block.x == 54 && nested.markerX == 15 && tail.block.x == 15);
            auto const &quotes = *full->quotes;
            constexpr std::array<double, 7> lefts{24, 24, 24, 0, 39, 0, 0};
            constexpr std::array<uint32_t, 7> depths{1, 1, 1, 1, 2, 1, 1};
            for (size_t index = 0; index < quotes.size(); ++index)
                Require(quotes[index].x == lefts[index] && quotes[index].depth == depths[index] &&
                        quotes[index].border_width == 5 && quotes[index].height > 0 &&
                        (index != 4 || quotes[index].width == 345));
            Require(quotes[0].x == parent.block.x && quotes[0].y + 3 == listQuote.block.y &&
                    quotes[0].height == listQuote.block.height + 6);
            Require(quotes[3].y + 3 == outer.block.y &&
                    quotes[3].y + quotes[3].height == tail.block.y + tail.block.height + 3 &&
                    quotes[3].x == 0 && quotes[4].x == 39 &&
                    quotes[4].y + 3 == nested.block.y && quotes[4].height == nested.block.height + 6);
            Require(quotes[5].y + quotes[5].height == finish.block.y + finish.block.height + 3 &&
                    quotes[6].y + 3 == sibling.block.y && quotes[6].node != quotes[5].node &&
                    quotes[5].y + quotes[5].height < quotes[6].y);

            auto task = std::find_if(taskQuote.decorations.begin(), taskQuote.decorations.end(),
                                    [](auto const &decoration) { return decoration.kind == 3; });
            Require(task != taskQuote.decorations.end());
            auto taskStart = static_cast<uint32_t>(task->logical_start);
            auto taskEnd = static_cast<uint32_t>(task->logical_end);
            auto rectangles = NativeDocumentRenderer::Rectangles(*full, taskStart, taskEnd);
            Require(rectangles.size() == 1 && rectangles[0].x == 6 && rectangles[0].width == 14);
            auto textLine = full->navigation->Line(full->navigation->LineAt(taskStart));
            auto const markerCenter = rectangles[0].y + rectangles[0].height * 0.5f;
            auto const lineCenter = static_cast<float>(textLine.y - full->viewportY + textLine.height * 0.5);
            Require(std::abs(markerCenter - lineCenter) <= 1.0f);
            auto caret = NativeDocumentRenderer::Rectangles(*full, taskStart, taskStart);
            Require(caret.size() == 1 && caret[0].x == 6);
            auto first = NativeDocumentRenderer::HitTest(*full, 7, rectangles[0].y + 7);
            auto last = NativeDocumentRenderer::HitTest(*full, 19, rectangles[0].y + 7);
            Require(first && last && first->position == taskStart && last->position == taskEnd &&
                    first->cluster.start == taskStart && last->cluster.end == taskEnd);
            auto emptyTask = std::find_if(full->Blocks().begin(), full->Blocks().end(), [&](auto const &visual)
            {
                return visual.block.kind == 7 && visual.block.quote_depth == 1;
            });
            Require(emptyTask != full->Blocks().end() && emptyTask->markerX == 15 && emptyTask->block.x == 39 &&
                    emptyTask->decorations.size() == 1);
            auto emptyStart = static_cast<uint32_t>(emptyTask->decorations[0].logical_start);
            auto lineIndex = full->navigation->LineAt(emptyStart);
            Require(full->navigation->Line(lineIndex).x == 21);
            auto lineHit = NativeDocumentRenderer::HitTestLine(*full, lineIndex, 22);
            Require(lineHit && lineHit->position == emptyStart && lineHit->x == 21);
            auto emptyCaret = NativeDocumentRenderer::Rectangles(*full, emptyStart + 1, emptyStart + 1);
            Require(emptyCaret.size() == 1 && emptyCaret[0].x == 35);

            auto mainAt = snapshot->Text().find(L"中😀");
            auto sourceAt = fixture.source.find("中😀");
            Require(mainAt != std::wstring_view::npos && sourceAt != std::string::npos);
            auto mapped = MapReadingSpan(*snapshot, cancel->Get(), true, sourceAt, sourceAt + 7, false);
            Require(mapped.start.sourceByteOffset == sourceAt && mapped.end.sourceByteOffset == sourceAt + 7 &&
                    mapped.start.previewUtf16Offset == mainAt && mapped.end.previewUtf16Offset == mainAt + 3);
            auto reverse = MapReadingSpan(*snapshot, cancel->Get(), false, mainAt, mainAt + 3, false);
            Require(reverse.start.sourceByteOffset == sourceAt && reverse.end.sourceByteOffset == sourceAt + 7 &&
                    snapshot->CopyText(static_cast<uint32_t>(mainAt), 3) == L"中😀");
            auto unicode = NativeDocumentRenderer::Rectangles(*full, static_cast<uint32_t>(mainAt),
                                                               static_cast<uint32_t>(mainAt + 3));
            Require(!unicode.empty());
            for (auto const &rectangle : unicode)
            {
                auto hit = NativeDocumentRenderer::HitTest(*full, rectangle.x + rectangle.width * 0.5f,
                                                           rectangle.y + rectangle.height * 0.5f);
                Require(hit && hit->position >= mainAt && hit->position <= mainAt + 3 &&
                        hit->cluster.start >= mainAt && hit->cluster.end <= mainAt + 3);
            }

            auto gapStart = outer.block.y + outer.block.height;
            auto next = std::find_if(full->Blocks().begin(), full->Blocks().end(),
                                     [&](auto const &visual) { return visual.block.y > outer.block.y; });
            Require(next != full->Blocks().end() && next->block.y - gapStart > 2);
            auto gapY = (gapStart + next->block.y) * 0.5;
            auto gap = renderer->CreateFrame(gapY + full->theme.padding, 1, 1, latest);
            Require(gap && gap->Blocks().empty() && gap->quotes == full->quotes &&
                    quotes[3].y < gapY && quotes[3].y + quotes[3].height > gapY + 1);
            auto repositioned = full->Reposition(gapY + full->theme.padding, 1);
            Require(repositioned && repositioned->quotes == full->quotes &&
                    repositioned->measuredBlocks == full->measuredBlocks);
            auto middle = renderer->CreateFrame(nested.block.y + full->theme.padding, 64, 1, latest);
            Require(middle && middle->quotes == full->quotes && (*middle->quotes)[3].y < nested.block.y);
            auto gapPixels = QuotePixels(*gap, {POINT{26, 0}, POINT{18, 0}, POINT{450, 0}});
            Require(gapPixels[0] == 0xFFF2F2F2 && gapPixels[1] == 0xFFCBD0D8 && gapPixels[2] == 0xFFFFFFFF);
            auto nestedPixels = QuotePixels(*middle, {POINT{26, 10}, POINT{65, 10}, POINT{57, 10}});
            Require(nestedPixels[0] == gapPixels[0] && nestedPixels[1] == nestedPixels[0] &&
                    nestedPixels[2] == gapPixels[1]);
            auto fractional = renderer->CreateFrame(outer.block.y + full->theme.padding + 8, 32.25, 1, latest);
            Require(fractional && quotes[3].height > 41);
            constexpr std::array<POINT, 6> bottomPoints{{{26, 31}, {18, 31}, {26, 32},
                                                        {18, 32}, {26, 33}, {18, 33}}};
            // phaseY=.75 shifts the visible 32.25-DIP viewport through row 32.
            // Both the tint and accent must cover that final complete surface row.
            auto bottomPixels = ReadingPixels(*fractional, bottomPoints, 16.75f, D2D1::RectF(0, 0, 512, 33));
            Require(bottomPixels[0] == gapPixels[0] && bottomPixels[1] == gapPixels[1] &&
                    bottomPixels[2] == bottomPixels[0] && bottomPixels[3] == bottomPixels[1] &&
                    bottomPixels[4] == 0xFFFFFFFF && bottomPixels[5] == 0xFFFFFFFF);
            middle->theme.highContrast = true;
            middle->theme.background = {255, 0, 0, 0};
            middle->theme.foreground = {255, 255, 255, 255};
            auto highContrast = QuotePixels(*middle, {POINT{26, 10}, POINT{65, 10}, POINT{57, 10}});
            Require(highContrast[0] == 0xFF000000 && highContrast[1] == highContrast[0] &&
                    highContrast[2] == 0xFFFFFFFF);
        }

        bool RepeatedTextEquals(ReadingString const &text, size_t length, wchar_t character)
        {
            return text.size() == length &&
                   std::all_of(text.begin(), text.end(), [character](wchar_t value) { return value == character; });
        }

        void CheckAllocatorMoves(std::shared_ptr<ReadingMemory> const &memory)
        {
            ReadingAllocator<wchar_t> source{memory};
            auto moved = std::move(source);
            Require(source.memory == memory && moved.memory == memory);
            ReadingAllocator<wchar_t> assigned;
            assigned = std::move(moved);
            Require(moved.memory == memory && assigned.memory == memory);
            auto allocation = source.allocate(128);
            source.deallocate(allocation, 128);
            allocation = moved.allocate(128);
            moved.deallocate(allocation, 128);

            ReadingString original{ReadingAllocator<wchar_t>{memory}};
            original.assign(80, L'a');
            auto temporary = std::move(original);
            original.assign(240, L'b');
            Require(RepeatedTextEquals(original, 240, L'b') && RepeatedTextEquals(temporary, 80, L'a'));
            original = std::move(temporary);
            temporary.assign(320, L'c');
            Require(RepeatedTextEquals(original, 80, L'a') && RepeatedTextEquals(temporary, 320, L'c'));
            original.swap(temporary);
            Require(RepeatedTextEquals(original, 320, L'c') && RepeatedTextEquals(temporary, 80, L'a'));

            ReadingVector<ReadingLinkDescriptor> links{ReadingAllocator<ReadingLinkDescriptor>{memory}};
            ReadingVector<ReadingObjectDescriptor> objects{ReadingAllocator<ReadingObjectDescriptor>{memory}};
            for (uint32_t index = 24; index != 0; --index)
            {
                links.emplace_back(memory);
                auto &link = links.back();
                link.node = index;
                link.text = {index, index + 1};
                link.destination.assign(40 + index * 3, static_cast<wchar_t>(L'a' + index % 26));
                link.title.assign(90 + index, static_cast<wchar_t>(L'A' + index % 26));
                objects.emplace_back(memory);
                auto &object = objects.back();
                object.object = index;
                object.text = {index, index + 1};
                object.kind = ReadingObjectKind::Image;
                object.alt.assign(120 + index * 2, static_cast<wchar_t>(L'a' + index % 26));
            }
            auto linkTemporary = std::move(links.front());
            links.front() = std::move(linkTemporary);
            auto objectTemporary = std::move(objects.front());
            objects.front() = std::move(objectTemporary);
            std::sort(links.begin(), links.end(),
                      [](auto const &left, auto const &right) { return left.text.start < right.text.start; });
            std::sort(objects.begin(), objects.end(),
                      [](auto const &left, auto const &right) { return left.text.start < right.text.start; });
            for (uint32_t index = 1; index <= 24; ++index)
            {
                auto const &link = links[index - 1];
                auto const &object = objects[index - 1];
                Require(link.node == index && link.text.start == index && object.object == index &&
                        object.text.start == index);
                Require(RepeatedTextEquals(link.destination, 40 + index * 3, static_cast<wchar_t>(L'a' + index % 26)) &&
                        RepeatedTextEquals(link.title, 90 + index, static_cast<wchar_t>(L'A' + index % 26)) &&
                        RepeatedTextEquals(object.alt, 120 + index * 2, static_cast<wchar_t>(L'a' + index % 26)));
            }
            auto movedLinks = std::move(links);
            links.emplace_back(memory);
            links.front().destination.assign(400, L'z');
            Require(RepeatedTextEquals(links.front().destination, 400, L'z'));
            links.swap(movedLinks);
            Require(links.size() == 24 && movedLinks.size() == 1 && movedLinks.front().destination.size() == 400);
            Require(links.get_allocator().memory == memory && movedLinks.get_allocator().memory == memory);
        }
        struct WorkerStackObservation
        {
            ULONG_PTR span{}, markerAboveLower{};
        };

        __declspec(noinline) WorkerStackObservation ObserveWorkerStack()
        {
            // This ordinary noinline helper owns a real stack local. A local in
            // the coroutine itself could instead live in its heap frame.
            volatile unsigned char marker{};
            ULONG_PTR lower{}, upper{};
            GetCurrentThreadStackLimits(&lower, &upper);
            auto address = reinterpret_cast<ULONG_PTR>(&marker);
            Require(lower < upper && address >= lower && address < upper);
            // Raw platform observation only: no audited synchronous Math bound,
            // guard margin or production funding/profile is established here.
            return {upper - lower, address - lower};
        }

        bool ExportEquals(MainTextSnapshot const &snapshot, std::wstring_view expected)
        {
            auto output = snapshot.CopyText(0, snapshot.Length());
            return std::wstring_view{output.data(), output.size()} == expected;
        }

        void Check(int32_t status)
        {
            Require(status == NOTEPADS_MARKDOWN_CORE_OK, status);
        }

        template <typename F> void ExpectStatus(int32_t expected, F &&body)
        {
            try
            {
                body();
            }
            catch (ReadingFailure const &failure)
            {
                Require(failure.status == expected, failure.status);
                return;
            }
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
        }

        notepads_markdown_preview_key ListMarkerKey(MainTextSnapshot const &snapshot, double width)
        {
            notepads_markdown_preview_key key{};
            key.header = {NOTEPADS_MARKDOWN_PREVIEW_ABI_VERSION, sizeof(key)};
            key.model = snapshot.Identity();
            key.layout_generation = key.theme_generation = key.measurement_generation = 1;
            key.width = width;
            key.indent = 28;
            key.gap = 12;
            return key;
        }

        void CheckEditImageWidths(std::shared_ptr<ReadingMemory> const &memory,
                                  std::shared_ptr<MainTextSnapshot const> const &snapshot)
        {
            for (double available : {603.0, 71.0, 9.0})
            {
                auto cancel = std::make_shared<PreviewCancellation>(memory->ModelOwner().Get());
                auto reading = std::allocate_shared<ReadingSourceState>(ReadingAllocator<ReadingSourceState>{memory},
                    memory, cancel, snapshot->SourceGeneration(), snapshot->TextGeneration());
                reading->PublishMain(snapshot);
                auto key = ListMarkerKey(*snapshot, available);
                PreviewEditResources prepared;
                auto draft = NativeDocumentRenderer::Create({}, cancel, key, PreviewTheme{}, reading,
                    {}, {}, {}, {}, nullptr, {}, {}, {}, &prepared);
                Require(prepared.main == snapshot && prepared.descriptors == reading->Descriptors() &&
                        prepared.imageWidths && prepared.imageWidths->size() == 8 && !draft->Extent().blocks);
                draft.reset();
                auto renderer = NativeDocumentRenderer::Create({}, cancel, key, PreviewTheme{}, reading);
                std::atomic<uint64_t> latest{1};
                auto frame = renderer->CreateFrame(0, 1000000, 1, latest);
                Require(frame && frame->descriptors == prepared.descriptors && !reading->fault);
                std::array<bool, 8> seen{};
                size_t measured{};
                for (auto const &block : frame->Blocks())
                    for (auto const &text : block.texts)
                        for (auto const &image : text.images)
                        {
                            auto const &widths = *prepared.imageWidths;
                            auto proof = std::find_if(widths.begin(), widths.end(),
                                [&](auto const &width) { return width.object == image.object; });
                            Require(proof != widths.end() && proof->width == static_cast<double>(text.width));
                            const auto index = static_cast<size_t>(proof - widths.begin());
                            Require(!seen[index]);
                            seen[index] = true;
                            ++measured;
                            auto const &objects = prepared.descriptors->objects;
                            auto object = std::find_if(objects.begin(), objects.end(),
                                [&](auto const &object) { return object.kind == ReadingObjectKind::Image && object.object == image.object; });
                            Require(object != objects.end());
                            if (available == 603 || available == 9)
                            {
                                double expected = available;
                                if (object->destination == L"md0" || object->destination == L"md1" || object->destination == L"md2")
                                    expected = available == 603 ? 189 : 1;
                                else if (object->destination == L"span")
                                    expected = available == 603 ? 390 : 4.5;
                                else if (object->destination == L"row" || object->destination == L"cell")
                                    expected = available == 603 ? 189 : 1.5;
                                Require(proof->width == expected);
                            }
                        }
                Require(measured == seen.size() && std::all_of(seen.begin(), seen.end(), [](bool value) { return value; }));
            }
        }

        void CheckRubyGeometry(std::shared_ptr<ReadingMemory> const &memory,
                               std::shared_ptr<MainTextSnapshot const> const &snapshot, Fixture const &fixture)
        {
            auto cancel = std::make_shared<PreviewCancellation>(memory->ModelOwner().Get());
            auto reading = std::allocate_shared<ReadingSourceState>(ReadingAllocator<ReadingSourceState>{memory},
                memory, cancel, snapshot->SourceGeneration(), snapshot->TextGeneration());
            reading->PublishMain(snapshot);
            std::atomic<uint64_t> latest{1};
            auto make = [&](double width)
            {
                PreviewTheme theme;
                theme.fontSize = 20;
                theme.lineHeight = 30;
                theme.background = {255, 255, 255, 255};
                theme.foreground = {255, 0, 0, 0};
                theme.linkForeground = {255, 15, 35, 165};
                auto renderer = NativeDocumentRenderer::Create({}, cancel, ListMarkerKey(*snapshot, width), theme, reading);
                auto frame = renderer->CreateFrame(0, 300, 1, latest);
                Require(frame && frame->navigation && !reading->fault);
                return frame;
            };
            const auto baseStart = static_cast<uint32_t>(fixture.expected.find(L"漢字read"));
            const auto annotationStart = baseStart + 2;
            auto wide = make(380);
            auto narrow = make(115);
            Require(wide->navigation->LineCount() < narrow->navigation->LineCount());
            for (auto const &frame : {wide, narrow})
            {
                Require(frame->descriptors && frame->descriptors->objects.size() == 1 &&
                        frame->descriptors->objects.front().kind == ReadingObjectKind::Image &&
                        frame->descriptors->objects.front().imageHasFixedSize &&
                        frame->descriptors->objects.front().imageReadinessAffectsText);
                bool decoratedRuby{};
                for (auto const &block : frame->Blocks())
                    for (auto const &text : block.texts)
                    {
                        if (!text.scripts || baseStart < text.logicalStart || baseStart >= text.logicalEnd)
                            continue;
                        auto local = baseStart - static_cast<uint32_t>(text.logicalStart);
                        BOOL underline{}, strike{};
                        winrt::check_hresult(text.layout->GetUnderline(local, &underline));
                        winrt::check_hresult(text.layout->GetStrikethrough(local, &strike));
                        Require(!underline && !strike);
                        size_t children{};
                        for (auto const &part : text.scripts->RubyParts())
                            if (local <= part.start && part.end <= local + 6)
                            {
                                winrt::check_hresult(part.layout->GetUnderline(0, &underline));
                                winrt::check_hresult(part.layout->GetStrikethrough(0, &strike));
                                Require(underline && strike);
                                ++children;
                            }
                        Require(children == 2);
                        decoratedRuby = true;
                    }
                Require(decoratedRuby);
                auto base = NativeDocumentRenderer::Rectangles(*frame, baseStart, annotationStart);
                auto rt = NativeDocumentRenderer::Rectangles(*frame, annotationStart, annotationStart + 4);
                auto inner = NativeDocumentRenderer::Rectangles(*frame, annotationStart + 1, annotationStart + 2);
                auto caret = NativeDocumentRenderer::Rectangles(*frame, annotationStart + 1, annotationStart + 1);
                Require(base.size() == 1 && rt.size() == 1 && inner.size() == 1 && caret.size() == 1);
                Require(base[0].y > rt[0].y + rt[0].height && base[0].height > rt[0].height &&
                        inner[0].width > 0 && inner[0].width < rt[0].width &&
                        caret[0].y == inner[0].y && std::abs(caret[0].x - inner[0].x) < 0.01);
                int32_t moved{};
                Require(frame->navigation->Move(annotationStart, RangeUnit::Character, 1, moved) == annotationStart + 1 &&
                        moved == 1);
                auto hit = NativeDocumentRenderer::HitTest(*frame, inner[0].x + inner[0].width * 0.25f,
                                                           inner[0].y + inner[0].height * 0.5f);
                Require(hit && hit->inside && hit->position == annotationStart + 1 &&
                        hit->cluster.start == annotationStart + 1 && hit->cluster.end == annotationStart + 2);
                if (frame == wide)
                {
                    for (auto const &box : {base[0], rt[0]})
                    {
                        std::vector<POINT> points;
                        for (auto y = static_cast<LONG>(std::ceil(box.y)); y < std::floor(box.y + box.height); ++y)
                            for (auto x = static_cast<LONG>(std::ceil(box.x)); x < std::floor(box.x + box.width); ++x)
                                points.push_back({x + 16, y + 16});
                        auto pixels = ReadingPixels(*frame, points);
                        Require(std::count_if(pixels.begin(), pixels.end(), [](auto color) { return color != 0xFFFFFFFF; }) >= 4);
                    }
                }
            }
            auto reverse = MapReadingSpan(*snapshot, cancel->Get(), false, annotationStart + 1, annotationStart + 2, true);
            const auto authored = fixture.source.find(">read<") + 2;
            Require(reverse.start.sourceByteOffset == authored && reverse.end.sourceByteOffset == authored + 1);
        }

        void CheckListMarkerCache(std::shared_ptr<ReadingMemory> const &memory,
                                  std::shared_ptr<MainTextSnapshot const> const &snapshot,
                                  std::shared_ptr<NativeMeasurementCache> const &cache, bool warm)
        {
            auto cancel = std::make_shared<PreviewCancellation>(memory->ModelOwner().Get());
            auto reading = std::allocate_shared<ReadingSourceState>(ReadingAllocator<ReadingSourceState>{memory},
                memory, cancel, snapshot->SourceGeneration(), snapshot->TextGeneration());
            reading->PublishMain(snapshot);
            auto renderer = NativeDocumentRenderer::Create({}, cancel, ListMarkerKey(*snapshot, 2400),
                                                           PreviewTheme{}, reading, {}, {}, {}, cache);
            auto const &statistics = renderer->ListMarkerStatistics();
            // 522 real text markers cross both the 256-group and 32-block boundaries.
            Require(statistics.batches == 3 && statistics.layouts == (warm ? 0u : 3u) &&
                    statistics.cacheHits == (warm ? 3u : 0u) && statistics.frameLayouts == 0);
            Require(reading->fault == 0 && renderer->Key().indent > 28);
        }
        void CheckListMarkerGeometry(std::shared_ptr<ReadingMemory> const &memory,
                                     std::shared_ptr<MainTextSnapshot const> const &snapshot)
        {
            auto cancel = std::make_shared<PreviewCancellation>(memory->ModelOwner().Get());
            auto reading = std::allocate_shared<ReadingSourceState>(ReadingAllocator<ReadingSourceState>{memory},
                memory, cancel, snapshot->SourceGeneration(), snapshot->TextGeneration());
            reading->PublishMain(snapshot);
            std::atomic<uint64_t> latest{1};
            auto make = [&](double width, float fontSize)
            {
                PreviewTheme theme;
                theme.fontSize = fontSize;
                theme.lineHeight = fontSize * 1.5f;
                auto renderer = NativeDocumentRenderer::Create({}, cancel, ListMarkerKey(*snapshot, width), theme, reading);
                Require(renderer->ListMarkerStatistics().frameLayouts == 0);
                auto frame = renderer->CreateFrame(0, 1000000, 1, latest);
                Require(frame && frame->main == snapshot && frame->main->Text() == snapshot->Text() && reading->fault == 0);
                return std::pair{std::move(renderer), std::move(frame)};
            };
            auto full = [&](PreviewFrame const &frame)
            {
                bool saw99{}, saw100{}, saw101{}, saw1000{}, sawBillion{}, sawHeading{}, sawCode{}, sawQuote{};
                uint32_t markers{};
                for (auto const &visual : frame.Blocks())
                    for (size_t i = 0; i < visual.decorations.size(); ++i)
                    {
                        auto const &decoration = visual.decorations[i];
                        if (IsPreviewTaskDecoration(decoration.kind))
                        {
                            auto start = static_cast<uint32_t>(decoration.logical_start);
                            auto rectangles = NativeDocumentRenderer::Rectangles(frame, start, start + 1);
                            Require(rectangles.size() == 1 && rectangles[0].width == 14);
                            continue;
                        }
                        auto const &marker = visual.markers[i];
                        Require(marker && marker.display == ListMarkerDisplay::Full);
                        DWRITE_LINE_METRICS line{};
                        UINT32 count{};
                        winrt::check_hresult(marker.get()->GetLineMetrics(&line, 1, &count));
                        Require(count == 1 && !line.isTrimmed && line.length == std::to_wstring(decoration.ordinal).size() + 1);
                        DWRITE_OVERHANG_METRICS overhang{};
                        winrt::check_hresult(marker.get()->GetOverhangMetrics(&overhang));
                        Require(marker.x - overhang.left >= -0.01f &&
                                marker.x + marker.get()->GetMaxWidth() + overhang.right <= marker.clipWidth + 0.01f &&
                                visual.markerX + marker.clipWidth <= visual.block.x - 6 + 0.01);
                        if (!visual.texts.empty())
                        {
                            auto const &text = visual.texts.front();
                            float baseline{};
                            if (text.scripts && !text.scripts->FinalLineMetrics().empty())
                                baseline = text.scripts->FinalLineMetrics().front().baseline;
                            else
                            {
                                UINT32 lines{};
                                auto status = text.layout->GetLineMetrics(nullptr, 0, &lines);
                                if (FAILED(status) && status != HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER))
                                    winrt::check_hresult(status);
                                std::vector<DWRITE_LINE_METRICS> metrics(lines);
                                winrt::check_hresult(text.layout->GetLineMetrics(metrics.data(), lines, &lines));
                                Require(lines > 0);
                                baseline = metrics.front().baseline;
                            }
                            Require(std::abs(marker.y + line.baseline - text.y - baseline) < 0.01f);
                        }
                        saw99 |= decoration.ordinal == 99;
                        saw100 |= decoration.ordinal == 100;
                        saw101 |= decoration.ordinal == 101;
                        saw1000 |= decoration.ordinal == 1000;
                        sawBillion |= decoration.ordinal == 1000000000;
                        sawHeading |= visual.block.kind == 2;
                        sawCode |= visual.block.kind == 3;
                        sawQuote |= visual.block.quote_depth > 0;
                        ++markers;
                    }
                Require(markers == 522 && saw99 && saw100 && saw101 && saw1000 && sawBillion &&
                        sawHeading && sawCode && sawQuote);
            };
            auto [wideRenderer, wide] = make(2400, 18);
            full(*wide);
            auto gutter = wideRenderer->Key().indent;
            auto [narrowRenderer, narrow] = make(gutter + 24, 18);
            Require(narrowRenderer->ListMarkerStatistics().batches == 0);
            uint32_t trimmed{}, clipped{};
            for (auto const &visual : narrow->Blocks())
                for (size_t i = 0; i < visual.decorations.size(); ++i)
                {
                    if (IsPreviewTaskDecoration(visual.decorations[i].kind))
                        continue;
                    auto const &marker = visual.markers[i];
                    if (marker.display == ListMarkerDisplay::Trimmed)
                    {
                        DWRITE_LINE_METRICS line{};
                        UINT32 count{};
                        winrt::check_hresult(marker.get()->GetLineMetrics(&line, 1, &count));
                        Require(count == 1 && line.isTrimmed);
                        ++trimmed;
                    }
                    clipped += marker.display == ListMarkerDisplay::Clipped;
                    Require(visual.markerX + marker.clipWidth <= visual.block.x + 0.01);
                }
            Require(trimmed > 0 && clipped > 0 && reading->fault == 0);
            auto [tinyRenderer, tiny] = make(8, 18);
            for (auto const &visual : tiny->Blocks())
                for (size_t i = 0; i < visual.decorations.size(); ++i)
                    if (!IsPreviewTaskDecoration(visual.decorations[i].kind))
                        Require(visual.markers[i].display == ListMarkerDisplay::Clipped);
            auto [restoredRenderer, restored] = make(2400, 18);
            Require(restoredRenderer->ListMarkerStatistics().batches == 0 && reading->fault == 0);
            full(*restored);
            auto [largeRenderer, large] = make(2400, 36);
            full(*large);
            Require(largeRenderer->Key().indent > gutter);
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

        winrt::Windows::Foundation::IAsyncAction WaitForPreview(
            winrt::Windows::UI::Core::CoreDispatcher dispatcher, std::shared_ptr<CoreValidation::Context> context,
            std::function<bool()> ready, wchar_t const *failure)
        {
            const auto deadline = GetTickCount64() + 8000;
            while (!ready())
            {
                context->Arbitrate();
                if (GetTickCount64() >= deadline)
                    throw winrt::hresult_error(HRESULT_FROM_WIN32(ERROR_TIMEOUT), failure);
                co_await winrt::resume_after(std::chrono::milliseconds(10));
                co_await winrt::resume_foreground(dispatcher);
            }
        }

        winrt::Windows::Foundation::IAsyncAction CheckRefreshViewportWait(
            winrt::Notepads::Markdown::MarkdownView view, winrt::Windows::UI::Core::CoreDispatcher dispatcher,
            std::shared_ptr<CoreValidation::Context> context)
        {
            using Access = winrt::Notepads::Markdown::implementation::PreviewPublicationTestAccess;
            co_await WaitForPreview(dispatcher, context, [&] { return Access::ViewportWaitSettled(view); },
                L"NotLoaded refresh regression did not start with a current frame.");
            auto retired = Access::RefreshIntoViewportWait(view);
            // The real controller completes and its actual UI parse observer
            // must make progress; the test never calls PreviewParsed directly.
            try
            {
                co_await WaitForPreview(dispatcher, context, [&] { return Access::ViewportWaitRecovered(view, retired); },
                    L"Successful current parse could not recover the forced NotLoaded refresh receipt.");
            }
            catch (winrt::hresult_error const &error)
            {
                throw winrt::hresult_error(error.code(), std::wstring{error.message().c_str()} + L" " + Access::ViewportWaitState(view));
            }
        }

        winrt::Windows::Foundation::IAsyncAction CheckLivePublication(
            winrt::Windows::UI::Core::CoreDispatcher dispatcher, std::shared_ptr<CoreValidation::Context> context,
            bool math)
        {
            using namespace winrt;
            using namespace winrt::Windows::Foundation;
            using namespace winrt::Windows::UI::Xaml;
            using namespace winrt::Windows::UI::Xaml::Controls;
            using namespace winrt::Notepads::Markdown;
            using Access = implementation::PreviewPublicationTestAccess;
            auto window = Window::Current();
            Require(window != nullptr);
            auto original = window.Content();
            auto parent = original.try_as<Panel>();
            bool replace = !parent;
            if (replace)
                parent = Grid{};
            auto view = make<implementation::MarkdownView>();
            view.Width(480);
            view.Height(200);
            view.CacheDocumentOnUnload(true);
            parent.Children().Append(view);
            struct Detach
            {
                Window window;
                UIElement original;
                Panel parent;
                MarkdownView view;
                bool replace;
                ~Detach() noexcept
                {
                    try
                    {
                        view.Cancel();
                        uint32_t index{};
                        if (parent.Children().IndexOf(view, index))
                            parent.Children().RemoveAt(index);
                        if (replace)
                            window.Content(original);
                    }
                    catch (...) {}
                }
            } detach{window, original, parent, view, replace};
            if (replace)
            {
                window.Content(parent);
                window.Activate();
            }
            co_await WaitForPreview(dispatcher, context, [&] { return view.IsLoaded() && Access::NativeLoaded(view); },
                                   L"Live publication fixture did not attach.");

            auto const source = math ? std::string_view{"$MMMMMMMMMMMMMMMMMMMM$\n\n$x$"}
                                     : std::string_view{"# Retained subject\n\nThe accepted source survives the stale notification."};
            guid document{0x86610043, 0x7734, 0x4602, {0xa8, 0xf2, 0x8d, 0x43, 0x5a, 0x97, 0x1a, 0x73}};
            auto update = view.SetDocumentWithExtensionsAsync(document, 1,
                {reinterpret_cast<uint8_t const *>(source.data()), static_cast<uint32_t>(source.size())},
                math ? MarkdownExtensions::Math : MarkdownExtensions::None);
            co_await WaitForPreview(dispatcher, context, [&] { return update.Status() != AsyncStatus::Started; },
                                   L"Initial publication did not complete.");
            auto initial = update.GetResults();
            Require(initial.Status() == DocumentUpdateStatus::Ready && initial.Published());

            if (!math)
            {
                co_await CheckRefreshViewportWait(view, dispatcher, context);
                // Simulate the late native notification without detaching XAML.
                Access::DeliverUnload(view);
                Require(view.IsLoaded() && !Access::NativeLoaded(view) && view.HasCachedDocument() &&
                        !view.IsCachedPresentationSuspended() && !view.IsSurfaceReady());
                auto wrong = co_await view.ResumeCachedDocumentAsync(document, 2);
                Require(wrong.Status() == DocumentUpdateStatus::ResyncRequired && !Access::NativeLoaded(view));
                view.Visibility(Visibility::Collapsed);
                auto hidden = co_await view.ResumeCachedDocumentAsync(document, 1);
                Require(hidden.Status() == DocumentUpdateStatus::NotLoaded && !Access::NativeLoaded(view));
                view.Visibility(Visibility::Visible);
                update = view.ResumeCachedDocumentAsync(document, 1);
                co_await WaitForPreview(dispatcher, context, [&] { return update.Status() != AsyncStatus::Started; },
                                       L"Cached resume did not complete after stale native unload.");
                auto resumed = update.GetResults();
                if (resumed.Status() != DocumentUpdateStatus::Ready || !resumed.Published() ||
                    !view.IsSurfaceReady() || view.DisplayedDocumentId() != document || view.DisplayedRevision() != 1)
                    throw hresult_error(E_FAIL, L"Attached cached preview could not recover its accepted revision.");
                auto generation = Access::Generation(view);
                Access::DeliverLoad(view);
                Require(Access::Generation(view) == generation && view.IsSurfaceReady());
                co_return;
            }

            com_ptr<implementation::ReadingObjectProvider> widePeer, shortPeer;
            const std::wstring_view rendered = L"Formula rendered.";
            const std::wstring_view limited = L"Formula resource or placement limit.";
            auto describes = [](auto const &peer, std::wstring_view status)
            {
                return peer && std::wstring_view{peer->GetHelpTextCore()}.starts_with(status);
            };
            co_await WaitForPreview(dispatcher, context, [&]
            {
                widePeer = Access::MathPeer(view, 0);
                shortPeer = Access::MathPeer(view, 1);
                return describes(widePeer, rendered) && describes(shortPeer, rendered);
            }, L"Formulas did not reach their wide ready placement.");
            (void)widePeer->Raw();
            (void)shortPeer->Raw();
            auto const mathGeneration = Access::MathGeneration(view);
            auto const wideChanges = widePeer->MathHelpTextChangesForDiagnostics();
            auto const shortChanges = shortPeer->MathHelpTextChangesForDiagnostics();
            view.Width(128);
            co_await WaitForPreview(dispatcher, context, [&]
            {
                return widePeer->MathHelpTextChangesForDiagnostics() == wideChanges + 1 &&
                       view.GetViewportAnchor().IsAvailable() && describes(widePeer, limited);
            }, L"The retained formula peer did not notify its narrow placement limit.");
            Require(describes(shortPeer, rendered) && shortPeer->MathHelpTextChangesForDiagnostics() == shortChanges &&
                    Access::MathGeneration(view) == mathGeneration && Access::MathPeer(view, 0).get() == widePeer.get());
            view.Width(480);
            co_await WaitForPreview(dispatcher, context, [&]
            {
                return widePeer->MathHelpTextChangesForDiagnostics() == wideChanges + 2 &&
                       view.GetViewportAnchor().IsAvailable() && describes(widePeer, rendered);
            }, L"The retained formula peer did not notify recovery at wide placement.");
            Require(describes(shortPeer, rendered) && shortPeer->MathHelpTextChangesForDiagnostics() == shortChanges &&
                    Access::MathGeneration(view) == mathGeneration && Access::MathPeer(view, 0).get() == widePeer.get());
        }

        winrt::Windows::Foundation::IAsyncAction RunLivePublicationCase(
            winrt::Windows::UI::Core::CoreDispatcher dispatcher, std::shared_ptr<CoreControllerTests::Results> results,
            std::shared_ptr<CoreValidation::Context> context, bool math)
        {
            CoreValidation::CheckResult check{
                math ? L"reading_math_help_text_on_relayout" : L"reading_cached_preview_after_stale_native_unload",
                true, 0, math ? L"Same peer and artifact generation, with HelpText notifications on Ready–Limited–Ready."
                              : L"Forced NotLoaded Refresh preserves its terminal receipt and reaches an independent Ready frame. Real native teardown while XAML is still attached; visible matching cache resumes, hidden and wrong revision do not."};
            try
            {
                co_await CheckLivePublication(dispatcher, context, math);
            }
            catch (winrt::hresult_error const &error)
            {
                check.passed = false;
                check.status = error.code().value;
                check.detail = error.message().c_str();
            }
            if (!check.passed)
                context->RecordPrimary(CoreValidation::FaultKind::Assertion, check.status, check.name.c_str());
            results->checks.push_back(std::move(check));
            context->Arbitrate();
        }

        winrt::Windows::Foundation::IAsyncAction RunCase(winrt::Windows::UI::Core::CoreDispatcher dispatcher,
                                                         std::shared_ptr<CoreControllerTests::Results> results,
                                                         std::shared_ptr<CoreValidation::Context> context, Case kind)
        {
            auto fixture = Input(kind);
            auto controller = CoreController::CreatePreview(dispatcher);
            auto lifetime = controller->Lifetime();
            CoreCompletionTicket ticket;
            CoreModelPin pin, identityPin;
            notepads_markdown_core_cancel *sourceFlag{}, *foreignFlag{}, *layoutFlag{};
            std::shared_ptr<ReadingMemory> memory;
            std::shared_ptr<MainTextSnapshot const> snapshot;
            std::shared_ptr<SemanticDescriptors const> semantics;
            std::exception_ptr failure;
            bool registered{}, sourceRegistered{};
            WorkerStackObservation mathStack{};
            MathWorkerTests::Results mathWorker{};
            MathRendererTests::Results mathRenderer{};
            DiagramRendererTests::Results diagramRenderer{};
            MeasurementCacheTests::Results measured{};
            ReadingSearchTests::Results search{};
            ReadingOutlineTests::Results outline{};
            TaskInteractionTests::Results taskInteraction{};
            bool pageReentrancy{};
            std::wstring inlineCodeDetail;
            std::wstring scriptDetail;
            std::wstring staticHtmlCheckboxDetail;
            std::wstring htmlTableDetail;
            std::wstring textBitmapDetail;
            std::wstring finalSurfaceThreadDetail;
            try
            {
                if (kind == Case::Ownership)
                {
                    std::exception_ptr concurrencyFailure;
                    co_await ControlConcurrencyTests::RunAsync(dispatcher, context, concurrencyFailure);
                    if (concurrencyFailure)
                        std::rethrow_exception(concurrencyFailure);
                }
                context->PublishController(controller.get());
                registered = true;
                auto descriptor = Pod<notepads_markdown_core_snapshot>();
                descriptor.source.document[0] = 0x53;
                descriptor.source.document[1] = static_cast<uint8_t>(kind);
                descriptor.source.document[15] = 0x29;
                descriptor.source.revision = 1;
                descriptor.profile = NOTEPADS_MARKDOWN_CORE_G3;
                auto submit = controller->SubmitUtf8(
                    {reinterpret_cast<uint8_t const *>(fixture.source.data()), fixture.source.size()}, descriptor,
                    &ticket, fixture.extensions);
                Check(submit.status);
                co_await winrt::resume_background();
                Require(Until(
                    [&]
                    {
                        context->Arbitrate();
                        return ticket.Done();
                    }));
                Check(ticket.Status());
                co_await winrt::resume_foreground(dispatcher);
                pin = controller->PinInstalled();
                if (kind == Case::Ownership)
                {
                    identityPin = controller->PinInstalled();
                    std::exception_ptr propertyFailure;
                    co_await CheckRangePropertyCalloutLifetime(dispatcher, context, propertyFailure);
                    if (propertyFailure)
                        std::rethrow_exception(propertyFailure);
                }
                co_await winrt::resume_background();
                Require(static_cast<bool>(pin));
                auto summary = Pod<notepads_markdown_core_model_summary_result>();
                auto error = Pod<notepads_markdown_core_error>();
                Check(notepads_markdown_core_model_summary(pin.Get(), &summary, &error));
                Check(notepads_markdown_core_model_cancel_create(pin.Get(), &sourceFlag, &error));
                context->Publish(sourceFlag);
                sourceRegistered = true;

                if (kind == Case::Ownership)
                {
                    auto wrong = summary.identity;
                    wrong.document[0] ^= 1;
                    ExpectStatus(NOTEPADS_MARKDOWN_CORE_WRONG_DOCUMENT,
                                 [&] { (void)ReadingMemory::Create(std::move(identityPin), sourceFlag, wrong); });
                }

                memory = ReadingMemory::Create(std::move(pin), sourceFlag, summary.identity);
                uint64_t token = NextReadingTextToken();
                Require(NextReadingTextToken() > token);
                snapshot = MainTextSnapshot::Build(memory, sourceFlag, 23, token);
                if (snapshot->Text() != fixture.expected || snapshot->Length() != fixture.expected.size())
                    throw AssertionFailure{NOTEPADS_MARKDOWN_CORE_INVALID_STATE,
                                           L"Fixed Main mismatch: expected length=" +
                                               std::to_wstring(fixture.expected.size()) + L", actual length=" +
                                               std::to_wstring(snapshot->Length()) + L"; actual prefix=" +
                                               std::wstring{snapshot->Text().substr(0, 512)}};
                Require(snapshot->Identity().revision == 1 &&
                        snapshot->Identity().generation == summary.identity.generation &&
                        snapshot->SourceGeneration() == 23 && snapshot->TextGeneration() == token);
                Require(snapshot->Memory() == memory && snapshot->ModelOwner().Get() == memory->ModelOwner().Get());
                Require(ExportEquals(*snapshot, fixture.expected));
                if (kind == Case::StaticHtmlCheckbox)
                    staticHtmlCheckboxDetail = TaskInteractionTests::RunStaticHtmlCheckbox(snapshot, sourceFlag);
                if (kind == Case::HtmlTables)
                    htmlTableDetail = InlineCodeTests::RunHtmlTables(snapshot, sourceFlag);
                if (kind == Case::EditImageWidths)
                    CheckEditImageWidths(memory, snapshot);
                if (kind == Case::TextBitmap)
                    textBitmapDetail = TextBitmapCacheTests::Run(memory);
                if (kind == Case::StaticTextBitmap)
                    textBitmapDetail = TextBitmapCacheTests::RunStatic(memory);
                if (kind == Case::FinalSurfaceThread)
                {
                    std::exception_ptr surfaceFailure;
                    co_await winrt::resume_foreground(dispatcher);
                    co_await FinalSurfaceThreadTests::RunAsync(memory, dispatcher, finalSurfaceThreadDetail, surfaceFailure);
                    if (surfaceFailure)
                        std::rethrow_exception(surfaceFailure);
                    co_await winrt::resume_background();
                }
                if (kind == Case::TaskInteraction)
                {
                    taskInteraction = TaskInteractionTests::Run(snapshot, sourceFlag);
                    Require(taskInteraction.passed && taskInteraction.sameDescriptorAcrossLayouts &&
                            taskInteraction.themedFocusPixels && taskInteraction.hiddenFocusHasNoPixels &&
                            taskInteraction.ownersReleased);
                }
                if (kind == Case::Outline || kind == Case::OutlineEmpty)
                {
                    outline = kind == Case::Outline ? ReadingOutlineTests::Run(snapshot, sourceFlag)
                                                   : ReadingOutlineTests::RunEmptyGeometry(snapshot, sourceFlag);
                    Require(outline.passed);
                    if (kind == Case::Outline)
                    {
                        auto directory = ReadingOutline::Build(*snapshot, sourceFlag);
                        winrt::guid document{};
                        std::memcpy(&document, snapshot->Identity().document, sizeof(document));
                        co_await winrt::resume_foreground(dispatcher);
                        {
                            auto projected = winrt::make<winrt::Notepads::Markdown::implementation::MarkdownOutline>(
                                document, snapshot->Identity().revision, snapshot->TextGeneration(), directory,
                                dispatcher);
                            pageReentrancy = ReadingOutlineTests::RunPageReentrancy(projected, memory);
                            Require(pageReentrancy);
                        }
                        directory.reset();
                        co_await winrt::resume_background();
                    }
                }
                if (kind == Case::Search)
                {
                    search = ReadingSearchTests::Run(snapshot);
                    Require(search.passed && search.permitsAndStops && search.queryFundingFailure &&
                            search.queryCapacityReleased && search.comparisons > 0);
                }
                if (kind == Case::Measurement)
                {
                    measured = MeasurementCacheTests::Run(memory);
                    Require(measured.geometryOracle && measured.currentLinks && measured.freshFrames &&
                            measured.configurationMiss && measured.tokenGuards && measured.epochAndContention &&
                            measured.budgets);
                    Require(measured.hits > 0 && measured.lines > 0 && measured.clusters > 0 &&
                            measured.cacheBytes + measured.nativeBytes <= 9u * 1024u * 1024u);
                }
                if (kind == Case::Scripts)
                    scriptDetail = ScriptTests::Run(memory);
                if (kind == Case::Ruby)
                    CheckRubyGeometry(memory, snapshot, fixture);
                if (kind == Case::InlineCode)
                {
                    auto code = InlineCodeTests::Run(memory);
                    inlineCodeDetail = std::move(code.detail);
                }
                if (kind == Case::Inline)
                {
                    // Two interior ASCII copy points have independent, authored
                    // source/Main offsets despite earlier UTF8, entity and syntax.
                    auto sourceAt = fixture.source.find("done");
                    auto mainAt = fixture.expected.find(L"done");
                    Require(sourceAt != std::string::npos && mainAt != std::wstring::npos);
                    auto cancel = std::make_shared<PreviewCancellation>(memory->ModelOwner().Get());
                    for (bool after : {false, true})
                    {
                        auto span = MapReadingSpan(*snapshot, cancel->Get(), true, sourceAt + 1, sourceAt + 2, after);
                        Require(span.start.sourceByteOffset == sourceAt + 1 && span.end.sourceByteOffset == sourceAt + 2 &&
                                span.start.previewUtf16Offset == mainAt + 1 && span.end.previewUtf16Offset == mainAt + 2);
                        auto reverse = MapReadingSpan(*snapshot, cancel->Get(), false, mainAt + 1, mainAt + 2, after);
                        Require(reverse.start.sourceByteOffset == sourceAt + 1 && reverse.end.sourceByteOffset == sourceAt + 2);
                        auto duplicate = MapReadingSpan(*snapshot, cancel->Get(), true, sourceAt + 1, sourceAt + 1, after);
                        Require(duplicate.start.sourceByteOffset == duplicate.end.sourceByteOffset &&
                                duplicate.start.previewUtf16Offset == duplicate.end.previewUtf16Offset);
                        // Boundary affinities retain the old chosen-point policy.
                        for (auto [first, last] : {std::pair<uint64_t, uint64_t>{0, 2}, {5, 8}})
                        {
                            auto batched = MapReadingSpan(*snapshot, cancel->Get(), true, first, last, after);
                            auto one = MapReadingPosition(*snapshot, cancel->Get(), true, first, after);
                            auto two = MapReadingPosition(*snapshot, cancel->Get(), true, last, after);
                            Require(batched.start.sourceByteOffset == one.sourceByteOffset &&
                                    batched.start.previewUtf16Offset == one.previewUtf16Offset &&
                                    batched.start.correspondence == one.correspondence && batched.start.effects == one.effects &&
                                    batched.end.sourceByteOffset == two.sourceByteOffset &&
                                    batched.end.previewUtf16Offset == two.previewUtf16Offset &&
                                    batched.end.correspondence == two.correspondence && batched.end.effects == two.effects);
                        }
                    }
                }
                if (kind == Case::Inline || kind == Case::LineEndings || kind == Case::Structured ||
                    kind == Case::TablePageNavigation || kind == Case::WrappedTablePageNavigation ||
                    kind == Case::SpanningCellNavigation || kind == Case::SpanningCellPageNavigation ||
                    kind == Case::WrappedPointerCaret || kind == Case::WideTableLineIndex ||
                    kind == Case::MathFallbackLineBreak || kind == Case::ScriptLineIndex ||
                    kind == Case::Scripts || kind == Case::Ruby)
                    CheckLineHits(memory, snapshot, kind);
                if (kind == Case::Structured)
                {
                    CheckQuoteGeometry(memory, snapshot, fixture);
                    textBitmapDetail = TextBitmapCacheTests::RunImageRelayout(memory);
                }
                if (kind == Case::ListMarkers)
                {
                    auto cache = NativeMeasurementCache::Create();
                    Require(static_cast<bool>(cache));
                    CheckListMarkerCache(memory, snapshot, cache, false);
                    auto edited = fixture.source;
                    auto row = edited.find("row");
                    Require(row != std::string::npos);
                    edited.replace(row, 3, "edited row");
                    auto editedDescriptor = descriptor;
                    editedDescriptor.source.revision = 2;
                    co_await winrt::resume_foreground(dispatcher);
                    Check(controller->SubmitUtf8(
                        {reinterpret_cast<uint8_t const *>(edited.data()), edited.size()}, editedDescriptor,
                        &ticket, fixture.extensions).status);
                    co_await winrt::resume_background();
                    Require(Until([&] { context->Arbitrate(); return ticket.Done(); }));
                    Check(ticket.Status());
                    co_await winrt::resume_foreground(dispatcher);
                    auto editedPin = controller->PinInstalled();
                    co_await winrt::resume_background();
                    auto editedCancel = std::make_shared<PreviewCancellation>(editedPin.Get());
                    auto editedSummary = Pod<notepads_markdown_core_model_summary_result>();
                    Check(notepads_markdown_core_model_summary(editedPin.Get(), &editedSummary, &error));
                    auto editedMemory = ReadingMemory::Create(std::move(editedPin), editedCancel->Get(), editedSummary.identity);
                    auto editedSnapshot = MainTextSnapshot::Build(editedMemory, editedCancel->Get(), 24, NextReadingTextToken());
                    Require(editedSnapshot->Identity().revision == 2 && editedSnapshot->Text().starts_with(L"edited row"));
                    CheckListMarkerCache(editedMemory, editedSnapshot, cache, true);
                    CheckListMarkerGeometry(memory, snapshot);
                }
                if (kind == Case::Math)
                {
                    // Exercise the actual Rust batch -> funded native descriptors ->
                    // copy chain. Fixed numerical geometry tests below are separate
                    // from a real dynamic engine artifact or product Ready state.
                    auto cancel = std::make_shared<PreviewCancellation>(memory->ModelOwner().Get());
                    auto reading = std::allocate_shared<ReadingSourceState>(
                        ReadingAllocator<ReadingSourceState>{memory}, memory, cancel, 23, token);
                    reading->PublishMain(snapshot);
                    notepads_markdown_preview_key key{};
                    key.header = {NOTEPADS_MARKDOWN_PREVIEW_ABI_VERSION, sizeof(key)};
                    key.model = summary.identity;
                    key.layout_generation = key.theme_generation = key.measurement_generation = 1;
                    key.width = 800;
                    key.indent = 24;
                    key.gap = 8;
                    auto renderer = NativeDocumentRenderer::Create({}, cancel, key, PreviewTheme{}, reading);
                    semantics = reading->Descriptors();
                    Require(semantics && semantics->objects.size() == 1);
                    auto const &object = semantics->objects[0];
                    Require(object.kind == ReadingObjectKind::Math && object.text.start == 4 && object.text.end == 5);
                    Require(object.input == L"a b" && object.source == L"$a\r\nb$");
                    Require(semantics->Copy(*snapshot, 0, snapshot->Length()) == L"pre $a\r\nb$ post");
                    Require(semantics->Copy(*snapshot, 4, 5) == L"$a\r\nb$");
                    mathStack = ObserveWorkerStack();
                    MathGeometryTests::Run(memory);
                    mathWorker = MathWorkerTests::Run(memory->ModelOwner().Get(), 1);
                    NativeMathKey mathKey{};
                    mathKey.identity = summary.identity;
                    mathKey.sourceGeneration = 23;
                    mathKey.textGeneration = token;
                    mathKey.attempt = mathKey.factoryEpoch = mathKey.engine = mathKey.fonts = mathKey.style = 1;
                    mathKey.object = object.object;
                    mathKey.syntax = object.syntax;
                    auto coordinatorChecks = MathCoordinatorTests::Run(memory->ModelOwner().Get(), mathKey);
                    Require(coordinatorChecks.queuedMetadataBeforeThread && coordinatorChecks.controlResident128 &&
                            coordinatorChecks.processResident256 && coordinatorChecks.rejected129And257 &&
                            coordinatorChecks.residentLastFreeReopensCapacity &&
                            coordinatorChecks.queuedRealFreeRestoresBaseline &&
                            coordinatorChecks.preparedThreadReallySignaled &&
                            coordinatorChecks.terminalRetiresJobKeepsResident &&
                            coordinatorChecks.terminalLastFreeRestoresBaseline &&
                            coordinatorChecks.prepareFailureDeduplicated && coordinatorChecks.expandedOrdinalsTracked &&
                            coordinatorChecks.oversizedOrdinalLimited);
                    mathRenderer = MathRendererTests::Run(memory);
                    Require(mathRenderer.pendingLabel && mathRenderer.typedLabels && mathRenderer.sourceAndCopy &&
                            mathRenderer.nativeReady && mathRenderer.sameFactoryDraw && mathRenderer.widthLimited &&
                            mathRenderer.arenaLimited && mathRenderer.immutableFrame && mathRenderer.arenaRecovered &&
                            mathRenderer.staleArtifactRejected && mathRenderer.staleSourceRejected &&
                            mathRenderer.nativeOwnersDropped && mathRenderer.baselineInkWithinInlineBox &&
                            mathRenderer.baselineInkMatchesTextBaseline);
                    Check(notepads_markdown_core_cancel_request(sourceFlag));
                }
                if (kind == Case::Diagram)
                {
                    diagramRenderer = DiagramRendererTests::Run(memory);
                    Require(diagramRenderer.pendingLabel && diagramRenderer.typedLabels &&
                            diagramRenderer.sourceAndCopy && diagramRenderer.atomicGeometry &&
                            diagramRenderer.nativeReady && diagramRenderer.exactInline &&
                            diagramRenderer.sameContextReuse && diagramRenderer.nativePixels &&
                            diagramRenderer.streamRetainsOwner && diagramRenderer.narrowFit &&
                            diagramRenderer.staleArtifactRejected && diagramRenderer.immutableFrame &&
                            diagramRenderer.nativeOwnersDropped);
                }
                snapshot->ValidateRange(snapshot->Length(), 0);
                ExpectStatus(NOTEPADS_MARKDOWN_CORE_INVALID_RANGE, [&] { snapshot->ValidateRange(snapshot->Length(), 1); });
                if (kind == Case::Inline || kind == Case::Ownership)
                {
                    Require(snapshot->IsBoundary(2) && !snapshot->IsBoundary(3) && snapshot->IsBoundary(4));
                    Require(snapshot->CopyText(2, 2) == L"😀");
                    ExpectStatus(NOTEPADS_MARKDOWN_CORE_INVALID_BOUNDARY, [&] { (void)snapshot->CopyText(2, 1); });
                    ExpectStatus(NOTEPADS_MARKDOWN_CORE_INVALID_BOUNDARY, [&] { (void)snapshot->CopyText(3, 1); });
                }
                if (kind == Case::Ownership)
                {
                    CheckNativeFailureConversion();
                    CheckAllocatorMoves(memory);
                    CheckRangeCalloutLifetime(snapshot);
                    Check(notepads_markdown_preview_cancel_create(&foreignFlag, &error));
                    ExpectStatus(NOTEPADS_MARKDOWN_CORE_WRONG_BUDGET, [&]
                                 { (void)MainTextSnapshot::Build(memory, foreignFlag, 23, NextReadingTextToken()); });
                    Check(notepads_markdown_core_model_cancel_create(memory->ModelOwner().Get(), &layoutFlag, &error));
                    Check(notepads_markdown_core_cancel_request(layoutFlag));
                    Require(snapshot->Text() == fixture.expected);
                    ExpectStatus(NOTEPADS_MARKDOWN_CORE_ARITHMETIC_OVERFLOW,
                                 [&] { (void)memory->Allocate(std::numeric_limits<size_t>::max()); });
                    ExpectStatus(NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT, [&] { (void)memory->Allocate(size_t{256} * 1024 * 1024); });
                    Check(notepads_markdown_core_cancel_request(sourceFlag));
                    ExpectStatus(NOTEPADS_MARKDOWN_CORE_CANCELLED, [&]
                                 { (void)MainTextSnapshot::Build(memory, sourceFlag, 23, NextReadingTextToken()); });
                    // Allocation funding is independent; cancellation did not
                    // authorize another producer, and did not revoke old reads.
                    Require(ExportEquals(*snapshot, fixture.expected));
                }
            }
            catch (...)
            {
                failure = std::current_exception();
            }

            if (sourceRegistered)
                context->Withdraw(sourceFlag);
            if (registered)
                context->WithdrawController(controller.get());
            controller->Stop();
            co_await winrt::resume_background();
            auto sourceCleanup = notepads_markdown_core_cancel_destroy(sourceFlag);
            auto foreignCleanup = notepads_markdown_core_cancel_destroy(foreignFlag);
            auto layoutCleanup = notepads_markdown_core_cancel_destroy(layoutFlag);
            context->RecordCleanup(sourceCleanup, fixture.name);
            context->RecordCleanup(foreignCleanup, fixture.name);
            context->RecordCleanup(layoutCleanup, fixture.name);
            sourceFlag = foreignFlag = layoutFlag = nullptr;
            pin.Reset();
            identityPin.Reset();
            memory.reset();
            auto drain = controller->Drain();
            results->lastDrain = drain;
            controller.reset();
            ticket = {};
            if (!failure)
            {
                try
                {
                    // Source/layout/caller flags and the controller are gone.
                    // Only the external snapshot keeps its real model pin alive.
                    Require(lifetime.Snapshot().modelPins == 1 && !lifetime.Snapshot().fullyReleased);
                    Require(snapshot->Text() == fixture.expected);
                    {
                        auto exported = snapshot->CopyText(0, snapshot->Length());
                        Require(std::wstring_view{exported.data(), exported.size()} == fixture.expected);
                    }
                    if (kind == Case::Math)
                    {
                        Require(semantics->Copy(*snapshot, 0, snapshot->Length()) == L"pre $a\r\nb$ post");
                        semantics.reset();
                    }
                    std::weak_ptr<ReadingMemory> oldMemory = snapshot->Memory();
                    std::weak_ptr<MainTextSnapshot const> oldRecord = snapshot;
                    snapshot.reset();
                    Require(oldMemory.expired() && oldRecord.expired());
                    Require(lifetime.Snapshot().modelPins == 0);
                    // The weak control block still owns only its independent
                    // allocation lease; final free does not borrow dead flags.
                    oldRecord.reset();
                    oldMemory.reset();
                }
                catch (...)
                {
                    failure = std::current_exception();
                }
            }
            semantics.reset();
            snapshot.reset();
            bool released = Until([&] { return lifetime.Snapshot().fullyReleased; });
            auto final = lifetime.Snapshot();
            results->lastLifetime = final;
            CoreValidation::CheckResult check{
                fixture.name,
                !failure && released && drain.coreDrained && drain.driverExited && drain.primaryStatus == NOTEPADS_MARKDOWN_CORE_OK &&
                    drain.cleanupStatus == NOTEPADS_MARKDOWN_CORE_OK && drain.primaryHresult == 0 && drain.cleanupHresult == 0 &&
                    final.cleanupStatus == NOTEPADS_MARKDOWN_CORE_OK && final.cleanupHresult == 0,
                NOTEPADS_MARKDOWN_CORE_OK,
                L"Real G3 Main stream, fixed independent UTF16 expectation, exact model/token, scalar ranges and old "
                L"snapshot "
                L"read/export after controller/flags retire; last strong model pin and weak control block released. "
                L"No visual, clipboard, natural-cancellation latency or physical-memory claim."};
            if (kind == Case::Structured && check.passed)
                check.detail += L" Authored loose-item/code/quote edges, quote-first bullet/task gutters, seven complete "
                                L"quote rectangles and nested/sibling ancestry; actual task hit/caret/selection and "
                                L"empty-task navigation; CRLF mixed UTF8/UTF16 span roundtrip; gap-only/mid-container "
                                L"frames and Reposition owner sharing; WARP gap/background/border/nested-tint and "
                                L"programmatic HC palette probes. No actual HC switch or public UI claim.";
            if (kind == Case::Structured && check.passed)
                check.detail += textBitmapDetail;
            if (kind == Case::TablePageNavigation && check.passed)
                check.detail += L" Authored three-column table at both document edges: PageUp/PageDown overshoot "
                                L"preserves every preferred column, ordinary Up/Down and boundary no-op behavior.";
            if (kind == Case::WrappedTablePageNavigation && check.passed)
                check.detail += L" Actual DWrite wrapped final middle cell: both shorter columns reach their final row "
                                L"when PageDown targets within the wraps or overshoots; ordinary Up/Down and final-line no-op preserved.";
            if (kind == Case::ListMarkers && check.passed)
                check.detail += L" 522 authored markers: 3 cold DWrite groups, 3 cache hits/0 measured groups after a real "
                                L"revision-2 body edit; no measurement-phase marker COM. Full 99/100/101, 999/1000 and "
                                L"999999999/1000000000 digits, heading/code/quote first-line baselines, task selection; "
                                L"narrow Trimmed/isTrimmed and zero-gutter Clipped states restore to Full without source fault; "
                                L"36-DIP font and same-source resize reuse. Native geometry only; public GUI remains separate.";
            if (kind == Case::TaskInteraction && check.passed)
                check.detail += taskInteraction.detail;
            if (kind == Case::StaticHtmlCheckbox && check.passed)
                check.detail += staticHtmlCheckboxDetail;
            if (kind == Case::HtmlTables && check.passed)
                check.detail += htmlTableDetail;
            if (kind == Case::EditImageWidths && check.passed)
                check.detail += L" Edit-only image width proofs match every measured paragraph, GFM cell and HTML "
                                L"colspan/rowspan owner at 603, 71 and 9 DIPs; fixed wide/narrow width oracles, "
                                L"eight distinct images and no covered-cell duplicate. No resource IO or timing claim.";
            if ((kind == Case::TextBitmap || kind == Case::StaticTextBitmap) && check.passed)
                check.detail += textBitmapDetail;
            if (kind == Case::FinalSurfaceThread && check.passed)
                check.detail += finalSurfaceThreadDetail;
            if (kind == Case::Search && check.passed)
                check.detail += search.detail;
            if (kind == Case::Measurement && check.passed)
                check.detail += L" Real renderer long-table FollowY plateau and unchanged exact caret/visual geometry; "
                                L"lineSize=" + std::to_wstring(measured.lineSize) +
                                L", lineCapacity=" + std::to_wstring(measured.lineCapacity) +
                                L", lineRequestedBytes=" + std::to_wstring(measured.lineAllocationBytes) +
                                L"; actual larger allocation physically freed and same-root lease retired once. "
                                L"Original cache/native-wrapper ceilings unchanged; no RSS claim.";
            if (kind == Case::InlineCode && check.passed)
                check.detail += inlineCodeDetail;
            if (kind == Case::Scripts && check.passed)
                check.detail += scriptDetail;
            if ((kind == Case::Outline || kind == Case::OutlineEmpty) && check.passed)
                check.detail += outline.detail;
            if (kind == Case::Outline && check.passed && pageReentrancy)
                check.detail += L" Actual UI-thread IVectorView ABI IndexOf handled an external Heading QI that "
                                L"released its caller's last page reference; self pin retained paid payload through "
                                L"the callback and final page capacity retired before returning to the caller.";
            if (kind == Case::Ownership && check.passed)
                check.detail +=
                    L" Real MTA Cancel before/after deferred UI ticket binding retires the exact request, preserves a later successful request and releases native owners; agile Theme defaults and 2-writer/2-reader property access also pass. Funded long strings and real link/image descriptors retain their root across sorting, "
                    L"move-back, same-root swap and moved-from allocator/container reuse. Actual Range ABI Compare/CompareEndpoints "
                    L"retain self through foreign identity callouts dropping the last caller reference, then retire before ABI return. Actual live-view Range ABI FindAttribute retains self through a foreign Boolean getter. The three current same-row follow APIs supersede a real deferred disclosure reveal; stale-layout rejections preserve its later navigation.";
            if (kind == Case::Math && check.passed)
            {
                check.detail += L" Separate fixed numeric D2D/WIC pixels, 13 actual DWrite inline/display fixtures and "
                                L"funded path ownership passed; numeric fixtures alone do not qualify public UI. "
                                L"Observed actual background-thread stack limits span=" +
                                std::to_wstring(mathStack.span) + L", markerAboveLower=" +
                                std::to_wstring(mathStack.markerAboveLower) +
                                L" bytes; observation only, not an audited Math headroom/profile.";
                check.detail +=
                    L" Real controlled Math thread: insufficient measured headroom rejected; 64MiB and actual-job held through "
                    L"thread tail, signaled-only completion, cancellation, injected CreateThread-site failure and "
                    L"real policy baseline restoration passed. committed=" +
                    std::to_wstring(mathWorker.stack.committedBytes) + L", guard=" +
                    std::to_wstring(mathWorker.stack.guardBytes) + L", allocation=" +
                    std::to_wstring(mathWorker.stack.allocationBytes) + L", guaranteedHeadroom=" +
                    std::to_wstring(mathWorker.stack.headroom) +
                    L" bytes. Production-profile formula built funded geometry: commands=" +
                    std::to_wstring(mathWorker.commands) + L", primitives=" + std::to_wstring(mathWorker.primitives) +
                    L", advance=" + std::to_wstring(mathWorker.advance) + L", actualRetained=" +
                    std::to_wstring(mathWorker.actualRetained) +
                    L"; arena16/17 refusal/reuse, heap-growth refusal, weak-owner and final policy baseline passed. "
                    L"Prepared residents128/256, terminal metadata retirement and actual worker signaled passed; "
                    L"real renderer Ready/Pending/typed labels, width/arena limits, immutable old Frame, "
                    L"same-factory draw, source/export and native-owner restoration passed. "
                    L"Real WARP a b ink pixels=" +
                    std::to_wstring(mathRenderer.baselineInkPixels) + L", observed top/bottom=" +
                    std::to_wstring(mathRenderer.baselineInkTop) + L"/" +
                    std::to_wstring(mathRenderer.baselineInkBottom) + L", independent expected=" +
                    std::to_wstring(mathRenderer.baselineExpectedTop) + L"/" +
                    std::to_wstring(mathRenderer.baselineExpectedBottom) + L", actual DWrite baseline=" +
                    std::to_wstring(mathRenderer.baselineTextY) +
                    L"; absolute baseline and inline-box containment passed. "
                    L"This diagnostic alone does not qualify public dynamic UI or ARM64 runtime.";
            }
            if (kind == Case::Diagram && check.passed)
                check.detail += L" Actual Diagram worker and builtin profile, 13 renderer ownership/state/geometry "
                                L"checks, real WARP SVG pixels=" +
                                std::to_wstring(diagramRenderer.inkPixels) +
                                L". Bitmap nonempty alone does not assert full visual parity.";
            if (failure)
            {
                try
                {
                    std::rethrow_exception(failure);
                }
                catch (ReadingFailure const &error)
                {
                    check.status = error.status;
                    check.detail = L"Reading snapshot fixture or typed ownership/cancellation/quota control failed.";
                }
                catch (AssertionFailure const &error)
                {
                    check.status = error.status;
                    check.detail = error.detail;
                }
                catch (TextBitmapCacheTests::Failure const &error)
                {
                    check.status = E_FAIL;
                    check.detail = error.detail;
                }
                catch (winrt::hresult_error const &error)
                {
                    check.status = error.code().value;
                    check.detail = error.message().c_str();
                }
                catch (...)
                {
                    check.status = NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT;
                    check.detail = L"Unexpected reading snapshot test exception; controller still stopped and drained.";
                }
            }
            else if (!check.passed)
                check.status = NOTEPADS_MARKDOWN_CORE_INVALID_STATE;
            if (!check.passed)
                context->RecordPrimary(CoreValidation::FaultKind::Assertion, check.status, fixture.name);
            context->RecordCleanup(drain.cleanupStatus, fixture.name);
            context->RecordCleanup(drain.cleanupHresult, fixture.name);
            context->RecordCleanup(final.cleanupStatus, fixture.name);
            context->RecordCleanup(final.cleanupHresult, fixture.name);
            if (!released)
                context->RecordCleanupInvariant(fixture.name);
            results->checks.push_back(std::move(check));
            context->Arbitrate();
            co_await winrt::resume_foreground(dispatcher);
        }
    } // namespace

    uint32_t CheckCount() noexcept
    {
        return static_cast<uint32_t>(Cases.size()) + 2;
    }

    winrt::Windows::Foundation::IAsyncAction RunAsync(winrt::Windows::UI::Core::CoreDispatcher const &dispatcher,
                                                      std::shared_ptr<CoreControllerTests::Results> const &results,
                                                      std::shared_ptr<CoreValidation::Context> const &context)
    {
        if (!dispatcher || !dispatcher.HasThreadAccess() || !results || !context)
            throw winrt::hresult_invalid_argument();
        auto ui = dispatcher;
        auto output = results;
        auto owner = context;
        output->checks.reserve(output->checks.size() + CheckCount());
        co_await RunLivePublicationCase(ui, output, owner, false);
        co_await RunLivePublicationCase(ui, output, owner, true);
        for (auto kind : Cases)
        {
            owner->Arbitrate();
            co_await RunCase(ui, output, owner, kind);
        }
    }
} // namespace Notepads::Markdown::Native::ReadingSnapshotTests
