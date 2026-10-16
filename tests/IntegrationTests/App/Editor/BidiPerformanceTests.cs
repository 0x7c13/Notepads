// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading.Tasks;
using Notepads.Features.Documents.Storage;
using Notepads.Presentation.Controls.TextEditor;
using Windows.Storage;
using Windows.UI.Core;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Media;
using WinUIEditor;

namespace NotepadsEditorTests;

// Optional measurements, with no elapsed-time pass/fail threshold. This source
// also compiles against the pre-RTL baseline: it uses existing native properties
// rather than the new managed reading-direction property. R2l scenarios measure
// the new feature's cost separately; old R2l behavior is not a correctness oracle.
internal static class BidiPerformanceTests
{
    internal static async Task RunAsync(StringBuilder log)
    {
        var previousContent = Window.Current.Content;
        var host = new Grid();
        Window.Current.Content = host;
        Window.Current.Activate();
        Append(log, "RTL-PERF architecture=" + RuntimeInformation.ProcessArchitecture);
        Append(log, "RTL-PERF columns=fixture,cycle,phase,milliseconds,privateBytes,workingSet,managedBytes,appBytes");
        try
        {
            foreach (var fixture in new[]
            {
                (Name: "ASCII_32K", Text: Repeat("alpha bravo 1234 ", 32768), Rtl: false),
                (Name: "CJK_32K", Text: Repeat("中文测试日本語 ", 32768), Rtl: false),
                (Name: "Mixed_32K", Text: Repeat("ABC אבג مرحبا 123 😀 ", 32768), Rtl: false),
                (Name: "RTL_32K", Text: Repeat("ABC אבג مرحبا 123 😀 ", 32768), Rtl: true),
                (Name: "RTL_30LongLines", Text: Repeat(Repeat("אבג مرحبا ABC 123 ", 5120) + "\r", 5121 * 30), Rtl: true),
                (Name: "ASCII_5000Lines", Text: Repeat("alpha bravo 1234\r", 80000), Rtl: false),
                (Name: "Mixed_256K_Stress", Text: Repeat("ABC אבג مرحبا 123 😀 ", 262144), Rtl: false)
            })
            {
                using var baseline = await DocumentBaseline.FromTextAsync(fixture.Text, Guid.NewGuid());
                Append(log, $"RTL-PERF fixture={fixture.Name},utf16={fixture.Text.Length},utf8={Encoding.UTF8.GetByteCount(fixture.Text)},rtl={fixture.Rtl}");
                var stress = fixture.Name.EndsWith("_Stress", StringComparison.Ordinal);
                var cycles = stress ? 1 : 5;
                var arrows = stress ? 2 : 16;
                var scrolls = stress ? 2 : 20;
                Append(log, $"RTL-PERF workload={fixture.Name},cycles={cycles},arrows={arrows},scrolls={scrolls}");
                for (var cycle = 0; cycle < cycles; cycle++)
                {
                    await RunCycleAsync(log, host, baseline, fixture.Name, cycle, fixture.Rtl, arrows, scrolls);
                    // Let apartment releases run before observing retention.
                    await Window.Current.Dispatcher.RunAsync(CoreDispatcherPriority.Low, () => { });
                    await Task.Run(() => { GC.Collect(); GC.WaitForPendingFinalizers(); GC.Collect(); });
                    await Window.Current.Dispatcher.RunAsync(CoreDispatcherPriority.Low, () => { });
                    Record(log, fixture.Name, cycle, "afterDisposeAndGc", 0);
                }
            }
            await RunSelectionMeasurementsAsync(log, host);
        }
        finally { Window.Current.Content = previousContent; }
        log.AppendLine("PASS: RTL/LTR/CJK load, paint, navigation, scrolling, wrapping and memory observations collected.");
    }

    // Stress measurements have a separate timeout; functional tests retain 5s.
    private static Task AfterPaintAsync(Editor native, Action action) =>
        EditorTestLayout.AfterPaintAsync(native, action, TimeSpan.FromSeconds(60));

