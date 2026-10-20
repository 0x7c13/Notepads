// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents.Storage;
using Notepads.Presentation.Controls.TextEditor;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Automation.Provider;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Controls.Primitives;
using Windows.UI.Xaml.Media;

namespace NotepadsEditorTests;

internal static class ScrollRestorationTests
{
    private const int LineCount = 2000;

    internal static async Task RunAsync(StringBuilder log)
    {
        await SmoothScrollingTests.RunAsync(log);
        await LineLayoutCacheTests.RunAsync(log);
        await CheckDetachedDocumentLoadAsync(log);
        await CheckLeadingRtlLoadAsync(log);
        await CheckLoadedReplacementAsync(log);
        await CheckRtlHistoricalCanvasAsync(log, panned: false);
        await CheckRtlHistoricalCanvasAsync(log, panned: true);
        foreach (var wrapping in new[] { TextWrapping.NoWrap, TextWrapping.Wrap })
        {
            await CheckFirstPaintAsync(log, wrapping);
            await CheckFirstPaintAsync(log, wrapping, rightToLeft: true);
        }
        await CheckFirstPaintAsync(log, TextWrapping.NoWrap, atEnd: true);
        await CheckFirstPaintAsync(log, TextWrapping.NoWrap, shortTarget: true);
        await CheckFirstPaintAsync(log, TextWrapping.Wrap, atEnd: true, singleLine: true);
        await CheckWrapTransitionAsync(log);
        await CheckLongPrecedingLinesAsync(log);
        await CheckLongPrecedingLinesAsync(log, atEnd: true);
        foreach (var wrapping in new[] { TextWrapping.Wrap, TextWrapping.NoWrap })
            await CheckV1CaretAsync(log, wrapping);
        foreach (var wrapping in new[] { TextWrapping.NoWrap, TextWrapping.Wrap })
        {
            await CheckLogAppendAsync(log, wrapping, pending: true);
            await CheckLogAppendAsync(log, wrapping, pending: false);
        }
        await CheckBlockedLogAppendAsync(log, readOnly: true);
        await CheckBlockedLogAppendAsync(log, readOnly: false);
    }

    private static WinUIEditor.EditorBaseControl View(TextEditorCore core) =>
        (WinUIEditor.EditorBaseControl)((Grid)core.Content).Children[0];

    private static TextEditorCore Create(TextWrapping wrapping) => new()
    {
        Width = 600, Height = 320, TextWrapping = wrapping,
        HorizontalAlignment = HorizontalAlignment.Left, VerticalAlignment = VerticalAlignment.Top,
        Padding = new Thickness(0, 4, 0, 0)
    };

    private static (long Line, long SubLine, long X) Position(WinUIEditor.Editor native)
    {
        var row = native.FirstVisibleLine;
        var line = native.DocLineFromVisible(row);
        return (line, row - native.VisibleFromDocLine(line), native.XOffset);
    }

    private static Task WaitForWrapAsync(WinUIEditor.Editor native) => UntilFrameAsync(
        () => native.VisibleFromDocLine(native.LineCount) == native.LineCount * native.WrapCount(0));

