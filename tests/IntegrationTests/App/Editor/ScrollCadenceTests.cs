// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents.FileTypes;
using Notepads.Features.Preferences;
using Notepads.Presentation.Controls.TextEditor;
using Windows.Graphics.Display;
using Windows.Storage;
using Windows.UI.Core;
using Windows.UI.ViewManagement;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Media;
using WinUIEditor;
using static NotepadsEditorTests.EditorTestLayout;

namespace NotepadsEditorTests;

// Opt-in diagnostics with private input copies. Rendering callbacks and native
// paint notifications are measured separately; neither is a presented-frame count.
internal static class ScrollCadenceTests
{
    internal static async Task RunAsync(StringBuilder log)
    {
        await PerformanceTestWindow.WaitForForegroundAsync("--scroll-cadence");
        var inputs = Path.Combine(ApplicationData.Current.LocalFolder.Path, "ScrollCadence");
        var files = Directory.GetFiles(inputs, "*.txt");
        if (files.Length == 0) throw new InvalidOperationException("No private scroll diagnostic inputs.");
        if (File.Exists(Path.Combine(inputs, "retention-only.flag"))) files = [];
        var previous = Window.Current.Content;
        var previousFont = ApplicationPreferences.EditorFontFamily;
        try
        {
            ApplicationView.GetForCurrentView().TryResizeView(new Windows.Foundation.Size(1200, 800));
            await Window.Current.Dispatcher.RunAsync(CoreDispatcherPriority.Low, () => { });
            var bounds = Window.Current.Bounds;
            var scale = DisplayInformation.GetForCurrentView().RawPixelsPerViewPixel;
            log.AppendLine(string.Create(CultureInfo.InvariantCulture, $"CADENCE window={bounds.Width}x{bounds.Height} scale={scale}"));
            foreach (var file in files.OrderBy(path => path, StringComparer.Ordinal))
            foreach (var font in new[] { "Consolas", "Microsoft YaHei UI" })
            foreach (var wrap in new[] { false, true })
            foreach (var large in new[] { false, true })
            {
                // Construction applies the real preference to native styles. The
                // inherited XAML FontFamily property alone does not update them.
                ApplicationPreferences.EditorFontFamily = font;
                var name = $"{Path.GetFileNameWithoutExtension(file)}/{font}/{(wrap ? "wrap" : "nowrap")}/{(large ? "large" : "small")}";
                RecordMemory(log, name, "beforeCreate");
                using (var core = new TextEditorCore
                {
                    Width = Math.Min(large ? bounds.Width : 620, bounds.Width),
                    Height = Math.Min(large ? bounds.Height : 360, bounds.Height), FontSize = 18,
                    TextWrapping = wrap ? TextWrapping.Wrap : TextWrapping.NoWrap,
                    HorizontalAlignment = HorizontalAlignment.Left, VerticalAlignment = VerticalAlignment.Top
                })
                {
                    var view = (EditorBaseControl)((Grid)core.Content).Children[0];
                    var native = view.Editor;
                    if (native.StyleGetFont(0) != font || native.StyleGetFont((int)StylesCommon.Default) != font)
                        throw new InvalidOperationException("The requested font was not applied to the native editor.");
                    core.SetText(File.ReadAllText(file).Replace("\r\n", "\r").Replace('\n', '\r'));
                    if (Path.GetFileNameWithoutExtension(file) == "syntax")
                        core.SetSyntaxLanguage(DocumentLanguages.Find("python"), "sample.py");
                    await AfterPaintAsync(native, () => Window.Current.Content = core);
                    await AfterPaintAsync(native, () => view.SetVerticalViewport(5, 0));
                    var height = native.TextHeight(0) / scale;
                    await AfterPaintAsync(native, () => view.SetVerticalViewport(5, height / 2));
                    log.AppendLine(string.Create(CultureInfo.InvariantCulture, $"CADENCE {name} size={core.ActualWidth}x{core.ActualHeight} nativeFont={native.StyleGetFont(0)} lineHeight={height}"));
                    using var process = Process.GetCurrentProcess();
                    var cpu = process.TotalProcessorTime;
                    var watch = Stopwatch.StartNew();
                    for (var i = 0; i < 40; i++)
                    {
                        var offset = height * (i % 2 == 0 ? 0.25 : 0.5);
                        await AfterPaintAsync(native, () => view.SetVerticalViewport(5, offset));
                    }
                    log.AppendLine(string.Create(CultureInfo.InvariantCulture,
                        $"CADENCE {name} repaint40 elapsedMs={watch.Elapsed.TotalMilliseconds:F3} cpuMs={(process.TotalProcessorTime - cpu).TotalMilliseconds:F3}"));
                    await MeasureFramesAsync(log, name, view, height, false);
                    await MeasureFramesAsync(log, name, view, height, true);
                    Window.Current.Content = null;
                }
                await Window.Current.Dispatcher.RunAsync(CoreDispatcherPriority.Low, () => { });
                await Task.Run(() => { GC.Collect(); GC.WaitForPendingFinalizers(); GC.Collect(); });
                // Native final_release posts resource destruction to the UI thread.
                await Window.Current.Dispatcher.RunAsync(CoreDispatcherPriority.Low, () => { });
                RecordMemory(log, name, "afterDispose");
            }
            ApplicationPreferences.EditorFontFamily = "Microsoft YaHei UI";
            await MeasureRetentionAsync(log, bounds.Width, bounds.Height);
        }
        finally
        {
            ApplicationPreferences.EditorFontFamily = previousFont;
            Window.Current.Content = previous;
        }
    }