    private static async Task RunCycleAsync(StringBuilder log, Grid host, DocumentBaseline baseline, string name, int cycle, bool rtl,
        int arrows, int scrolls)
    {
        Record(log, name, cycle, "beforeCreate", 0);
        var watch = Stopwatch.StartNew();
        using var core = new TextEditorCore
        {
            Width = 620, Height = 400, FontSize = 18, TextWrapping = TextWrapping.NoWrap,
            HorizontalAlignment = HorizontalAlignment.Left, VerticalAlignment = VerticalAlignment.Top
        };
        var native = ((EditorBaseControl)((Grid)core.Content).Children[0]).Editor;
        native.Bidirectional = rtl ? Bidirectional.R2l : Bidirectional.L2r;
        Record(log, name, cycle, "construct", watch.Elapsed.TotalMilliseconds);
        try
        {
            watch.Restart();
            await core.LoadBaselineAsync(baseline, preserveUndo: false);
            Record(log, name, cycle, "nativeLoad", watch.Elapsed.TotalMilliseconds);
            // Force distinct foreground runs regardless of the machine theme.
            // Dark normally uses white for both ordinary and selected text.
            var normalForeground = native.StyleGetFore(0) & 0xffffff;
            var selectedForeground = unchecked((int)0xff000000) | (normalForeground ^ 0xffffff);
            native.SetElementColour(Element.SelectionText, selectedForeground);
            native.SetElementColour(Element.SelectionInactiveText, selectedForeground);
            Append(log, $"RTL-PERF selectionColors={name},{cycle},normal={normalForeground:X6},selected={selectedForeground:X8}");
            watch.Restart();
            await AfterPaintAsync(native, () => host.Children.Add(core));
            Record(log, name, cycle, "firstPaint", watch.Elapsed.TotalMilliseconds);
            if (native.GetElementColour(Element.SelectionText) != selectedForeground ||
                native.GetElementColour(Element.SelectionInactiveText) != selectedForeground ||
                (native.StyleGetFore(0) & 0xffffff) == (selectedForeground & 0xffffff))
                throw new InvalidOperationException("Performance selection fixture must retain distinct foreground colors.");
            native.SetEmptySelection(0);
            native.ChooseCaretX();
            watch.Restart();
            if (rtl) native.CharLeft(); else native.CharRight();
            Record(log, name, cycle, "firstArrowCall", watch.Elapsed.TotalMilliseconds);
            watch.Restart();
            // The old implementation rebuilds long-line geometry per arrow.
            // Keep both builds' repeated work identical and bounded in duration.
            for (var index = 0; index < arrows; index++)
                if (rtl) native.CharLeft(); else native.CharRight();
            Record(log, name, cycle, $"next{arrows}ArrowCalls", watch.Elapsed.TotalMilliseconds);

            native.SetEmptySelection(0);
            native.EmptyUndoBuffer();
            var originalLength = native.Length;
            double inputCallMilliseconds = 0;
            watch.Restart();
            await AfterPaintAsync(native, () =>
            {
                var inputWatch = Stopwatch.StartNew();
                core.TypeText("x");
                inputCallMilliseconds = inputWatch.Elapsed.TotalMilliseconds;
            });
            Record(log, name, cycle, "insertAndPaint", watch.Elapsed.TotalMilliseconds);
            Record(log, name, cycle, "insertCall", inputCallMilliseconds);
            watch.Restart();
            await AfterPaintAsync(native, () =>
            {
                var inputWatch = Stopwatch.StartNew();
                core.Undo();
                inputCallMilliseconds = inputWatch.Elapsed.TotalMilliseconds;
            });
            Record(log, name, cycle, "undoAndPaint", watch.Elapsed.TotalMilliseconds);
            Record(log, name, cycle, "undoCall", inputCallMilliseconds);
            if (native.Length != originalLength || core.CanUndo)
                throw new InvalidOperationException("Performance input fixture did not restore its baseline in one undo.");

            long selectionEnd = 0;
            for (var index = 0; index < 3; index++) selectionEnd = native.PositionAfter(selectionEnd);
            // SetSel also scrolls the caret; SetSelection leaves this paint's
            // viewport unchanged while selecting the same logical characters.
            watch.Restart();
            await AfterPaintAsync(native, () => native.SetSelection(0, selectionEnd));
            Record(log, name, cycle, "selectAndPaint", watch.Elapsed.TotalMilliseconds);
            watch.Restart();
            await AfterPaintAsync(native, () => native.SetEmptySelection(0));
            Record(log, name, cycle, "clearSelectionPaint", watch.Elapsed.TotalMilliseconds);

            watch.Restart();
            for (var index = 0; index < scrolls; index++)
            {
                var target = index % 2 == 0;
                await AfterPaintAsync(native, () =>
                {
                    if (native.LineCount > 1) native.FirstVisibleLine = target ? native.LineCount / 2 : 0;
                    else native.XOffset = target ? 400 : 0;
                });
            }
            Record(log, name, cycle, $"{scrolls}ScrollPaints", watch.Elapsed.TotalMilliseconds);
            watch.Restart();
            await AfterPaintAsync(native, () => core.TextWrapping = TextWrapping.Wrap);
            Record(log, name, cycle, "enableWrapPaint", watch.Elapsed.TotalMilliseconds);
            watch.Restart();
            await AfterPaintAsync(native, () => native.SetSelection(0, selectionEnd));
            Record(log, name, cycle, "wrappedSelectAndPaint", watch.Elapsed.TotalMilliseconds);
            watch.Restart();
            await AfterPaintAsync(native, () => native.SetEmptySelection(0));
            Record(log, name, cycle, "wrappedClearSelectionPaint", watch.Elapsed.TotalMilliseconds);
            // The first wrapped paint can precede XAML arranging a newly visible
            // vertical scrollbar. Keep that first-selection cost above, and
            // separately measure selection once the narrower viewport is painted.
            watch.Restart();
            await AfterPaintAsync(native, () => native.SetSelection(0, selectionEnd));
            Record(log, name, cycle, "wrappedSteadySelectAndPaint", watch.Elapsed.TotalMilliseconds);
            watch.Restart();
            await AfterPaintAsync(native, () => native.SetEmptySelection(0));
            Record(log, name, cycle, "wrappedSteadyClearSelectionPaint", watch.Elapsed.TotalMilliseconds);
            watch.Restart();
            await AfterPaintAsync(native, () => { core.Width = 420; core.UpdateLayout(); });
            Record(log, name, cycle, "resizeWrappedPaint", watch.Elapsed.TotalMilliseconds);
            watch.Restart();
            foreach (var rightToLeft in new[] { !rtl, rtl })
                await AfterPaintAsync(native, () =>
                {
                    native.Bidirectional = rightToLeft ? Bidirectional.R2l : Bidirectional.L2r;
                    // Old Scintilla ignored R2l. A caret move ensures that both
                    // builds paint, while the new build also resolves direction.
                    native.SetEmptySelection(native.CurrentPos == 0 ? native.PositionAfter(0) : 0);
                });
            Record(log, name, cycle, "directionRoundtripPaint", watch.Elapsed.TotalMilliseconds);
        }
        finally { host.Children.Remove(core); }
    }