    private static async Task CheckDetachedDocumentLoadAsync(StringBuilder log)
    {
        foreach (var rtl in new[] { false, true })
        {
            var text = new string(rtl ? 'א' : 'x', 1200) + '\r' +
                string.Join('\r', Enumerable.Repeat(new string(rtl ? 'א' : 'x', 9), 120));
            var owner = Guid.NewGuid();
            using var baseline = await DocumentBaseline.FromTextAsync(text, owner);
            using var journal = await DocumentJournal.CreateAsync(owner);
            using var source = Create(TextWrapping.NoWrap);
            await source.LoadBaselineAsync(baseline, preserveUndo: false);
            source.StartJournal(journal);
            try
            {
                View(source).Editor.SetEmptySelection(View(source).Editor.Length);
                source.TypeText("\rjournal 😀");
                using var checkpoint = source.AcquireJournalCheckpoint();
                await checkpoint.FlushAsync();
                foreach (var replay in new[] { false, true })
                {
                    using var core = Create(TextWrapping.NoWrap);
                    core.RightToLeftReadingOrder = rtl;
                    core.SetText("old\rselection");
                    var view = View(core);
                    var native = view.Editor;
                    if (replay)
                    {
                        native.SelectionMode = WinUIEditor.SelectionMode.Rectangle;
                        native.RectangularSelectionAnchor = 1;
                        native.RectangularSelectionCaret = 6;
                    }
                    else
                    {
                        native.SetSelection(2, 1);
                        native.AddSelection(5, 4);
                    }
                    if (replay) await core.RestoreBaselineAsync(baseline, checkpoint, CancellationToken.None);
                    else await core.LoadBaselineAsync(baseline, preserveUndo: false);
                    Check(!view.IsLoaded && view.IsViewportRestorePending,
                        "Detached baseline/journal load queues caret-follow until its first layout");
                    Check(native.Selections == 1 && !native.SelectionIsRectangle &&
                        native.SelectionMode == WinUIEditor.SelectionMode.Stream && native.CurrentPos == 0 && native.Anchor == 0,
                        "Document publication resets rectangle/multiple selections to one empty stream selection");
                    Check(core.GetText() == text + (replay ? "\rjournal 😀" : string.Empty),
                        "The baseline or committed journal supplies the exact document bytes");
                    var paints = new List<bool>();
                    void Painted(WinUIEditor.Editor sender, WinUIEditor.PaintedEventArgs args) =>
                        paints.Add(sender.CurrentPos == 0 && sender.Anchor == 0 && CaretVisible(core));
                    native.Painted += Painted;
                    try
                    {
                        await EditorTestLayout.AfterPaintAsync(native, () => Window.Current.Content = core);
                        await UntilFrameAsync(() => !view.IsViewportRestorePending);
                        Check(paints.Count > 0 && paints.All(visible => visible),
                            $"Detached {(replay ? "journal" : "baseline")} {(rtl ? "RTL" : "LTR")} load shows caret zero from its first paint");
                        native.Painted -= Painted;
                        // A restored caret has no prior user-chosen X. The
                        // first arrow must use its actual document origin.
                        native.LineDown();
                        var secondRow = rtl ? 2401 : 1201;
                        Check(native.CurrentPos == secondRow && native.Anchor == secondRow,
                            "The first Down after baseline/journal restore retains logical column zero");
                        native.LineUp();
                        Check(native.CurrentPos == 0 && native.Anchor == 0,
                            "Up returns to the restored document origin without an explicit column reset");
                    }
                    finally
                    {
                        native.Painted -= Painted;
                        Window.Current.Content = null;
                    }
                }
            }
            finally { await source.StopJournalAsync(); }
        }

        log.AppendLine("PASS: detached baseline/journal loads reset selections, paint restored carets and retain the first vertical column.");
    }

    private static async Task CheckLeadingRtlLoadAsync(StringBuilder log)
    {
        using var core = Create(TextWrapping.NoWrap);
        await core.LoadTextAsync(new string('א', 1600) + " ABC");
        var view = View(core);
        var native = view.Editor;
        Check(!core.RightToLeftReadingOrder && !view.IsLoaded && view.IsViewportRestorePending,
            "A detached LTR-base document with a long RTL prefix needs initial caret-follow");
        var paints = new List<bool>();
        void Painted(WinUIEditor.Editor sender, WinUIEditor.PaintedEventArgs args) => paints.Add(
            sender.CurrentPos == 0 && sender.Anchor == 0 && sender.XOffset > 0 && CaretVisible(core));
        native.Painted += Painted;
        try
        {
            await EditorTestLayout.AfterPaintAsync(native, () => Window.Current.Content = core);
            await UntilFrameAsync(() => !view.IsViewportRestorePending);
            Check(paints.Count > 0 && paints.All(valid => valid) && native.PointXFromPosition(0) > native.PointXFromPosition(2),
                "LTR base direction must still follow the nonzero visual origin of a leading RTL run from its first paint");
        }
        finally
        {
            native.Painted -= Painted;
            Window.Current.Content = null;
        }
        log.AppendLine("PASS: an LTR document with a long RTL prefix follows its real leading caret before first paint.");
    }

