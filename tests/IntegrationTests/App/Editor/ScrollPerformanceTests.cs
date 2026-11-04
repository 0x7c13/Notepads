// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Diagnostics;
using System.Globalization;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Notepads.Presentation.Controls.TextEditor;
using Windows.Graphics.Display;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Media;
using WinUIEditor;
using static NotepadsEditorTests.EditorTestLayout;

namespace NotepadsEditorTests;

// Current-only pixel/frame workload; BidiPerformance supplies the unchanged
// before/after workload. Measurements are evidence, never timing assertions.
internal static class ScrollPerformanceTests
{
    internal static async Task RunAsync(StringBuilder log)
    {
        await PerformanceTestWindow.WaitForForegroundAsync("--smooth-scroll-performance");
        var previous = Window.Current.Content;
        try
        {
            log.AppendLine("SCROLL-PERF columns=fixture,phase,paintSamples,elapsedMs,nativeCallMs,cpuMs,privateBytes,workingSet,peakWorkingSet,managedBytes");
            foreach (var fixture in new[]
            {
                (Name: "LTR", Text: "alpha bravo 1234 ", Rtl: false, Wrap: false),
                (Name: "CJK", Text: "中文测试日本語 ", Rtl: false, Wrap: false),
                (Name: "RTL", Text: "ABC אבג مرحبا 123 ", Rtl: true, Wrap: false),
                (Name: "Wrap", Text: "ABC 中文 אבג مرحبا 123 ", Rtl: true, Wrap: true)
            })
            {
                using (var core = new TextEditorCore
                {
                    Width = 620, Height = 400, FontSize = 18,
                    RightToLeftReadingOrder = fixture.Rtl,
                    TextWrapping = fixture.Wrap ? TextWrapping.Wrap : TextWrapping.NoWrap,
                    HorizontalAlignment = HorizontalAlignment.Left, VerticalAlignment = VerticalAlignment.Top
                })
                {
                    var view = (EditorBaseControl)((Grid)core.Content).Children[0];
                    Windows.UI.Xaml.Automation.AutomationProperties.SetAutomationId(view, "PerformanceEditor");
                    var native = view.Editor;
                    var line = string.Concat(Enumerable.Repeat(fixture.Text, fixture.Wrap ? 12 : 3));
                    core.SetText(string.Join('\r', Enumerable.Repeat(line, 1000)));
                    await AfterPaintAsync(native, () => Window.Current.Content = core);
                    await UntilFrameAsync(() => !view.IsViewportRestorePending && (!fixture.Wrap ||
                        native.VisibleFromDocLine(native.LineCount) == native.LineCount * native.WrapCount(0)));
                    await AfterPaintAsync(native, () => view.SetVerticalViewport(100, 0));
                    var height = native.TextHeight(0) / DisplayInformation.GetForCurrentView().RawPixelsPerViewPixel;
                    // Warm both visible boundaries before measuring a stationary
                    // first row. No edit/metric/width change invalidates layouts.
                    await AfterPaintAsync(native, () => view.SetVerticalViewport(100, height / 2));
                    using var process = Process.GetCurrentProcess();
                    var cpu = process.TotalProcessorTime;
                    var watch = Stopwatch.StartNew();
                    double calls = 0;
                    const int frames = 60;
                    for (var frame = 0; frame < frames; ++frame)
                    {
                        var offset = height * (frame % 2 == 0 ? 0.25 : 0.5);
                        await AfterPaintAsync(native, () =>
                        {
                            var start = Stopwatch.GetTimestamp();
                            view.SetVerticalViewport(100, offset);
                            calls += Stopwatch.GetElapsedTime(start).TotalMilliseconds;
                        });
                    }
                    Record(log, fixture.Name, "sameRowPixelPaint", frames, watch.Elapsed.TotalMilliseconds,
                        calls, (process.TotalProcessorTime - cpu).TotalMilliseconds);
                    if (native.FirstVisibleLine != 100) throw new InvalidOperationException("The stationary-row workload changed its first row.");
                    cpu = process.TotalProcessorTime;
                    watch.Restart();
                    var paints = 0;
                    void Painted(Editor sender, PaintedEventArgs args) => ++paints;
                    native.Painted += Painted;
                    try
                    {
                        await AfterPaintAsync(native, () => view.ScrollBy(0, 12.5 * height, true));
                        await UntilFrameAsync(() => !view.IsScrollAnimating);
                        // Include any final VSIS paint already queued by the
                        // last rendering callback, including a subpixel tail.
                        await Window.Current.Dispatcher.RunAsync(Windows.UI.Core.CoreDispatcherPriority.Low, () => { });
                    }
                    finally { native.Painted -= Painted; }
                    Record(log, fixture.Name, "animatedWheel", paints, watch.Elapsed.TotalMilliseconds,
                        0, (process.TotalProcessorTime - cpu).TotalMilliseconds);
                    Window.Current.Content = null;
                }
                await Window.Current.Dispatcher.RunAsync(Windows.UI.Core.CoreDispatcherPriority.Low, () => { });
                await Task.Run(() => { GC.Collect(); GC.WaitForPendingFinalizers(); GC.Collect(); });
                Record(log, fixture.Name, "afterDispose", 0, 0, 0, 0);
            }
            // Keep the established vertical workload above unchanged so older
            // hosts can supply a matching before/after comparison.
            foreach (var rtl in new[] { false, true }) await CheckHorizontalAsync(log, rtl);
            await CjkScrollPerformanceTests.RunAsync(log);
        }
        finally { Window.Current.Content = previous; }
    }