    // Keep several independently styled editors alive, as inactive tabs do. The
    // sweep exercises eviction and revisits rather than only a stationary page.
    private static async Task MeasureRetentionAsync(StringBuilder log, double width, double height)
    {
        var references = await PopulateAndDisposeEditorsAsync(log, width, height);
        // Completing the creating method releases its async locals. XAML must
        // then process removal of the old tree before native final_release can run.
        for (var pass = 0; pass < 3; pass++)
        {
            var frame = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
            void Rendering(object sender, object args) => frame.TrySetResult(true);
            CompositionTarget.Rendering += Rendering;
            try { await frame.Task.WaitAsync(TimeSpan.FromSeconds(5)); }
            finally { CompositionTarget.Rendering -= Rendering; }
            await Task.Run(() => { GC.Collect(); GC.WaitForPendingFinalizers(); GC.Collect(); });
            await Window.Current.Dispatcher.RunAsync(CoreDispatcherPriority.Low, () => { });
        }
        log.AppendLine($"CADENCE retention released managedEditorsAlive={references.Count(reference => reference.IsAlive)}");
        RecordMemory(log, "retention", "afterDispose");
    }

    private static async Task<WeakReference[]> PopulateAndDisposeEditorsAsync(StringBuilder log, double width, double height)
    {
        var editors = new List<TextEditorCore>();
        RecordMemory(log, "retention", "beforeCreate");
        try
        {
            for (var tab = 0; tab < 8; tab++)
            {
                var core = new TextEditorCore { Width = width, Height = height, FontSize = 18, TextWrapping = TextWrapping.NoWrap };
                editors.Add(core);
                core.SetText(string.Join('\r', Enumerable.Range(0, 1200).Select(row =>
                    $"value_{tab}_{row} = process('中文段落 {tab} {row} office AV', {row}) # 说明 {row}")));
                core.SetSyntaxLanguage(DocumentLanguages.Find("python"), "sample.py");
                var view = (EditorBaseControl)((Grid)core.Content).Children[0];
                if (view.Editor.StyleGetFont(0) != "Microsoft YaHei UI")
                    throw new InvalidOperationException("The retention font was not applied.");
                await AfterPaintAsync(view.Editor, () => Window.Current.Content = core);
                for (var step = 1; step <= 20; step++)
                {
                    if (Window.Current.CoreWindow.ActivationMode != CoreWindowActivationMode.ActivatedInForeground)
                        throw new InvalidOperationException("Retention measurement lost foreground.");
                    var row = step <= 10 ? step * 80 : (20 - step) * 80;
                    await AfterPaintAsync(view.Editor, () => view.SetVerticalViewport(row, 0));
                    if (view.Editor.FirstVisibleLine != row)
                        throw new InvalidOperationException("The retention sweep did not reach its requested row.");
                }
                RecordMemory(log, $"retention-{tab + 1}", "retained");
            }
            return editors.Select(core => new WeakReference(core)).ToArray();
        }
        finally
        {
            Window.Current.Content = new Grid();
            foreach (var core in editors) core.Dispose();
            editors.Clear();
        }
    }