    private static async Task CheckLoadedReplacementAsync(StringBuilder log)
    {
        using var core = Create(TextWrapping.NoWrap);
        core.RightToLeftReadingOrder = true;
        var before = new string('א', 1200) + '\r' + string.Join('\r', Enumerable.Repeat("old row", 120));
        var replacement = new string('ב', 1200) + '\r' + string.Join('\r', Enumerable.Repeat(new string('ב', 9), 120));
        await core.LoadTextAsync(before);
        var native = View(core).Editor;
        try
        {
            await EditorTestLayout.AfterPaintAsync(native, () => Window.Current.Content = core);
            await UntilFrameAsync(() => !View(core).IsViewportRestorePending);
            native.SetEmptySelection(native.Length);
            core.TypeText(" unsaved 😀");
            native.FirstVisibleLine = 80;
            native.XOffset = 100;
            Check(native.FirstVisibleLine == 80, "Reload starts away from document zero");
            using var baseline = await DocumentBaseline.FromTextAsync(replacement, Guid.NewGuid());
            await core.LoadBaselineAsync(baseline, preserveUndo: true);
            // Inspect immediately: a future paint must not be needed to follow the new caret.
            Check(!View(core).IsViewportRestorePending && native.CurrentPos == 0 && native.Anchor == 0 && CaretVisible(core),
                "A loaded replacement immediately shows its reset RTL caret after final margin/profile updates");
            Check(core.GetText() == replacement && core.CanUndo, "Undo-preserving load publishes the replacement");
            native.LineDown();
            Check(native.CurrentPos == 2401 && native.Anchor == 2401,
                "The first Down after loaded replacement uses the new caret instead of the old document's chosen column");
            core.Undo();
            Check(core.GetText() == before + " unsaved 😀", "One undo restores the complete pre-reload edited document");
            core.Redo();
            Check(core.GetText() == replacement, "Redo restores the exact replacement document");
        }
        finally { Window.Current.Content = null; }
        log.AppendLine("PASS: loaded RTL replacement follows caret zero immediately and retains exact reload undo/redo.");
    }

    private static double RightTextEdge(TextEditorCore core)
    {
        var native = View(core).Editor;
        var surface = FindNamed<Border>(View(core), "ImageTarget");
        var scale = Windows.Graphics.Display.DisplayInformation.GetForCurrentView().RawPixelsPerViewPixel;
        double gutter = 0;
        for (var margin = 0; margin < native.Margins; margin++) gutter += native.GetMarginWidthN(margin);
        return surface.ActualWidth * scale - gutter - native.MarginRight;
    }