    private static async Task RunSelectionMeasurementsAsync(StringBuilder log, Grid host)
    {
        // These operations were incorrect in the pre-fix host. Keep their cost
        // separate from the unchanged 31-cycle comparison, rather than treating
        // the old wrong output as an equivalent performance baseline.
        const string category = "RTL-TARGET";
        Append(log, category + " currentOnly=True,repetitions=5,layoutResetEachRepetition=True,callTimingExcludesFixtureAndPaint=True");
        foreach (var fixture in new[]
        {
            (Name: "MixedCollapse_Short", Kind: 0, Size: 0),
            (Name: "MixedCollapse_32K", Kind: 0, Size: 32768),
            (Name: "RectangleReplace_32Rows", Kind: 1, Size: 32),
            (Name: "RectangleReplace_161Rows", Kind: 1, Size: 161),
            (Name: "RectanglePaste_16Glyphs", Kind: 2, Size: 16),
            (Name: "RectanglePaste_1000Glyphs", Kind: 2, Size: 1000)
        })
        {
            var tail = fixture.Kind == 1 && fixture.Size == 161 ? new string('א', 1000) : string.Empty;
            var source = fixture.Kind switch
            {
                0 => fixture.Size == 0 ? "abc אבג def" : Repeat("abc אבג def ", fixture.Size),
                1 => string.Join('\r', System.Linq.Enumerable.Repeat("אבג ABC", fixture.Size)) + tail,
                _ => "אבג\rאבג"
            };
            Append(log, $"{category} fixture={fixture.Name},utf16={source.Length},utf8={Encoding.UTF8.GetByteCount(source)}");
            Record(log, fixture.Name, 0, "beforeCreate", 0, category);
            {
                using var core = new TextEditorCore
                {
                    Width = 620, Height = 400, FontSize = 24, FontFamily = new FontFamily("Consolas"),
                    TextWrapping = TextWrapping.NoWrap, DisplayLineNumbers = false,
                    HorizontalAlignment = HorizontalAlignment.Left, VerticalAlignment = VerticalAlignment.Top
                };
                var native = ((EditorBaseControl)((Grid)core.Content).Children[0]).Editor;
                native.Bidirectional = fixture.Kind == 0 ? Bidirectional.L2r : Bidirectional.R2l;
                try
                {
                    await AfterPaintAsync(native, () => host.Children.Add(core));
                    for (var repetition = 0; repetition < 5; repetition++)
                    {
                        await AfterPaintAsync(native, () =>
                        {
                            native.SelectionMode = WinUIEditor.SelectionMode.Stream;
                            native.MoveExtendsSelection = false;
                            core.SetText(source);
                            native.SetEmptySelection(0);
                            native.EmptyUndoBuffer();
                        });
                        void Require(bool condition, string message)
                        {
                            if (!condition) throw new InvalidOperationException($"{fixture.Name}: {message}");
                        }
                        void Measure(string phase, Action operation)
                        {
                            var watch = Stopwatch.StartNew();
                            operation();
                            watch.Stop();
                            Record(log, fixture.Name, repetition, phase, watch.Elapsed.TotalMilliseconds, category);
                        }
                        if (fixture.Kind == 0)
                        {
                            native.SetSelection(fixture.Size == 0 ? 12 : native.Length, fixture.Size == 0 ? 4 : 0);
                            Require(native.CurrentPos != native.Anchor, "Collapse starts with a nonempty mixed stream range.");
                            var left = repetition % 2 == 0;
                            Measure(left ? "collapseLeftCall" : "collapseRightCall", () =>
                            {
                                if (left) native.CharLeft(); else native.CharRight();
                            });
                            Require(native.CurrentPos == native.Anchor && native.Selections == 1 && core.GetText() == source,
                                "Collapse preserves the exact document and leaves one caret.");
                        }
                        else if (fixture.Kind == 1)
                        {
                            native.RectangularSelectionAnchor = 0;
                            native.RectangularSelectionCaret = (fixture.Size - 1) * 11L + 8;
                            Require(native.Selections == fixture.Size * 2, "Rectangle contains two logical islands per row.");
                            for (var row = 0; row < fixture.Size; row++)
                            {
                                Require(native.GetSelectionNStart(row * 2) == row * 11L &&
                                    native.GetSelectionNEnd(row * 2) == row * 11L + 7 &&
                                    native.GetSelectionNStart(row * 2 + 1) == row * 11L + 8 &&
                                    native.GetSelectionNEnd(row * 2 + 1) == row * 11L + 10,
                                    "Rectangle selects the exact Hebrew/Latin islands before measurement.");
                            }
                            var once = string.Join('\r', System.Linq.Enumerable.Repeat("XA", fixture.Size)) + tail;
                            var twice = string.Join('\r', System.Linq.Enumerable.Repeat("XYA", fixture.Size)) + tail;
                            Measure("rectangleReplaceCall", () => core.TypeText("X"));
                            Require(core.GetText() == once, "Replacement inserts once at each original leading edge.");
                            Measure("rectangleSecondInputCall", () => core.TypeText("Y"));
                            Require(core.GetText() == twice, "Second input preserves every row's insertion anchor.");
                            await AfterPaintAsync(native, core.Undo);
                            Require(core.GetText() == once, "One undo removes the second input across all rows.");
                            await AfterPaintAsync(native, core.Undo);
                            Require(core.GetText() == source && !core.CanUndo, "A second undo restores the exact source.");
                        }
                        else
                        {
                            var first = new string('א', fixture.Size);
                            var paste = first + "\rX";
                            var byteCount = Encoding.UTF8.GetByteCount(paste);
                            Measure("rectanglePasteCall", () => native.ReplaceRectangular(byteCount, paste));
                            Require(core.GetText() == first + "אבג\rXאבג", "Widening the first row preserves the second insertion column.");
                            await AfterPaintAsync(native, core.Undo);
                            Require(core.GetText() == source && !core.CanUndo, "One undo removes the complete rectangular paste.");
                        }
                    }
                }
                finally { host.Children.Remove(core); }
            }
            await Window.Current.Dispatcher.RunAsync(CoreDispatcherPriority.Low, () => { });
            await Task.Run(() => { GC.Collect(); GC.WaitForPendingFinalizers(); GC.Collect(); });
            await Window.Current.Dispatcher.RunAsync(CoreDispatcherPriority.Low, () => { });
            Record(log, fixture.Name, 0, "afterDisposeAndGc", 0, category);
        }
        Append(log, "PASS: current-only mixed-selection and rectangle call timings preserve exact text and undo.");
    }

    private static string Repeat(string pattern, int targetCharacters)
    {
        var text = new StringBuilder(targetCharacters);
        while (text.Length + pattern.Length <= targetCharacters) text.Append(pattern);
        return text.ToString();
    }

    private static void Record(StringBuilder log, string name, int cycle, string phase, double milliseconds,
        string category = "RTL-PERF")
    {
        using var process = Process.GetCurrentProcess();
        process.Refresh();
        Append(log, $"{category} {name},{cycle},{phase}," + milliseconds.ToString("F3", CultureInfo.InvariantCulture) +
            $",{process.PrivateMemorySize64},{process.WorkingSet64},{GC.GetTotalMemory(false)},{Windows.System.MemoryManager.AppMemoryUsage}");
    }

    private static void Append(StringBuilder log, string line)
    {
        log.AppendLine(line);
        File.AppendAllText(Path.Combine(ApplicationData.Current.LocalFolder.Path, "progress.txt"), line + Environment.NewLine);
    }
}