    private static void RecordMemory(StringBuilder log, string name, string phase)
    {
        using var process = Process.GetCurrentProcess();
        log.AppendLine(string.Create(CultureInfo.InvariantCulture,
            $"CADENCE {name} {phase} privateBytes={process.PrivateMemorySize64} workingSet={process.WorkingSet64} managedBytes={GC.GetTotalMemory(false)}"));
    }

    private static async Task MeasureFramesAsync(StringBuilder log, string name, EditorBaseControl view, double height, bool animate)
    {
        var intervals = new List<double>(512);
        var completion = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
        using var process = Process.GetCurrentProcess();
        var cpu = process.TotalProcessorTime;
        var watch = Stopwatch.StartNew();
        double previous = -1, nextWheel = 0;
        var firstRow = view.Editor.FirstVisibleLine;
        var furthestRow = firstRow;
        using var sampling = new CancellationTokenSource();
        var memoryTask = Task.Run(async () =>
        {
            using var memoryProcess = Process.GetCurrentProcess();
            long peak = 0;
            try
            {
                while (true)
                {
                    memoryProcess.Refresh();
                    peak = Math.Max(peak, memoryProcess.PrivateMemorySize64);
                    await Task.Delay(100, sampling.Token).ConfigureAwait(false);
                }
            }
            catch (OperationCanceledException) when (sampling.IsCancellationRequested) { return peak; }
        });
        var paints = 0;
        long sampledPeakPrivate;
        void Painted(Editor sender, PaintedEventArgs args) => paints++;
        void Rendering(object sender, object args)
        {
            try
            {
                if (Window.Current.CoreWindow.ActivationMode != CoreWindowActivationMode.ActivatedInForeground)
                    throw new InvalidOperationException("Scroll measurement lost foreground.");
                var now = watch.Elapsed.TotalSeconds;
                furthestRow = Math.Max(furthestRow, view.Editor.FirstVisibleLine);
                if (previous >= 0) intervals.Add((now - previous) * 1000);
                previous = now;
                if (now >= (animate ? 2.2 : 1)) { completion.TrySetResult(true); return; }
                if (animate && now >= nextWheel)
                {
                    view.ScrollBy(0, (now < 1.1 ? 1 : -1) * 4 * height, true);
                    nextWheel = now + 0.1;
                }
            }
            catch (Exception error) { completion.TrySetException(error); }
        }
        view.Editor.Painted += Painted;
        CompositionTarget.Rendering += Rendering;
        try { await completion.Task.WaitAsync(TimeSpan.FromSeconds(15)); }
        finally
        {
            CompositionTarget.Rendering -= Rendering;
            view.Editor.Painted -= Painted;
            view.CancelScrollAnimation();
            sampling.Cancel();
            sampledPeakPrivate = await memoryTask;
        }
        if (intervals.Count == 0) throw new InvalidOperationException("No rendering intervals captured.");
        if (animate && furthestRow <= firstRow)
            throw new InvalidOperationException("The animated workload did not move the viewport.");
        var elapsed = watch.Elapsed.TotalMilliseconds;
        var cpuMs = (process.TotalProcessorTime - cpu).TotalMilliseconds;
        intervals.Sort();
        log.AppendLine(string.Create(CultureInfo.InvariantCulture,
            $"CADENCE {name} {(animate ? "animated" : "idle")} callbacks={intervals.Count + 1} paints={paints} movedRows={furthestRow - firstRow} elapsedMs={elapsed:F3} cpuMs={cpuMs:F3} medianMs={intervals[intervals.Count / 2]:F3} p95Ms={intervals[(int)((intervals.Count - 1) * .95)]:F3} sampledPeakPrivate={sampledPeakPrivate}"));
    }
}