    private static async Task CheckRtlHistoricalCanvasAsync(StringBuilder log, bool panned)
    {
        const int targetLine = 80;
        const int caretWidth = 2;
        var scale = Windows.Graphics.Display.DisplayInformation.GetForCurrentView().RawPixelsPerViewPixel;
        var target = panned ? new string('א', 120) + " ABC" : "אבג ABC";
        var first = new string('א', 1200);
        var text = first + '\r' + string.Join('\r', Enumerable.Repeat(target, 130));
        var targetByte = Encoding.UTF8.GetByteCount(first + '\r') +
            (targetLine - 1) * Encoding.UTF8.GetByteCount(target + '\r');
        var selectedUtf16 = panned ? 1 : first.Length + 1 + (targetLine - 1) * (target.Length + 1) + 1;
        var selectedByte = Encoding.UTF8.GetByteCount(text.AsSpan(0, selectedUtf16));
        var samples = new[] { targetByte + (panned ? 32 : 2), targetByte + (panned ? 40 : 4) };
        var expectedGeometry = new (double FromRight, long Y)[samples.Length];
        var expectedLeading = panned ? 60 : 0;
        var scenario = panned ? "RTL panned viewport, narrower restore and offscreen selection" :
            "RTL right-edge viewport after a previously visited long line";
        double horizontal, vertical;
        int? documentLine;
        int subLine;
        long historicalX;
        using (var original = Create(TextWrapping.NoWrap))
        {
            original.DisplayLineNumbers = true;
            original.RightToLeftReadingOrder = true;
            await original.LoadTextAsync(text);
            var native = View(original).Editor;
            native.CaretWidth = caretWidth;
            try
            {
                // The first paint measures the very long row. Later visible rows
                // retain that historical canvas even though their text is short.
                await EditorTestLayout.AfterPaintAsync(native, () => Window.Current.Content = original);
                await UntilFrameAsync(() => !View(original).IsViewportRestorePending);
                await EditorTestLayout.AfterPaintAsync(native, () =>
                {
                    native.SetEmptySelection(targetByte);
                    native.ScrollCaret();
                    native.FirstVisibleLine = targetLine;
                });
                await EditorTestLayout.AfterPaintAsync(native, () =>
                {
                    var desired = RightTextEdge(original) - caretWidth + expectedLeading * scale;
                    native.XOffset += checked((int)Math.Round(native.PointXFromPosition(targetByte) - desired));
                    native.SetSelection(targetByte, targetByte);
                });
                Check(Math.Abs(native.PointXFromPosition(targetByte) -
                    (RightTextEdge(original) - caretWidth + expectedLeading * scale)) <= 1,
                    scenario + ": source leading edge is positioned independently of the caret-scroll policy");
                historicalX = native.XOffset;
                Check(historicalX > original.Width * scale * 4 && native.FirstVisibleLine == targetLine,
                    scenario + ": the saved viewport retains the offscreen long line's much wider canvas");
                for (var i = 0; i < samples.Length; i++)
                {
                    var x = native.PointXFromPosition(samples[i]);
                    Check(x >= native.MarginLeft && x < RightTextEdge(original),
                        scenario + ": source sample text is visible inside the gutter boundary");
                    expectedGeometry[i] = ((RightTextEdge(original) - x) / scale,
                        native.PointYFromPosition(samples[i]));
                }
                // Selection metadata must not pull an explicitly saved viewport
                // toward its caret, including a caret outside the visible rows.
                original.RestoreTextSelectionPosition(selectedUtf16, selectedUtf16);
                original.GetScrollViewerPosition(out horizontal, out vertical);
                original.GetViewportAnchor(out documentLine, out subLine);
                Check(documentLine == targetLine && subLine == 0 && native.CurrentPos == selectedByte,
                    scenario + ": exact document anchor and independent selection are captured");
            }
            finally { Window.Current.Content = null; }
        }

        using var restored = Create(TextWrapping.NoWrap);
        restored.Width = panned ? 480 : 600;
        restored.DisplayLineNumbers = true;
        await restored.LoadTextAsync(text);
        restored.RightToLeftReadingOrder = true;
        restored.RestoreTextSelectionPosition(selectedUtf16, selectedUtf16);
        restored.SetScrollViewerInitPosition(horizontal, vertical, documentLine, subLine);
        restored.GetScrollViewerPosition(out var pendingX, out var pendingY);
        Check(pendingX == horizontal && pendingY == vertical,
            scenario + ": a never-presented editor retains its pending session coordinates");
        var editor = View(restored).Editor;
        editor.CaretWidth = caretWidth;
        var observations = new List<(long Line, long X, double FirstFromRight, bool Valid)>();
        void Painted(WinUIEditor.Editor sender, WinUIEditor.PaintedEventArgs args)
        {
            var right = RightTextEdge(restored);
            var valid = sender.FirstVisibleLine == targetLine && sender.CurrentPos == selectedByte &&
                sender.Anchor == selectedByte && sender.Selections == 1 &&
                sender.Bidirectional == WinUIEditor.Bidirectional.R2l;
            for (var i = 0; i < samples.Length; i++)
            {
                var x = sender.PointXFromPosition(samples[i]);
                valid &= x >= sender.MarginLeft && x < right &&
                    Math.Abs((right - x) / scale - expectedGeometry[i].FromRight) <= 1 / scale &&
                    sender.PointYFromPosition(samples[i]) == expectedGeometry[i].Y;
            }
            observations.Add((sender.FirstVisibleLine, sender.XOffset,
                (right - sender.PointXFromPosition(samples[0])) / scale, valid));
        }
        editor.Painted += Painted;
        try
        {
            await EditorTestLayout.AfterPaintAsync(editor, () => Window.Current.Content = restored);
            await UntilFrameAsync(() => !View(restored).IsViewportRestorePending);
            if (panned)
            {
                var provider = FrameworkElementAutomationPeer.CreatePeerForElement(View(restored))
                    ?.GetPattern(PatternInterface.Text) as ITextProvider2;
                Check(provider != null, scenario + ": the restored editor exposes its accessibility text provider");
                provider.GetCaretRange(out _).GetBoundingRectangles(out var bounds);
                Check(bounds is not { Length: > 0 },
                    scenario + ": accessibility clips the offscreen caret without measuring the old long line");
            }
            // Include the delayed widen tick: it must not replay the obsolete
            // historical extent after the first correctly positioned paint.
            await Task.Delay(100);
            await EditorTestLayout.AfterPaintAsync(editor, () => editor.SetSelection(selectedByte, selectedByte));
            editor.Painted -= Painted;
            Check(observations.Count > 0 && observations.All(item => item.Valid),
                $"{scenario}: every restored paint must retain visible text geometry; " +
                $"expected right distance {expectedGeometry[0].FromRight:F2}, observed {string.Join(", ", observations.Take(8))}");
            restored.GetScrollViewerPosition(out var restoredX, out _);
            Check(Math.Abs(horizontal - expectedLeading) <= 1 / scale &&
                Math.Abs(restoredX - horizontal) <= 1 / scale,
                scenario + ": session offsets describe distance from the reading-order leading edge");
            Check(editor.XOffset < historicalX / 2 && restored.GetText() == text,
                scenario + ": a fresh lazy canvas does not need the old offscreen extent or alter document text");
        }
        finally
        {
            editor.Painted -= Painted;
            Window.Current.Content = null;
        }
        log.AppendLine($"PASS: {scenario} restores visible text from its first paint without measuring historical offscreen lines.");
    }

