// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Notepads.Presentation.Controls.DiffViewer;
using Notepads.Presentation.Controls.Print;
using Notepads.Presentation.Controls.TextEditor;
using NotepadsEditorTests;
using NotepadsWorkspaceTests;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Media;
using WinUIEditor;
using static NotepadsIntegrationTests.FeatureUI;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal static class ReadingOrderFeatureTests
{
    internal static async Task RunAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        ((FrameworkElement)Window.Current.Content).FlowDirection = FlowDirection.RightToLeft;
        var first = host.Core.GetSelectedTab();
        var editor = Editor(first);
        var core = editor.TestNativeCore;
        var menu = (TextEditorContextFlyout)editor.GetContextFlyout();
        await CheckMenuChromeAsync(editor, menu, log);
        Check(!core.RightToLeftReadingOrder && core.FlowDirection == FlowDirection.LeftToRight,
            "An RTL application starts a new LTR document without mirroring its native canvas.");
        Invoke(menu.RightToLeftReadingOrder);
        Check(core.RightToLeftReadingOrder && core.IsDocumentEmpty && !editor.IsModified,
            "The actual context command sets RTL before the first character without dirtying the document.");
        editor.TypeText("مرحبا abc 123\rשלום XYZ");
        core.SetTextSelectionPosition(1, 4);
        var text = editor.GetText();
        var sequence = editor.DocumentSequence;
        var canUndo = core.CanUndo;
        Invoke(menu.RightToLeftReadingOrder);
        core.TestReadingOrderCommand(false);
        Check(!core.RightToLeftReadingOrder && core.TestFeatureNative.Bidirectional == Bidirectional.L2r,
            "The reading-order command changes an enabled document to LTR.");
        core.TestReadingOrderCommand(true);
        menu.TestFeatureRefresh();
        Check(core.RightToLeftReadingOrder && core.TestFeatureNative.Bidirectional == Bidirectional.R2l &&
            menu.RightToLeftReadingOrder.Icon.Visibility == Visibility.Visible,
            "Menu checked state follows the shared reading-order command.");
        Check(editor.GetText() == text && editor.DocumentSequence == sequence && core.CanUndo == canUndo && editor.IsModified,
            "Changing direction preserves text, journal sequence, undo availability and dirty state.");
        DocumentFeatureAssertions.Selection(editor, 1, 4, "Changing direction preserves the logical selection");
        core.Undo();
        Check(editor.IsDocumentEmpty && !editor.IsModified && core.RightToLeftReadingOrder,
            "Undo after a direction change still removes the preceding text edit and retains view direction.");
        core.Redo();
        Check(editor.GetText() == text, "Redo restores the original Unicode edit after direction changes.");
        core.SetTextSelectionPosition(1, 4);

        var second = await NewTabAsync(host.Page);
        Check(!Editor(second).TestNativeCore.RightToLeftReadingOrder && core.RightToLeftReadingOrder,
            "Reading order belongs to each tab and is not a global application setting.");
        core.IsEnabled = false;
        menu.TestFeatureRefresh();
        core.TestReadingOrderCommand(false);
        Check(!menu.RightToLeftReadingOrder.IsEnabled && core.RightToLeftReadingOrder,
            "A loading or disabled editor does not accept reading-order commands.");
        core.IsEnabled = true;
        host.Core.SwitchTo(first);
        log.AppendLine("PASS: reading-order commands preserve editing state and remain independent of tabs and RTL app chrome.");

        await RestoreCapturedDirectionAsync(host, editor, log);
        await CheckDiffAsync(editor, log);
        await CheckPrintingAsync(host, editor, Editor(second), log);
        await CheckReloadAsync(host, log);
        CheckTextStoreIndex(core, log);
        await CheckSavedSelectionAsync(host, log);
        await CheckDiffViewportAsync(log);
        await CheckDiffNavigationPanAsync(log);
        await DiffSourceViewportTests.RunAsync(host, log);
    }

    private static async Task CheckMenuChromeAsync(TextEditor editor, TextEditorContextFlyout menu, StringBuilder log)
    {
        var core = editor.TestNativeCore;
        var root = (FrameworkElement)Window.Current.Content;
        var originalFlow = root.FlowDirection;
        var originalReadingOrder = core.RightToLeftReadingOrder;
        var nativeView = (FrameworkElement)((Grid)core.Content).Children[0];
        IEnumerable<DependencyObject> OpenPopupElements()
        {
            var popups = OperatingSystem.IsWindowsVersionAtLeast(10, 0, 18362)
                ? VisualTreeHelper.GetOpenPopupsForXamlRoot(nativeView.XamlRoot)
                : VisualTreeHelper.GetOpenPopups(Window.Current);
            return popups.SelectMany(popup => Descendants(popup.Child));
        }
        MenuFlyoutPresenter OpenPresenter() => OpenPopupElements().OfType<MenuFlyoutPresenter>().FirstOrDefault();
        try
        {
            await UntilAsync(() => nativeView.IsLoaded && nativeView.ActualWidth > 0 && nativeView.ActualHeight > 0,
                "The editor context menu target is loaded and arranged.");
            foreach (var flow in new[] { FlowDirection.LeftToRight, FlowDirection.RightToLeft, FlowDirection.LeftToRight })
            {
                root.FlowDirection = flow;
                root.UpdateLayout();
                Check(editor.FlowDirection == flow,
                    "The owning editor inherits the changed application chrome direction.");
                foreach (var readingOrder in new[] { false, true })
                {
                    core.RightToLeftReadingOrder = readingOrder;
                    var opened = false;
                    var closed = false;
                    void OnOpened(object sender, object args) => opened = true;
                    void OnClosed(object sender, object args) => closed = true;
                    menu.Opened += OnOpened;
                    menu.Closed += OnClosed;
                    try
                    {
                        menu.ShowAt(nativeView);
                        await UntilAsync(() => opened && OpenPresenter() != null,
                            "The actual editor context menu opens its presenter.");
                        Check(OpenPresenter().FlowDirection == flow,
                            $"Editor menu chrome follows app {flow} with document RTL={readingOrder}; " +
                            $"actual presenter={OpenPresenter().FlowDirection}.");
                        Check(menu.Copy.FlowDirection == flow,
                            "The actual menu items inherit their presenter's chrome direction.");
                        Check(core.FlowDirection == FlowDirection.LeftToRight &&
                            core.RightToLeftReadingOrder == readingOrder,
                            "Opening the menu preserves the unmirrored native canvas and document direction.");
                    }
                    catch (TimeoutException error)
                    {
                        var popupTypes = string.Join(", ", OpenPopupElements()
                            .Select(element => element.GetType().Name).Distinct());
                        throw new InvalidOperationException($"Editor popup opened={opened}; visual types=[{popupTypes}].", error);
                    }
                    finally
                    {
                        try
                        {
                            menu.Hide();
                            if (opened)
                                await UntilAsync(() => closed,
                                    "The editor context menu completes Closed before reopening.");
                        }
                        finally
                        {
                            menu.Opened -= OnOpened;
                            menu.Closed -= OnClosed;
                        }
                    }
                }
            }
        }
        finally
        {
            root.FlowDirection = originalFlow;
            core.RightToLeftReadingOrder = originalReadingOrder;
        }
        log.AppendLine("PASS: actual editor popup direction follows application chrome independently of document reading order.");
    }

    private static void CheckTextStoreIndex(TextEditorCore core, StringBuilder log)
    {
        core.SetText("אב😀\rمرحباZ");
        var native = core.TestFeatureNative;
        var boundaries = new (long Acp, long Bytes)[]
        {
            (0, 0), (1, 2), (2, 4), (4, 8), (5, 9), (6, 11),
            (7, 13), (8, 15), (9, 17), (10, 19), (11, 20)
        };
        foreach (var (acp, bytes) in boundaries)
        {
            Check(native.PositionRelativeCodeUnits(0, acp) == bytes && native.CountCodeUnits(0, bytes) == acp,
                "Native UTF-16/UTF-8 boundaries round-trip across Hebrew, emoji and Arabic.");
        }
        Check(native.IndexPositionFromLine(1, LineCharacterIndexType.Utf16) == 5 &&
            native.LineFromIndexPosition(5, LineCharacterIndexType.Utf16) == 1,
            "The text-store line index counts UTF-16 units rather than UTF-8 bytes.");
        core.SetTextSelectionPosition(0, 2);
        core.TypeText("س");
        Check(core.GetText() == "س😀\rمرحباZ" && native.IndexPositionFromLine(1, LineCharacterIndexType.Utf16) == 4 &&
            native.CountCodeUnits(0, native.Length) == 10,
            "Replacing RTL text updates the native UTF-16 index without splitting the adjacent surrogate pair.");
        log.AppendLine("PASS: native UTF-16 indices stay exact across Hebrew, Arabic, emoji and replacement.");
    }

    private static async Task RestoreCapturedDirectionAsync(FeatureTestHost host, TextEditor source, StringBuilder log)
    {
        using var captured = source.CaptureRecoveryState();
        await captured.Checkpoint.FlushAsync();
        source.TestNativeCore.RightToLeftReadingOrder = false;
        var restored = (TextEditor)host.Core.CreateTextEditor(Guid.NewGuid(), null, "RTL recovery");
        try
        {
            await restored.RestoreV2Async(captured.SavedSnapshot, captured.RecoveryBaseline, captured.DocumentJournal,
                captured.Checkpoint, null, captured.MetaData, captured.TextDirty);
            Check(restored.TestNativeCore.RightToLeftReadingOrder && restored.GetText() == source.GetText() && restored.IsModified,
                "Recovery restores the captured RTL direction and unsaved bytes even after the source switches to LTR.");
            DocumentFeatureAssertions.Selection(restored, 1, 4, "Recovery preserves the captured selection");
            Check(restored.GetTextEditorStateMetaData().RightToLeftReadingOrder,
                "The recovered editor includes its direction in the next session/transfer capture.");
        }
        finally { await host.Core.DiscardUnattachedEditorAsync(restored); }
        source.TestNativeCore.RightToLeftReadingOrder = true;
        log.AppendLine("PASS: reading-order recovery captures are detached and restore direction with content and selection.");
    }

    private static async Task CheckSavedSelectionAsync(FeatureTestHost host, StringBuilder log)
    {
        var source = Editor(await NewTabAsync(host.Page));
        var text = "א😀ב\rא😀ב\r" + string.Join('\r', Enumerable.Range(0, 120).Select(line => $"saved row {line}"));
        source.TypeText(text);
        source.TestNativeCore.RightToLeftReadingOrder = true;
        using var captured = source.CaptureRecoveryState();
        await captured.Checkpoint.FlushAsync();
        var metadata = captured.MetaData.Clone();
        metadata.WrapWord = false;
        metadata.SelectionStartPosition = 4;
        metadata.SelectionEndPosition = 1;
        metadata.ScrollViewerHorizontalOffset = 0;
        metadata.ScrollViewerVerticalOffset = 0;
        metadata.FirstVisibleDocumentLine = 80;
        metadata.FirstVisibleSubLine = 0;
        var restored = (TextEditor)host.Core.CreateTextEditor(Guid.NewGuid(), null, "Saved selection");
        var root = Window.Current.Content;
        try
        {
            await restored.RestoreV2Async(captured.SavedSnapshot, captured.RecoveryBaseline, captured.DocumentJournal,
                captured.Checkpoint, null, metadata, captured.TextDirty);
            var core = restored.TestNativeCore;
            var native = core.TestFeatureNative;
            var view = (EditorBaseControl)((Grid)core.Content).Children[0];
            Check(!view.IsLoaded && view.IsViewportRestorePending && native.Anchor == 8 && native.CurrentPos == 2 &&
                core.GetSelectedText() == "😀ב", "Detached recovery preserves reversed UTF-16 selection across an emoji as exact UTF-8 caret/anchor bytes.");

            native.SelectionMode = WinUIEditor.SelectionMode.Rectangle;
            native.RectangularSelectionAnchor = 0;
            native.RectangularSelectionCaret = 8;
            var clamped = metadata.Clone();
            clamped.SelectionStartPosition = int.MaxValue;
            clamped.SelectionEndPosition = -1;
            restored.ResetEditorState(clamped);
            Check(native.Selections == 1 && !native.SelectionIsRectangle && native.SelectionMode == WinUIEditor.SelectionMode.Stream &&
                native.Anchor == Encoding.UTF8.GetByteCount(text) && native.CurrentPos == 0 && core.GetSelectedText() == text,
                "Session selection restoration clamps invalid endpoints and clears rectangular selection without losing direction.");
            restored.ResetEditorState(metadata);
            core.GetViewportAnchor(out var pendingLine, out var pendingSubLine);
            Check(pendingLine == 80 && pendingSubLine == 0 && native.Anchor == 8 && native.CurrentPos == 2,
                "Explicit saved viewport supersedes default follow while keeping the reversed emoji selection.");
            restored.Width = 600;
            restored.Height = 320;
            var paints = new List<bool>();
            void Painted(WinUIEditor.Editor sender, PaintedEventArgs args) => paints.Add(
                sender.FirstVisibleLine == 80 && sender.Anchor == 8 && sender.CurrentPos == 2);
            native.Painted += Painted;
            try
            {
                await EditorTestLayout.AfterPaintAsync(native, () => Window.Current.Content = restored);
                await UntilAsync(() => !view.IsViewportRestorePending, "Saved viewport restoration completes.");
                Check(paints.Count > 0 && paints.All(valid => valid),
                    "Every first paint honors the saved distant viewport rather than scrolling to the selected emoji.");
            }
            finally { native.Painted -= Painted; }
            // ResetEditorState restores a nonzero, reversed UTF-16 selection.
            // Its first arrow must acquire X from caret byte2, without a Home,
            // GotoPos or ChooseCaretX repairing the restored preferred column.
            native.LineDown();
            Check(native.CurrentPos == 11 && native.Anchor == 11,
                "The first Down after saved selection restore retains the exact column before the second emoji.");
            native.LineUp();
            Check(native.CurrentPos == 2 && native.Anchor == 2,
                "Up returns to the restored nonzero caret column.");
            core.RightToLeftReadingOrder = false;
            native.LineDown();
            Check(native.CurrentPos == 11 && native.Anchor == 11,
                "Changing base direction reinitializes the chosen column from the current nonzero caret.");
        }
        finally
        {
            Window.Current.Content = root;
            await host.Core.DiscardUnattachedEditorAsync(restored);
        }
        log.AppendLine("PASS: actual session restore keeps reversed emoji selections, clamps endpoints and applies the saved viewport before first paint.");
    }

    private static async Task CheckDiffViewportAsync(StringBuilder log)
    {
        var previous = Window.Current.Content;
        try
        {
            foreach (var newLength in new[] { 400, 650, 150, 3 })
            {
                var lines = Enumerable.Range(0, 35).Select(index => $"שלום {index:D2}").ToArray();
                lines[0] = new string('א', 400);
                var oldText = string.Join('\r', lines);
                lines[0] = new string('א', newLength);
                lines[1] = "ברכה חדשה";
                lines[9] = "שינוי נוסף";
                var newText = string.Join('\r', lines);
                using var baseline = await Notepads.Features.Documents.Storage.DocumentBaseline.FromTextAsync(oldText, Guid.NewGuid());
                using var source = new TextEditorCore { RightToLeftReadingOrder = true };
                source.SetText(newText);
                using var viewer = new SideBySideDiffViewer { Width = 760, Height = 360 };
                Check(await viewer.PrepareAsync(baseline, source,
                    Notepads.Features.Documents.FileTypes.DocumentLanguages.PlainText, "rtl.txt", System.Threading.CancellationToken.None),
                    "RTL diff viewport fixture prepares the actual viewer.");
                var old = Named<TextEditorCore>(viewer, "OldEditor");
                var current = Named<TextEditorCore>(viewer, "NewEditor");
                var oldView = (EditorBaseControl)((Grid)old.Content).Children[0];
                var newView = (EditorBaseControl)((Grid)current.Content).Children[0];
                var oldNative = oldView.Editor;
                var newNative = newView.Editor;
                async Task PaintBothAsync(Action action)
                {
                    var newPaint = EditorTestLayout.AfterPaintAsync(newNative, () => { });
                    await EditorTestLayout.AfterPaintAsync(oldNative, action);
                    await newPaint;
                }
                var scale = Windows.Graphics.Display.DisplayInformation.GetForCurrentView().RawPixelsPerViewPixel;
                bool GlyphVisible(EditorBaseControl view, int line)
                {
                    var native = view.Editor;
                    var position = native.PositionFromLine(line);
                    var first = native.PointXFromPosition(position);
                    var next = native.PointXFromPosition(native.PositionAfter(position));
                    // Template parts live in a separate XAML namescope. Inspect
                    // the arranged surface without throwing inside Painted.
                    var surface = Descendants(view).OfType<Border>().FirstOrDefault(element => element.Name == "ImageTarget");
                    if (surface == null || surface.ActualWidth <= 0) return false;
                    var surfaceWidth = surface.ActualWidth * scale;
                    var gutterWidth = native.GetMarginWidthN(0);
                    var rtl = native.Bidirectional == Bidirectional.R2l;
                    var left = native.MarginLeft + (rtl ? 0 : gutterWidth);
                    var right = surfaceWidth - native.MarginRight - (rtl ? gutterWidth : 0);
                    return Math.Min(first, next) < right && Math.Max(first, next) > left;
                }
                var firstPaints = new List<bool>();
                void OldPainted(WinUIEditor.Editor sender, PaintedEventArgs args) => firstPaints.Add(GlyphVisible(oldView, 1));
                void NewPainted(WinUIEditor.Editor sender, PaintedEventArgs args) => firstPaints.Add(GlyphVisible(newView, 1));
                oldNative.Painted += OldPainted;
                newNative.Painted += NewPainted;
                try
                {
                    await PaintBothAsync(() => Window.Current.Content = viewer);
                    Check(firstPaints.Count >= 2 && firstPaints.All(visible => visible),
                        "Both panes show the short RTL change from their first paint, despite different long-line widths.");
                }
                finally
                {
                    oldNative.Painted -= OldPainted;
                    newNative.Painted -= NewPainted;
                }
                await PaintBothAsync(() => Invoke(Named<Button>(viewer, "NextButton")));
                Check(GlyphVisible(oldView, 9) && GlyphVisible(newView, 9), "Next RTL hunk remains horizontally visible.");
                await PaintBothAsync(() => Invoke(Named<Button>(viewer, "PreviousButton")));
                Check(GlyphVisible(oldView, 1) && GlyphVisible(newView, 1), "Previous RTL hunk returns to both leading edges.");

                if (newLength == 650)
                {
                    var displayedOrigin = oldNative.XOffset;
                    old.SetDiffViewport(0, 500);
                    oldNative.XOffset = displayedOrigin - 120;
                    Check(Math.Abs(old.DiffHorizontalOffset - 120 / scale) < 1,
                        "A physical scroll before a pending request paints uses the previous displayed origin and DPI.");
                    await UntilAsync(() => !old.IsDiffViewportPending && !current.IsDiffViewportPending &&
                        Math.Abs(old.DiffHorizontalOffset - current.DiffHorizontalOffset) < 0.01,
                        "A physical scroll superseding a pending request synchronizes after projection.");
                }

                var events = 0;
                void ViewportChanged(object sender, EventArgs args) => events++;
                old.ViewportChanged += ViewportChanged;
                current.ViewportChanged += ViewportChanged;
                try
                {
                    const double requested = 2000;
                    old.SetDiffViewport(0, requested);
                    await UntilAsync(() => !old.IsDiffViewportPending && !current.IsDiffViewportPending &&
                        Math.Abs(old.DiffHorizontalOffset - requested) < 0.01 &&
                        Math.Abs(current.DiffHorizontalOffset - requested) < 0.01,
                        "Paired RTL panes synchronize a leading-edge offset, retaining intent when the short pane clamps.");
                    await Task.Delay(100); // Allow queued UpdateUI echoes to drain after both paints.
                    var settled = events;
                    await Task.Delay(100);
                    Check(events == settled && settled < 20,
                        "Delayed paired viewport notifications converge without an echo loop.");
                    if (newLength == 150)
                    {
                        Check(newNative.XOffset == 0 && oldNative.XOffset > 0,
                            "The shorter pane clamps physically without pulling the longer pane away from its requested position.");
                        await EditorTestLayout.AfterPaintAsync(oldNative, () => newNative.XOffset = 100);
                        await UntilAsync(() => !old.IsDiffViewportPending &&
                            Math.Abs(old.DiffHorizontalOffset - current.DiffHorizontalOffset) < 0.01,
                            "A real horizontal scroll after a clamped projection establishes a new shared offset.");
                        Check(current.DiffHorizontalOffset < requested,
                            "Real scrolling supersedes the retained, previously clamped request.");
                    }
                }
                finally
                {
                    old.ViewportChanged -= ViewportChanged;
                    current.ViewportChanged -= ViewportChanged;
                }

                if (newLength == 3)
                {
                    old.SetDiffViewport(34, 0);
                    await UntilAsync(() => !old.IsDiffViewportPending && !current.IsDiffViewportPending &&
                        old.DiffFirstVisibleRow == 34 && current.DiffFirstVisibleRow == 34,
                        "EOF synchronization retains the requested row despite each pane's scrollbar-dependent clamp.");
                    await Task.Delay(100);
                    Check(old.DiffFirstVisibleRow == 34 && current.DiffFirstVisibleRow == 34,
                        "A shorter pane's queued EOF update cannot pull back the other pane.");
                    var movedRow = Math.Max(0, newNative.FirstVisibleLine - 2);
                    newNative.FirstVisibleLine = movedRow;
                    await UntilAsync(() => !old.IsDiffViewportPending && old.DiffFirstVisibleRow == movedRow,
                        "Real vertical scrolling after an EOF clamp replaces the retained row intent.");
                }
                old.SetDiffViewport(old.DiffFirstVisibleRow, 0);
                await UntilAsync(() => !old.IsDiffViewportPending && !current.IsDiffViewportPending &&
                    old.DiffHorizontalOffset == 0 && current.DiffHorizontalOffset == 0,
                    "Both panes return to their leading edge before the resize/zoom fixture.");
                await PaintBothAsync(() => Invoke(Named<Button>(viewer, "PreviousButton")));
                await PaintBothAsync(() => viewer.Width = 900);
                await PaintBothAsync(() => old.SetFontZoomFactor(125));
                Check(GlyphVisible(oldView, 9) && GlyphVisible(newView, 9) &&
                    old.DiffHorizontalOffset == 0 && current.DiffHorizontalOffset == 0,
                    "Resizing and zooming preserve each RTL pane's leading edge without a stale-canvas jump.");
                if (newLength == 650)
                {
                    const double beforeZoomOffset = 120;
                    old.SetDiffViewport(0, beforeZoomOffset);
                    await UntilAsync(() => !old.IsDiffViewportPending && !current.IsDiffViewportPending &&
                        current.DiffFirstVisibleRow == 0 && current.DiffHorizontalOffset == beforeZoomOffset,
                        "Both panes show the long line at a nonzero leading offset before zooming down.");
                    var widthBeforeZoom = oldNative.ScrollWidth;
                    // A covered/hidden viewer can receive a zoom change before
                    // its next paint. Let the width timer run in that interval.
                    viewer.Opacity = 0;
                    await Task.Delay(100);
                    var paintsWhileHidden = 0;
                    void HiddenPainted(WinUIEditor.Editor sender, PaintedEventArgs args) => paintsWhileHidden++;
                    oldNative.Painted += HiddenPainted;
                    newNative.Painted += HiddenPainted;
                    try
                    {
                        old.SetFontZoomFactor(60);
                        await Task.Delay(200);
                        Check(paintsWhileHidden == 0, "The hidden diff defers painting while its width timer can run.");
                        Check(old.DiffHorizontalOffset == beforeZoomOffset && current.DiffHorizontalOffset == beforeZoomOffset,
                            "A metrics-driven width clamp before paint cannot replace the shared leading-edge intent.");
                    }
                    finally
                    {
                        oldNative.Painted -= HiddenPainted;
                        newNative.Painted -= HiddenPainted;
                    }
                    await PaintBothAsync(() => viewer.Opacity = 1);
                    Check(old.DiffHorizontalOffset == beforeZoomOffset && current.DiffHorizontalOffset == beforeZoomOffset &&
                        oldNative.ScrollWidth < widthBeforeZoom,
                        "The hidden zoom-down retains the shared offset and shrinks the measured extent on the next paint.");
                }
                Check(source.GetText() == newText && old.GetText() == oldText && current.GetText() == newText,
                    "Diff viewport operations preserve all three documents.");
                Window.Current.Content = previous;
            }
        }
        finally { Window.Current.Content = previous; }
        log.AppendLine("PASS: RTL diff first paint, navigation, unequal-width scroll sync, clamp recovery and resize/zoom.");
    }

    private static async Task CheckDiffNavigationPanAsync(StringBuilder log)
    {
        var previous = Window.Current.Content;
        try
        {
            foreach (var rtl in new[] { false, true })
            {
                var glyph = rtl ? 'א' : 'a';
                var lines = Enumerable.Range(0, 80).Select(index => $"row {index:D2}").ToArray();
                lines[0] = new string(glyph, 600);
                var oldText = string.Join('\r', lines);
                lines[0] = new string(glyph, 150);
                lines[20] = "changed 20";
                lines[50] = "changed 50";
                var newText = string.Join('\r', lines);
                using var baseline = await Notepads.Features.Documents.Storage.DocumentBaseline.FromTextAsync(oldText, Guid.NewGuid());
                using var source = new TextEditorCore { RightToLeftReadingOrder = rtl, FontSize = 24 };
                source.SetText(newText);
                using var viewer = new SideBySideDiffViewer { Width = 760, Height = 360 };
                Check(await viewer.PrepareAsync(baseline, source,
                    Notepads.Features.Documents.FileTypes.DocumentLanguages.PlainText, "pan.txt", System.Threading.CancellationToken.None),
                    "The horizontal navigation fixture prepares three distinct changes.");
                var old = Named<TextEditorCore>(viewer, "OldEditor");
                var current = Named<TextEditorCore>(viewer, "NewEditor");
                var oldNative = old.TestFeatureNative;
                var newNative = current.TestFeatureNative;
                async Task PaintBothAsync(Action action)
                {
                    var newPaint = EditorTestLayout.AfterPaintAsync(newNative, () => { });
                    await EditorTestLayout.AfterPaintAsync(oldNative, action);
                    await newPaint;
                }
                await PaintBothAsync(() => Window.Current.Content = viewer);
                Check(old.DiffHorizontalOffset == 0 && current.DiffHorizontalOffset == 0,
                    "Initial diff navigation starts at the reading-order leading edge.");
                const double requested = 2000;
                old.SetDiffViewport(0, requested);
                await UntilAsync(() => !old.IsDiffViewportPending && !current.IsDiffViewportPending &&
                    old.DiffHorizontalOffset == requested && current.DiffHorizontalOffset == requested,
                    "Both panes retain the long pane's requested horizontal pan.");
                Check(oldNative.XOffset != newNative.XOffset && (rtl ? newNative.XOffset == 0 : oldNative.XOffset > newNative.XOffset),
                    "The shorter pane clamps physically while retaining the shared horizontal intent.");

                void CheckNavigation(string button, long row, double offset)
                {
                    Check(!old.IsDiffViewportPending && !current.IsDiffViewportPending &&
                        old.DiffFirstVisibleRow == row && current.DiffFirstVisibleRow == row &&
                        Math.Abs(old.DiffHorizontalOffset - offset) < 0.01 &&
                        Math.Abs(current.DiffHorizontalOffset - offset) < 0.01,
                        $"RTL={rtl}: {button} reaches row {row} without changing the shared leading pan {offset}.");
                }
                async Task NavigateAsync(string button, long row, double offset)
                {
                    await PaintBothAsync(() => Invoke(Named<Button>(viewer, button)));
                    CheckNavigation(button, row, offset);
                }
                await NavigateAsync("NextButton", 18, requested);
                await NavigateAsync("NextButton", 48, requested);
                await NavigateAsync("NextButton", 0, requested);
                await NavigateAsync("PreviousButton", 48, requested);
                await NavigateAsync("PreviousButton", 18, requested);

                newNative.XOffset = 100;
                await UntilAsync(() => !old.IsDiffViewportPending && !current.IsDiffViewportPending &&
                    Math.Abs(current.DiffHorizontalOffset - requested) > 1 &&
                    Math.Abs(old.DiffHorizontalOffset - current.DiffHorizontalOffset) < 0.01,
                    "A real scroll in the shorter pane replaces the shared horizontal intent.");
                var updated = current.DiffHorizontalOffset;
                await NavigateAsync("PreviousButton", 0, updated);

                var scale = Windows.Graphics.Display.DisplayInformation.GetForCurrentView().RawPixelsPerViewPixel;
                foreach (var (pane, row) in new[] { (old, 18L), (current, 48L) })
                {
                    var native = pane.TestFeatureNative;
                    var expected = pane.DiffHorizontalOffset + 40 / scale;
                    await PaintBothAsync(() =>
                    {
                        native.XOffset += rtl ? -40 : 40;
                        Check(Math.Abs(pane.DiffHorizontalOffset - expected) < 0.01,
                            "A physical scroll updates the pane's leading offset before its UpdateUI notification.");
                        // Do not yield: the peer has not received the delayed
                        // scroll notification when the user navigates next.
                        Invoke(Named<Button>(viewer, "NextButton"));
                    });
                    CheckNavigation("NextButton immediately after a physical scroll", row, expected);
                }
                Check(source.GetText() == newText && old.GetText() == oldText && current.GetText() == newText,
                    "Horizontal panning and change navigation preserve all source and snapshot bytes.");
                Window.Current.Content = previous;
            }
        }
        finally { Window.Current.Content = previous; }
        log.AppendLine("PASS: LTR/RTL diff navigation preserves horizontal pan through clamping, wraparound and scrolling from either pane.");
    }

    private static async Task CheckDiffAsync(TextEditor editor, StringBuilder log)
    {
        var text = editor.GetText();
        var sequence = editor.DocumentSequence;
        await editor.OpenSideBySideDiffViewerAsync();
        try
        {
            var viewer = Named<ContentPresenter>(editor, "DiffPresenter").Content as SideBySideDiffViewer;
            Check(viewer != null, "A changed RTL document opens its actual diff viewer.");
            var oldPane = Named<TextEditorCore>(viewer, "OldEditor");
            var newPane = Named<TextEditorCore>(viewer, "NewEditor");
            Check(oldPane.RightToLeftReadingOrder && newPane.RightToLeftReadingOrder,
                "Both diff snapshots inherit the document's reading order.");
            oldPane.TestReadingOrderCommand(false);
            newPane.TestReadingOrderCommand(false);
            Check(oldPane.RightToLeftReadingOrder && newPane.RightToLeftReadingOrder,
                "Reading-order commands refuse independent changes to either diff snapshot.");
        }
        finally { editor.CloseSideBySideDiffViewer(); }
        Check(editor.TestNativeCore.RightToLeftReadingOrder && editor.GetText() == text && editor.DocumentSequence == sequence,
            "Closing diff preserves source reading order, document bytes and journal sequence.");
        log.AppendLine("PASS: RTL diff shares the source direction and preserves source editing state on return.");
    }

    private static async Task CheckPrintingAsync(FeatureTestHost host, TextEditor rtl, TextEditor ltr, StringBuilder log)
    {
        ltr.TypeText("LTR print text");
        await host.Page.PrintAllAsync(new[] { host.Core.GetTab(rtl), host.Core.GetTab(ltr) });
        var pages = host.Page.TestPrintCanvas.Children.Cast<PrintPageFormat>().ToArray();
        Check(pages.Length == 2, "Print All prepares both nonempty documents.");
        for (var index = 0; index < pages.Length; index++)
        {
            var expected = index == 0 ? FlowDirection.RightToLeft : FlowDirection.LeftToRight;
            var content = Named<RichTextBlock>(pages[index], "TextContent");
            var overflow = Named<RichTextBlockOverflow>(pages[index], "ContinuationPageLinkedContainer");
            Check(content.FlowDirection == expected && overflow.FlowDirection == expected &&
                content.TextReadingOrder == TextReadingOrder.UseFlowDirection &&
                content.TextAlignment == (index == 0 ? TextAlignment.Right : TextAlignment.Left),
                "Print content and its first overflow use document direction independently of the app.");
            var continuation = new ContinuationPageFormat(overflow, new FontFamily("Consolas"), 14, "Header", "Footer");
            Check(Named<RichTextBlockOverflow>(continuation, "ContinuationPageLinkedContainer").FlowDirection == expected,
                "Continuation print pages retain the linked document's direction.");
        }
        log.AppendLine("PASS: Print All and continuation content retain independent LTR/RTL document directions.");
    }

    private static async Task CheckReloadAsync(FeatureTestHost host, StringBuilder log)
    {
        var editor = await DocumentFeatureAssertions.OpenAsync(host, "rtl-reload.txt", Encoding.UTF8.GetBytes("שלום saved"));
        editor.TestNativeCore.RightToLeftReadingOrder = true;
        await editor.ReloadFromEditingFileAsync();
        Check(editor.TestNativeCore.RightToLeftReadingOrder && editor.GetText() == "שלום saved" && !editor.IsModified,
            "Reload preserves the document's reading order without adding a text modification.");
        await editor.TestStopFileChecksAsync();
        log.AppendLine("PASS: reloading a file retains its per-document reading direction.");
    }
}