    private static async Task CheckHorizontalAsync(StringBuilder log, bool rtl)
    {
        var name = rtl ? "RTL_2D" : "LTR_2D";
        using (var core = new TextEditorCore
        {
            Width = 620, Height = 400, FontSize = 18, RightToLeftReadingOrder = rtl,
            TextWrapping = TextWrapping.NoWrap,
            HorizontalAlignment = HorizontalAlignment.Left, VerticalAlignment = VerticalAlignment.Top
        })
        {
            var view = (EditorBaseControl)((Grid)core.Content).Children[0];
            var native = view.Editor;
            core.SetText(string.Join('\r', Enumerable.Repeat(
                string.Concat(Enumerable.Repeat("alpha 中文 אבג مرحبا 123 ", 24)), 80)));
            await AfterPaintAsync(native, () => Window.Current.Content = core);
            await UntilFrameAsync(() => !view.IsViewportRestorePending);
            await AfterPaintAsync(native, () => view.SetVerticalViewport(10, 0));
            await AfterPaintAsync(native, () => native.XOffset = 100);
            var scale = DisplayInformation.GetForCurrentView().RawPixelsPerViewPixel;
            var height = native.TextHeight(0) / scale;
            using var process = Process.GetCurrentProcess();
            var cpu = process.TotalProcessorTime;
            var watch = Stopwatch.StartNew();
            double calls = 0;
            const int frames = 60;
            for (var frame = 0; frame < frames; ++frame)
            {
                await AfterPaintAsync(native, () =>
                {
                    var start = Stopwatch.GetTimestamp();
                    view.ScrollBy(1 / scale, 0, false);
                    calls += Stopwatch.GetElapsedTime(start).TotalMilliseconds;
                });
            }
            Record(log, name, "horizontalPixelPaint", frames, watch.Elapsed.TotalMilliseconds,
                calls, (process.TotalProcessorTime - cpu).TotalMilliseconds);
            if (native.XOffset != 100 + frames || native.FirstVisibleLine != 10 || view.DisplayVerticalOffset != 0)
                throw new InvalidOperationException("The horizontal workload did not move one device pixel per paint while retaining Y.");
            foreach (var diagonal in new[] { false, true })
            {
                var initialX = native.XOffset;
                cpu = process.TotalProcessorTime;
                watch.Restart();
                var paints = 0;
                void Painted(Editor sender, PaintedEventArgs args) => ++paints;
                native.Painted += Painted;
                try
                {
                    await AfterPaintAsync(native, () => view.ScrollBy(180, diagonal ? 12.5 * height : 0, true));
                    await UntilFrameAsync(() => !view.IsScrollAnimating);
                    await Window.Current.Dispatcher.RunAsync(Windows.UI.Core.CoreDispatcherPriority.Low, () => { });
                }
                finally { native.Painted -= Painted; }
                Record(log, name, diagonal ? "diagonalWheel" : "horizontalWheel", paints,
                    watch.Elapsed.TotalMilliseconds, 0, (process.TotalProcessorTime - cpu).TotalMilliseconds);
                if (native.XOffset != (int)Math.Round(initialX + 180 * scale, MidpointRounding.AwayFromZero) ||
                    native.FirstVisibleLine != (diagonal ? 22 : 10) ||
                    Math.Abs(view.DisplayVerticalOffset - (diagonal ? height / 2 : 0)) > 1e-8)
                    throw new InvalidOperationException("The animated workload did not complete its measured axis targets.");
            }
            Window.Current.Content = null;
        }
        await Window.Current.Dispatcher.RunAsync(Windows.UI.Core.CoreDispatcherPriority.Low, () => { });
        await Task.Run(() => { GC.Collect(); GC.WaitForPendingFinalizers(); GC.Collect(); });
        Record(log, name, "afterDispose", 0, 0, 0, 0);
    }

    private static async Task UntilFrameAsync(Func<bool> ready)
    {
        if (ready()) return;
        var completion = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
        void Rendering(object sender, object args) { if (ready()) completion.TrySetResult(true); }
        CompositionTarget.Rendering += Rendering;
        try { await completion.Task.WaitAsync(TimeSpan.FromSeconds(10)); }
        finally { CompositionTarget.Rendering -= Rendering; }
    }

    private static void Record(StringBuilder log, string fixture, string phase, int frames, double elapsed, double calls, double cpu)
    {
        using var process = Process.GetCurrentProcess();
        log.AppendLine(string.Create(CultureInfo.InvariantCulture,
            $"SCROLL-PERF {fixture},{phase},{frames},{elapsed:F3},{calls:F3},{cpu:F3},{process.PrivateMemorySize64},{process.WorkingSet64},{process.PeakWorkingSet64},{GC.GetTotalMemory(false)}"));
    }
}