    private static async Task CheckFirstPaintAsync(StringBuilder log, TextWrapping wrapping,
        bool atEnd = false, bool shortTarget = false, bool singleLine = false, bool rightToLeft = false)
    {
        var scenario = $"{(rightToLeft ? "RTL " : string.Empty)}{wrapping}, {(atEnd ? "EOF" : shortTarget ? "short visible lines" : "middle")} document anchor";
        var character = rightToLeft ? 'א' : 'x';
        var text = singleLine ? new string(character, 4000) : string.Join('\r', Enumerable.Range(0, LineCount)
            .Select(line => new string(character, shortTarget && line > 0 ? 8 : 120)));
        double horizontal, vertical;
        int? documentLine;
        int subLine;
        (long Line, long SubLine, long X) expected;
        using (var original = Create(wrapping))
        {
            original.RightToLeftReadingOrder = rightToLeft;
            await original.LoadTextAsync(text);
            original.SetFontZoomFactor(150);
            if (rightToLeft) original.SetTextSelectionPosition(100, 100);
            var native = View(original).Editor;
            await EditorTestLayout.AfterPaintAsync(native, () =>
            {
                Window.Current.Content = original;
                Window.Current.Activate();
            });
            await WaitForWrapAsync(native);
            if (wrapping == TextWrapping.NoWrap)
            {
                var bar = FindNamed<ScrollBar>(View(original), "HorizontalScrollBar");
                var surface = FindNamed<Border>(View(original), "ImageTarget");
                await UntilFrameAsync(() => bar.Visibility == Visibility.Visible && bar.ActualHeight > 0 &&
                    surface.ActualHeight + bar.ActualHeight <= View(original).ActualHeight + 0.5);
            }
            await EditorTestLayout.AfterPaintAsync(native, () =>
            {
                native.FirstVisibleLine = atEnd ? native.VisibleFromDocLine(native.LineCount) :
                    native.VisibleFromDocLine(800) + (wrapping == TextWrapping.Wrap ? 1 : 0);
                if (wrapping == TextWrapping.NoWrap) native.XOffset = 120;
            });
            original.GetScrollViewerPosition(out horizontal, out vertical);
            original.GetViewportAnchor(out documentLine, out subLine);
            expected = Position(native);
            Check(singleLine ? expected.Line == 0 && expected.SubLine > 0 :
                (atEnd ? expected.Line > LineCount - 30 : expected.Line == 800) &&
                expected.SubLine == (wrapping == TextWrapping.Wrap ? 1 : 0),
                scenario + ": source viewport established");
            Window.Current.Content = null;
        }

        using var restored = Create(rightToLeft ? TextWrapping.NoWrap : wrapping);
        await restored.LoadTextAsync(text);
        // Match session restore: direction precedes wrap, font, selection and viewport.
        restored.RightToLeftReadingOrder = rightToLeft;
        restored.TextWrapping = wrapping;
        restored.SetFontZoomFactor(150); // Same order as production ResetEditorState.
        restored.SetTextSelectionPosition(100, 100); // Deliberately outside the saved viewport.
        restored.SetScrollViewerInitPosition(horizontal, vertical, documentLine, subLine);
        restored.GetScrollViewerPosition(out var pendingX, out var pendingY);
        restored.GetViewportAnchor(out var pendingLine, out var pendingSubLine);
        Check(pendingX == horizontal && pendingY == vertical && pendingLine == documentLine &&
            pendingSubLine == subLine, scenario + ": never-presented metadata preserved");
        var editor = View(restored).Editor;
        var expectedCaret = editor.PositionRelativeCodeUnits(0, 100);
        var expectedDirection = rightToLeft ? WinUIEditor.Bidirectional.R2l : WinUIEditor.Bidirectional.L2r;
        var observed = new List<(long Line, long SubLine, long X)>();
        var observedState = new List<bool>();
        void Painted(WinUIEditor.Editor sender, WinUIEditor.PaintedEventArgs args)
        {
            observed.Add(Position(sender));
            observedState.Add(sender.Bidirectional == expectedDirection &&
                sender.CurrentPos == expectedCaret && sender.Anchor == expectedCaret);
        }
        void Loaded(object sender, RoutedEventArgs args) => restored.Focus(FocusState.Programmatic);
        editor.Painted += Painted;
        restored.Loaded += Loaded;
        try
        {
            await EditorTestLayout.AfterPaintAsync(editor, () => Window.Current.Content = restored);
            await WaitForWrapAsync(editor);
            await UntilFrameAsync(() => !View(restored).IsViewportRestorePending);
            if (shortTarget)
            {
                // Cross the native 50 ms widen tick, then force another paint.
                // The assertion concerns a delayed timer, not readiness by sleep.
                await Task.Delay(100);
                await EditorTestLayout.AfterPaintAsync(editor, () => restored.Width += 1);
            }
            Check(observed.Count > 0 && observed.All(position => position == expected),
                $"{scenario}: every paint must show {expected}; observed {string.Join(", ", observed.Take(12))}");
            Check(observedState.All(valid => valid), scenario + ": direction and logical selection preserved from first paint");

            // Once restored, later tab switches must use the user's new viewport.
            editor.Painted -= Painted;
            await EditorTestLayout.AfterPaintAsync(editor, () => editor.FirstVisibleLine -= 5);
            var moved = Position(editor);
            Window.Current.Content = new Grid();
            await UntilFrameAsync(() => !restored.IsLoaded);
            restored.GetViewportAnchor(out var unloadedLine, out var unloadedSubLine);
            Check(unloadedLine == moved.Line && unloadedSubLine == moved.SubLine, scenario + ": unloaded metadata is current");
            observed.Clear();
            observedState.Clear();
            editor.Painted += Painted;
            await EditorTestLayout.AfterPaintAsync(editor, () => Window.Current.Content = restored);
            await UntilFrameAsync(() => restored.IsLoaded);
            Check(observed.Count > 0 && observed.All(position => position == moved), scenario + ": tab reattach does not replay restore");
            Check(observedState.All(valid => valid), scenario + ": tab reattach preserves direction and selection");
        }
        finally
        {
            editor.Painted -= Painted;
            restored.Loaded -= Loaded;
            Window.Current.Content = null;
        }
        log.AppendLine($"PASS: {scenario} restores before first paint and survives focus, idle wrapping and tab reattachment.");
    }

