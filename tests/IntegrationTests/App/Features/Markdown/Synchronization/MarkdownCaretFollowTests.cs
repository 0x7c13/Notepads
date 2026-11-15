// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Notepads.Markdown;
using NotepadsWorkspaceTests;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Automation.Provider;
using Windows.UI.Xaml.Controls;
using static NotepadsWorkspaceTests.DocumentFeatureAssertions;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal static partial class MarkdownFeatureTests
{
    private static async Task ScrollProgressFollowAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var source = string.Join("\r\r", Enumerable.Range(0, 160).Select(index =>
            index < 100 ? $"# Tall heading {index}" : $"Plain paragraph {index}"));
        var editor = await OpenAsync(host, "scroll-progress.md", Encoding.UTF8.GetBytes(source));
        var core = editor.TestNativeCore;
        var native = core.TestFeatureNative;
        var sourceView = (WinUIEditor.EditorBaseControl)((Grid)core.Content).Children[0];
        editor.ShowHideContentPreview();
        var extension = await CurrentAsync(editor);
        var preview = extension.PreviewView;
        await UntilAsync(() => sourceView.VerticalScrollProgress >= 0 && preview.VerticalScrollProgress >= 0,
            "Both differently formatted panes have actual vertical travel.");
        var selection = (native.Anchor, native.CurrentPos, native.SelectionRevision);

        extension.TestBeginViewportScroll(preview: false);
        Check(sourceView.TrySetVerticalScrollProgress(1), "The source admits its exact bottom.");
        var peer = FrameworkElementAutomationPeer.FromElement(preview) ?? FrameworkElementAutomationPeer.CreatePeerForElement(preview);
        var reading = (ITextProvider)peer.GetPattern(PatternInterface.Text);
        await UntilAsync(() => sourceView.VerticalScrollProgress == 1 && preview.VerticalScrollProgress == 1 &&
            preview.GetViewportAnchor().IsAvailable && reading.GetVisibleRanges().Any(range =>
                range.GetText(-1).Contains("Plain paragraph 159", StringComparison.Ordinal)),
            "The preview paints its last paragraph even when the source top row is before EOF.");

        extension.TestBeginViewportScroll(preview: true);
        Check(preview.TrySetVerticalScrollProgress(0), "The preview admits its exact top.");
        await UntilAsync(() => preview.VerticalScrollProgress == 0 && sourceView.VerticalScrollProgress == 0,
            "The preview also drives the source to its physical top.");
        preview.TrySetVerticalScrollProgress(0.75);
        preview.TrySetVerticalScrollProgress(0.25);
        await UntilAsync(() => Math.Abs(preview.VerticalScrollProgress - 0.25) < 0.005 &&
            preview.GetViewportAnchor().PreviewUtf16Offset > 0,
            "A rapid preview reversal draws the latest interior position.");
        var anchor = preview.GetViewportAnchor();
        var mapped = await preview.MapPreviewPositionAsync(anchor.DocumentId, anchor.Revision,
            anchor.TextGeneration, anchor.PreviewUtf16Offset, MarkdownPositionAffinity.After);
        var expectedLine = native.LineFromPosition((long)mapped.SourceByteOffset);
        await UntilAsync(() => Math.Abs(native.DocLineFromVisible(native.FirstVisibleLine) - expectedLine) <= 1,
            "The source follows the latest rendered content after the reversal.");
        Check((native.Anchor, native.CurrentPos, native.SelectionRevision) == selection,
            "Viewport following never changes the source selection.");
        log.AppendLine("PASS: Markdown.ScrollProgressFollow — physical edges in both directions, latest reversal and unchanged selection.");
    }

    private static async Task CaretScrollArbitrationAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        // Tall headings make source-top following insufficient to reveal a caret near the source bottom.
        var source = string.Join("\r\r", Enumerable.Range(0, 180).Select(index => "# E1MARK" + index.ToString("D4")));
        var editor = await OpenAsync(host, "caret-scroll-arbitration.md", Encoding.UTF8.GetBytes(source));
        var core = editor.TestNativeCore;
        var native = core.TestFeatureNative;
        var nativeView = (WinUIEditor.EditorBaseControl)((Grid)core.Content).Children[0];
        core.TextWrapping = Windows.UI.Xaml.TextWrapping.NoWrap;
        editor.ShowHideContentPreview();
        var extension = await CurrentAsync(editor);
        var preview = extension.PreviewView;
        extension.FollowSource = true;
        editor.Focus();
        var peer = FrameworkElementAutomationPeer.FromElement(preview) ?? FrameworkElementAutomationPeer.CreatePeerForElement(preview);
        var text = peer.GetPattern(PatternInterface.Text) as ITextProvider;
        Check(text != null, "The E1 fixture exposes real preview visible text ranges.");
        long Byte(string marker) => Encoding.UTF8.GetByteCount(source.AsSpan(0, source.IndexOf(marker, StringComparison.Ordinal)));
        bool Visible(string marker) => text.GetVisibleRanges().Any(range => range.GetText(-1).Contains(marker, StringComparison.Ordinal));
        bool Selected(ulong position)
        {
            var selection = preview.GetSelection();
            return selection.IsAvailable && extension.SourceSession.IsCurrent &&
                selection.DocumentId == extension.SourceSession.DocumentId &&
                selection.Revision == extension.SourceSession.AcceptedRevision && selection.TextGeneration != 0 &&
                selection.TextGeneration == preview.GetViewportAnchor().TextGeneration &&
                selection.Start == position && selection.Length == 0;
        }
        async Task Caret(string marker, string label)
        {
            var position = checked((ulong)preview.DisplayedText.IndexOf(marker, StringComparison.Ordinal));
            await UntilAsync(() => native.LineFromPosition(native.CurrentPos) == native.LineFromPosition(Byte(marker)) &&
                Selected(position) && Visible(marker), label);
            await TabDragTestHost.FrameAsync();
            await TabDragTestHost.FrameAsync();
            Check(Selected(position) && Visible(marker), "Natural Rendering keeps the exact current caret marker visible: " + label);
        }
        async Task ScrollSelection(string label)
        {
            var identity = preview.GetSelection();
            var mapped = await preview.MapSourcePositionAsync(identity.DocumentId, identity.Revision,
                identity.TextGeneration, checked((ulong)native.CurrentPos), MarkdownPositionAffinity.After);
            await UntilAsync(() => nativeView.IsSelectionFromScroll && Selected(mapped.PreviewUtf16Offset), label);
        }

        void GoTo(string marker) => Check(core.GoTo(checked((int)native.LineFromPosition(Byte(marker))) + 1),
            "The real Go To Line command accepts " + marker + ".");

        GoTo("E1MARK0000");
        await Caret("E1MARK0000", "Explicit navigation starts with the first marker visible.");
        await UntilAsync(() => native.LinesOnScreen > 10, "The source shows multiple visible headings.");
        var clicked = $"E1MARK{Math.Min(20, (native.LinesOnScreen - 1) / 2):D4}";
        Check(!Visible(clicked), "Tall preview headings put a visible source line outside the preview viewport.");
        var fixedSourceTop = native.FirstVisibleLine;
        var fixedPreview = preview.GetViewportAnchor();
        Check(preview.Focus(Windows.UI.Xaml.FocusState.Programmatic), "The preview can own focus before the source click.");
        editor.Focus();
        native.SetEmptySelection(Byte(clicked));
        var clickedPosition = checked((ulong)preview.DisplayedText.IndexOf(clicked, StringComparison.Ordinal));
        await UntilAsync(() => Selected(clickedPosition), "The new source caret is marked in the preview.");
        await TabDragTestHost.FrameAsync();
        Check(native.FirstVisibleLine == fixedSourceTop && !Visible(clicked) &&
            preview.GetViewportAnchor().PreviewUtf16Offset == fixedPreview.PreviewUtf16Offset,
            "Selecting another visible source line keeps both viewports steady.");
        GoTo("E1MARK0120");
        Check(native.FirstVisibleLine != fixedSourceTop, "Go To Line also moves the source viewport.");
        await Caret("E1MARK0120", "The explicit Go To Line command reveals the target in the preview.");
        extension.SourceSession.RequestFullSync();
        GoTo("E1MARK0130");
        await CurrentAsync(editor);
        await Caret("E1MARK0130", "Go To Line issued during an update reveals after the new preview is ready.");

        // Ctrl+Down/Up share these exact public native commands (the real KeyMap entries).
        native.GotoPos(Byte("E1MARK0080"));
        native.FirstVisibleLine = Math.Max(0, native.LineFromPosition(native.CurrentPos) - native.LinesOnScreen / 2);
        await UntilAsync(() => !nativeView.IsViewportRestorePending && native.LinesOnScreen > 4,
            "Ctrl-scroll has a real source viewport with the caret inside it.");
        await TabDragTestHost.FrameAsync();
        var row = native.FirstVisibleLine;
        var caret = native.CurrentPos;
        native.LineScrollDown();
        Check(native.FirstVisibleLine == row + 1 && native.CurrentPos == caret && nativeView.IsSelectionFromScroll,
            "Real Ctrl+Down command scrolls one interior row without changing the caret and owns viewport authority.");
        native.LineScrollUp();
        Check(native.FirstVisibleLine == row && native.CurrentPos == caret && nativeView.IsSelectionFromScroll,
            "Real Ctrl+Up command reverses the row and preserves its scroll origin.");

        extension.FollowSource = false;
        native.SetEmptySelection(Byte("E1MARK0000"));
        native.FirstVisibleLine = 120;
        await TabDragTestHost.FrameAsync();
        extension.FollowSource = true;
        var edgeMinimum = checked((ulong)preview.DisplayedText.IndexOf("E1MARK0055", StringComparison.Ordinal));
        await UntilAsync(() => !nativeView.IsViewportRestorePending && native.FirstVisibleLine == 120 &&
            preview.GetViewportAnchor().IsAvailable && preview.GetViewportAnchor().PreviewUtf16Offset > edgeMinimum &&
            Visible("E1MARK0060"),
            "Follow restoration presents the current source viewport before the edge Ctrl-scroll command.");
        var anchorBefore = preview.GetViewportAnchor().PreviewUtf16Offset;
        caret = native.CurrentPos;
        native.LineScrollDown();
        Check(native.CurrentPos != caret && nativeView.IsSelectionFromScroll,
            "Ctrl+Down really pulled an offscreen caret inside the source viewport through the scroll-origin setter.");
        await ScrollSelection("The scroll-origin caret remains synchronized without taking Reveal authority.");
        await TabDragTestHost.FrameAsync();
        await TabDragTestHost.FrameAsync();
        Check(preview.GetViewportAnchor().PreviewUtf16Offset >= anchorBefore,
            "Scroll-origin selection did not reveal backwards over the forward viewport intent.");

        native.FirstVisibleLine = 0;
        native.SetEmptySelection(0);
        native.LineScrollUp();
        Check(native.FirstVisibleLine == 0 && nativeView.IsSelectionFromScroll,
            "Boundary Ctrl+Up is a real no-op input with a bounded current origin stamp.");
        GoTo("E1MARK0140");
        Check(!nativeView.IsSelectionFromScroll, "Explicit navigation invalidates the no-op origin.");
        await Caret("E1MARK0140", "No-op scroll cannot swallow an explicit Go To Line.");

        native.SetEmptySelection(Byte("E1MARK0000"));
        native.LineScrollDown();
        await ScrollSelection("Selection then Ctrl-scroll in one UI turn gives the later viewport intent authority.");
        native.LineScrollUp();
        GoTo("E1MARK0160");
        await Caret("E1MARK0160", "A Go To Line after Ctrl-scroll takes navigation authority.");
        log.AppendLine("PASS: Markdown.CaretScrollArbitration — independent selection marker and explicit Go To Line reveal; real native CtrlDown/Up interior/edge/no-op; no-op and coalesced order restore ordinary navigation. These are native commands, not physical wheel/thumb input.");

        async Task DuringPublication(string marker, Action reenter, string label, bool keepSourceViewport = false)
        {
            var position = checked((ulong)preview.DisplayedText.IndexOf(marker, StringComparison.Ordinal));
            bool fired = false;
            void Reenter(MarkdownView sender, MarkdownSelection range)
            {
                if (fired || !range.IsAvailable || range.Start != position || range.Length != 0) return;
                fired = true;
                reenter();
            }
            preview.SelectionChanged += Reenter;
            try
            {
                var sourceTop = native.FirstVisibleLine;
                GoTo(marker);
                // Keep the explicit caret reveal intent while restoring the driver viewport
                // in the same UI turn. Direct selection setters below never EnsureVisible.
                if (keepSourceViewport) native.FirstVisibleLine = sourceTop;
                await UntilAsync(() => fired && !extension.TestSourceFindBusy, label);
                await TabDragTestHost.FrameAsync();
                await TabDragTestHost.FrameAsync();
            }
            finally { preview.SelectionChanged -= Reenter; }
        }

        var beforeFollowOff = preview.VerticalScrollProgress;
        Check(!Visible("E1MARK0020"), "The follow-off callback target starts outside the current preview viewport.");
        await DuringPublication("E1MARK0020", () => extension.FollowSource = false,
            "The actual collapsed selection notification disables follow before its old reveal returns.");
        Check(!extension.FollowSource && Math.Abs(preview.VerticalScrollProgress - beforeFollowOff) < 0.000001 &&
            !Visible("E1MARK0020"), "Follow-off inside SelectionChanged retires the old Go To reveal before any viewport change.");

        extension.FollowSource = true;
        GoTo("E1MARK0000");
        await Caret("E1MARK0000", "The nested selection case begins from the physical top.");
        var beforeNestedSelection = preview.VerticalScrollProgress;
        var externalPosition = checked((ulong)preview.DisplayedText.IndexOf("E1MARK0070", StringComparison.Ordinal));
        await DuringPublication("E1MARK0120", () =>
        {
            var identity = preview.GetSelection();
            preview.SelectRange(identity.DocumentId, identity.Revision, identity.TextGeneration, externalPosition, 10);
        }, "An actual SelectionChanged subscriber publishes a newer external range.");
        var externalSelection = preview.GetSelection();
        Check(externalSelection.Start == externalPosition && externalSelection.Length == 10 &&
            Math.Abs(preview.VerticalScrollProgress - beforeNestedSelection) < 0.000001,
            "The external selection survives and the retired ordinary caret cannot reveal over its viewport.");

        editor.Focus();
        GoTo("E1MARK0000");
        await Caret("E1MARK0000", "The direct source caret replacement begins from the first marker.");
        var beforeDirectCaret = (SourceTop: native.FirstVisibleLine, Preview: preview.VerticalScrollProgress);
        var replacementByte = Byte("E1MARK0070");
        Check(!Visible("E1MARK0120"), "The direct source caret callback's old target starts offscreen.");
        await DuringPublication("E1MARK0120", () => native.SetEmptySelection(replacementByte),
            "The actual preview selection callback directly replaces the source caret before UpdateUI.", keepSourceViewport: true);
        Check(native.CurrentPos == replacementByte && native.Anchor == replacementByte &&
            native.FirstVisibleLine == beforeDirectCaret.SourceTop &&
            Math.Abs(preview.VerticalScrollProgress - beforeDirectCaret.Preview) < 0.000001 && !Visible("E1MARK0120"),
            "A direct source caret replacement remains selected and the retired target cannot reveal over the unchanged viewports.");

        GoTo("E1MARK0000");
        await Caret("E1MARK0000", "The direct source range replacement begins from the first marker.");
        var beforeDirectRange = (SourceTop: native.FirstVisibleLine, Preview: preview.VerticalScrollProgress);
        long preservedCaret = -1;
        ulong replacedSelectionRevision = 0;
        Check(!Visible("E1MARK0120"), "The same-caret range callback's old target starts offscreen.");
        await DuringPublication("E1MARK0120", () =>
        {
            preservedCaret = native.CurrentPos;
            replacedSelectionRevision = native.SelectionRevision;
            native.SetSelection(preservedCaret, replacementByte);
        }, "The actual preview selection callback changes only the source anchor before UpdateUI.", keepSourceViewport: true);
        Check(native.CurrentPos == preservedCaret && native.Anchor == replacementByte &&
            native.SelectionRevision > replacedSelectionRevision && native.FirstVisibleLine == beforeDirectRange.SourceTop &&
            Math.Abs(preview.VerticalScrollProgress - beforeDirectRange.Preview) < 0.000001 && !Visible("E1MARK0120"),
            "Changing the source range with the same active caret retires the old reveal and preserves the replacement selection and both viewports.");

        GoTo("E1MARK0000");
        await Caret("E1MARK0000", "The mapped caret replacement begins from the first marker.");
        var beforeMappedRange = (SourceTop: native.FirstVisibleLine, Preview: preview.VerticalScrollProgress);
        Check(!Visible("E1MARK0120"), "The mapped caret's old target starts outside the preview.");
        long mappedCaret = -1;
        // Replace the anchor in the same UI turn as mapping completion,
        // before Scintilla delivers the queued selection notification.
        extension.TestBeforeNextSourceCaretReveal(() =>
        {
            mappedCaret = native.CurrentPos;
            native.SetSelection(mappedCaret, replacementByte);
        });
        GoTo("E1MARK0120");
        native.FirstVisibleLine = beforeMappedRange.SourceTop;
        await UntilAsync(() => mappedCaret >= 0 && !extension.TestSourceFindBusy,
            "The explicit mapping retires after a same-caret anchor change preceding preview publication.");
        await TabDragTestHost.FrameAsync();
        Check(native.LineFromPosition(mappedCaret) == native.LineFromPosition(Byte("E1MARK0120")) &&
            native.CurrentPos == mappedCaret && native.Anchor == replacementByte &&
            native.FirstVisibleLine == beforeMappedRange.SourceTop &&
            Math.Abs(preview.VerticalScrollProgress - beforeMappedRange.Preview) < 0.000001 && !Visible("E1MARK0120"),
            "Changing the anchor after mapping prevents its old reveal and preserves both viewports.");

        GoTo("E1MARK0000");
        await Caret("E1MARK0000", "The superseding navigation case begins from the first marker.");
        var newerMarker = $"E1MARK{Math.Min(170, 120 + Math.Max(12, (native.LinesOnScreen - 1) / 2)):D4}";
        bool newerAccepted = false;
        await DuringPublication("E1MARK0120", () =>
            newerAccepted = core.GoTo(checked((int)native.LineFromPosition(Byte(newerMarker))) + 1),
            "An actual SelectionChanged subscriber issues a newer explicit Go To Line.");
        Check(newerAccepted, "The synchronous subscriber's superseding Go To Line is accepted.");
        await Caret(newerMarker, "The newer explicit Go To keeps selection and reveal authority after the old publication returns.");
        log.AppendLine("PASS: Markdown.CaretPublicationReentry — actual SelectionChanged callbacks disable follow, preserve external preview/source ranges and unchanged viewports, reject a source range replaced after mapping, and supersede ordinary caret publication with a newer Go To reveal.");
    }
}