    private static async Task CheckWrapTransitionAsync(StringBuilder log)
    {
        using var core = Create(TextWrapping.Wrap);
        await core.LoadTextAsync(string.Join('\r', Enumerable.Repeat(new string('x', 120), LineCount)));
        var native = View(core).Editor;
        try
        {
            await EditorTestLayout.AfterPaintAsync(native, () => Window.Current.Content = core);
            await WaitForWrapAsync(native);
            var positions = new List<(long Line, long SubLine, long X)>();
            void Painted(WinUIEditor.Editor sender, WinUIEditor.PaintedEventArgs args) => positions.Add(Position(sender));
            native.Painted += Painted;
            try
            {
                await EditorTestLayout.AfterPaintAsync(native, () =>
                {
                    core.TextWrapping = TextWrapping.NoWrap;
                    core.SetScrollViewerInitPosition(0, 0, 800);
                });
                await UntilFrameAsync(() => !View(core).IsViewportRestorePending);
                Check(positions.Count > 0 && positions.All(position => position == (800, 0, 0)),
                    "disabling wrapping restores against unwrapped document lines");
            }
            finally { native.Painted -= Painted; }
        }
        finally { Window.Current.Content = null; }
        log.AppendLine("PASS: restoring while disabling Word Wrap discards obsolete wrapped row heights.");
    }

    private static async Task UntilFrameAsync(Func<bool> ready)
    {
        var completion = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
        void Rendering(object sender, object args)
        {
            if (ready()) completion.TrySetResult(true);
        }
        CompositionTarget.Rendering += Rendering;
        try { await completion.Task.WaitAsync(TimeSpan.FromSeconds(10)); }
        finally { CompositionTarget.Rendering -= Rendering; }
    }

    private static async Task CheckLongPrecedingLinesAsync(StringBuilder log, bool atEnd = false)
    {
        // The five lines before a distant anchor exceed the visible-wrap budget.
        // Both the anchor's subline and following rows must be ready at first paint.
        const int anchor = 105;
        using var core = Create(TextWrapping.Wrap);
        core.SetFontZoomFactor(150);
        var preceding = string.Join('\r', Enumerable.Repeat(new string('x', 20), 100)
            .Concat(Enumerable.Repeat(new string('x', 430_000), 5)));
        var tail = atEnd ? new string('x', 120) + '\r' + new string('x', 2000) :
            string.Join('\r', Enumerable.Repeat(new string('x', 120), 100));
        await core.LoadTextAsync(preceding + '\r' + tail);
        core.SetScrollViewerInitPosition(0, 0, anchor, 1);
        var native = View(core).Editor;
        var observed = new List<(long Line, long SubLine, long X)>();
        var followingRowsReady = new List<bool>();
        void Painted(WinUIEditor.Editor sender, WinUIEditor.PaintedEventArgs args)
        {
            observed.Add(Position(sender));
            followingRowsReady.Add(sender.VisibleFromDocLine(anchor + 2) - sender.VisibleFromDocLine(anchor + 1) ==
                sender.WrapCount(anchor + 1));
        }
        native.Painted += Painted;
        try
        {
            await EditorTestLayout.AfterPaintAsync(native, () => Window.Current.Content = core);
            // Equal-length source lines give an independent completion condition
            // for the prefix's idle wrapping, without a fixed delay.
            var prefixRows = 100 * native.WrapCount(0) + 5 * native.WrapCount(100);
            await UntilFrameAsync(() => native.VisibleFromDocLine(anchor) == prefixRows);
            Check(observed.Count > 0 && observed.All(position => position == (anchor, 1, 0)) && followingRowsReady.All(ready => ready),
                "long preceding lines must not discard the saved subline or paint incomplete following rows");
        }
        finally
        {
            native.Painted -= Painted;
            Window.Current.Content = null;
        }
        log.AppendLine($"PASS: a wrapped anchor survives long preceding lines{(atEnd ? " and an unresolved EOF tail" : "")}.");
    }

    private static async Task CheckV1CaretAsync(StringBuilder log, TextWrapping wrapping)
    {
        using var core = Create(wrapping);
        await core.LoadTextAsync(new string('x', 12000));
        core.SetFontZoomFactor(150);
        core.SetTextSelectionPosition(8000, 8000);
        core.SetScrollViewerInitPosition(0, 0); // V1 has no native viewport anchor.
        var native = View(core).Editor;
        var observed = new List<bool>();
        void Painted(WinUIEditor.Editor sender, WinUIEditor.PaintedEventArgs args)
        {
            var y = sender.PointYFromPosition(sender.CurrentPos);
            var x = sender.PointXFromPosition(sender.CurrentPos);
            var surface = FindNamed<Border>(View(core), "ImageTarget");
            var scale = Windows.Graphics.Display.DisplayInformation.GetForCurrentView().RawPixelsPerViewPixel;
            observed.Add(y >= 0 && y < sender.LinesOnScreen * sender.TextHeight(0) &&
                x >= 0 && x < surface.ActualWidth * scale);
        }
        native.Painted += Painted;
        try
        {
            await EditorTestLayout.AfterPaintAsync(native, () => Window.Current.Content = core);
            await UntilFrameAsync(() => !View(core).IsViewportRestorePending);
            Check(native.CurrentPos == 8000 && native.Anchor == 8000 && observed.Count > 0 && observed.All(visible => visible),
                $"V1 import shows a saved caret inside a long {wrapping} paragraph from the first paint");
        }
        finally
        {
            native.Painted -= Painted;
            Window.Current.Content = null;
        }
        log.AppendLine($"PASS: V1 import preserves and shows the saved caret within a long {wrapping} paragraph.");
    }

    private static string LongLogText() => ".LOG\r" +
        string.Join('\r', Enumerable.Repeat(new string('x', 120), 160));

    private static bool CaretVisible(TextEditorCore core)
    {
        var native = View(core).Editor;
        var y = native.PointYFromPosition(native.CurrentPos);
        var x = native.PointXFromPosition(native.CurrentPos);
        var surface = FindNamed<Border>(View(core), "ImageTarget");
        var scale = Windows.Graphics.Display.DisplayInformation.GetForCurrentView().RawPixelsPerViewPixel;
        return y >= 0 && y < native.LinesOnScreen * native.TextHeight(0) &&
            x >= 0 && x < surface.ActualWidth * scale;
    }

    private static async Task CheckLogAppendAsync(StringBuilder log, TextWrapping wrapping, bool pending)
    {
        var scenario = $".LOG {wrapping}, {(pending ? "pending" : "completed")} restore";
        using var core = Create(wrapping);
        var before = LongLogText();
        await core.LoadTextAsync(before);
        core.SetFontZoomFactor(150);
        core.SetScrollViewerInitPosition(60, 800, 40);
        var view = View(core);
        var native = view.Editor;
        var observed = new List<bool>();
        var positions = new List<(long Row, long X, long Y)>();
        void Painted(WinUIEditor.Editor sender, WinUIEditor.PaintedEventArgs args)
        {
            observed.Add(CaretVisible(core));
            positions.Add((sender.FirstVisibleLine, sender.PointXFromPosition(sender.CurrentPos),
                sender.PointYFromPosition(sender.CurrentPos)));
        }
        try
        {
            if (!pending)
            {
                await EditorTestLayout.AfterPaintAsync(native, () => Window.Current.Content = core);
                await UntilFrameAsync(() => !view.IsViewportRestorePending);
            }
            int? pendingLine = null;
            var pendingSubLine = 0;
            double pendingX = 0, pendingY = 0;
            var after = string.Empty;
            native.Painted += Painted;
            await EditorTestLayout.AfterPaintAsync(native, () =>
            {
                Check(view.IsViewportRestorePending == pending, scenario + ": initial restore state");
                // No await between this assertion and the append: force the ordering
                // that can occur when Loaded runs during scrollbar layout deferral.
                core.TryInsertNewLogEntry();
                core.GetViewportAnchor(out pendingLine, out pendingSubLine);
                core.GetScrollViewerPosition(out pendingX, out pendingY);
                after = core.GetText();
                Check(after.StartsWith(before + "\r", StringComparison.Ordinal) && after.EndsWith('\r') &&
                    after.Length > before.Length + 2, scenario + ": timestamp appended without changing existing text");
                Check(native.CurrentPos == native.Length && native.Anchor == native.Length, scenario + ": caret at EOF");
                Check(view.IsViewportRestorePending, scenario + ": append waits for final layout before following EOF");
                if (pending) Window.Current.Content = core;
            });
            await UntilFrameAsync(() => !view.IsViewportRestorePending);
            // .LOG, timestamp and final empty line are short; the 160 identical
            // history lines give an independent idle-wrap completion condition.
            await UntilFrameAsync(() => native.VisibleFromDocLine(native.LineCount) == 3 + 160 * native.WrapCount(1));
            Check(observed.Count > 0 && observed.All(visible => visible),
                scenario + $": every paint must show the appended EOF caret; row/X/Y: {string.Join(", ", positions.Take(12))}");
            Check(pendingLine == null && pendingSubLine == 0 && pendingX == 0 && pendingY == 0,
                scenario + ": metadata must not retain the superseded anchor before first paint");
            core.TryInsertNewLogEntry();
            Check(core.GetText() == after && !view.IsViewportRestorePending,
                scenario + ": only one automatic append and caret-follow request");
        }
        finally
        {
            native.Painted -= Painted;
            Window.Current.Content = null;
        }
        log.AppendLine($"PASS: {scenario} shows the new EOF caret from the first paint.");
    }

    private static async Task CheckBlockedLogAppendAsync(StringBuilder log, bool readOnly)
    {
        using var core = Create(TextWrapping.NoWrap);
        var before = LongLogText();
        await core.LoadTextAsync(before);
        core.SetScrollViewerInitPosition(0, 800, 40);
        var view = View(core);
        if (readOnly) view.Editor.ReadOnly = true;
        else core.IsEnabled = false;
        Check(view.IsViewportRestorePending, "Blocked .LOG starts with a pending restore");
        core.TryInsertNewLogEntry();
        Check(core.GetText() == before, "Blocked .LOG must not append text");
        try
        {
            await EditorTestLayout.AfterPaintAsync(view.Editor, () => Window.Current.Content = core);
            await UntilFrameAsync(() => !view.IsViewportRestorePending);
            Check(Position(view.Editor).Line == 40, "Skipped .LOG append must preserve the saved viewport");
        }
        finally { Window.Current.Content = null; }
        log.AppendLine($"PASS: {(readOnly ? "read-only" : "disabled")} .LOG preserves its pending viewport.");
    }

    private static void Check(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }

    private static T FindNamed<T>(DependencyObject root, string name) where T : FrameworkElement
    {
        if (root is T element && element.Name == name) return element;
        for (var i = 0; i < VisualTreeHelper.GetChildrenCount(root); i++)
            if (FindNamed<T>(VisualTreeHelper.GetChild(root, i), name) is T child) return child;
        return null;
    }
}
