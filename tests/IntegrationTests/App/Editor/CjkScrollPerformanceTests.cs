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
using Notepads.Features.Documents.FileTypes;
using Notepads.Presentation.Controls.TextEditor;
using Windows.Graphics.Display;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Media;
using WinUIEditor;
using static NotepadsEditorTests.EditorTestLayout;

namespace NotepadsEditorTests;

// Opt-in observations, without timing assertions. Longer batches reduce the
// relative error of Windows' coarse process CPU accounting. Keep this workload
// compatible with the 5.6.3 wrapper so both complete hosts run identical code.
internal static class CjkScrollPerformanceTests
{
    internal static async Task RunAsync(StringBuilder log)
    {
        var previous = Window.Current.Content;
        log.AppendLine("CJK-PERF columns=fixture,batch,requestedPaints,actualPaints,elapsedMs,cpuMs,nativeCallMs,updates,contentUpdates,verticalUpdates,privateBytes,workingSet");
        try
        {
            foreach (var fixture in new[]
            {
                (Name: "ASCII_short", Text: "alpha bravo 1234 ", Repeats: 3, Rows: 1000, Wrap: false),
                (Name: "CJK_short", Text: "中文测试日本語 ", Repeats: 3, Rows: 1000, Wrap: false),
                (Name: "CJK_long", Text: "中文测试日本語 ", Repeats: 256, Rows: 128, Wrap: false),
                (Name: "CJK_wrap", Text: "中文测试日本語 ", Repeats: 64, Rows: 128, Wrap: true)
            })
            {
                using (var core = new TextEditorCore
                {
                    Width = 620, Height = 400, FontSize = 18,
                    TextWrapping = fixture.Wrap ? TextWrapping.Wrap : TextWrapping.NoWrap,
                    HorizontalAlignment = HorizontalAlignment.Left, VerticalAlignment = VerticalAlignment.Top
                })
                {
                    var view = (EditorBaseControl)((Grid)core.Content).Children[0];
                    var native = view.Editor;
                    Windows.UI.Xaml.Automation.AutomationProperties.SetAutomationId(view, "PerformanceEditor");
                    native.CaretPeriod = 0;
                    var line = string.Concat(Enumerable.Repeat(fixture.Text, fixture.Repeats));
                    core.SetText(string.Join('\r', Enumerable.Repeat(line, fixture.Rows)));
                    // TextEditor normally installs a language after loading. A bare core
                    // otherwise keeps requesting container styling on every paint,
                    // invalidating the line cache even though the text never changes.
                    core.SetSyntaxLanguage(DocumentLanguages.PlainText, "performance.txt");
                    var length = native.Length;
                    await AfterPaintAsync(native, () => Window.Current.Content = core);
                    await UntilLayoutAsync(() => !view.IsViewportRestorePending && (!fixture.Wrap ||
                        native.VisibleFromDocLine(native.LineCount) == native.LineCount * native.WrapCount(0)));
                    await AfterPaintAsync(native, () => view.SetVerticalViewport(100, 0));
                    var height = native.TextHeight(0) / DisplayInformation.GetForCurrentView().RawPixelsPerViewPixel;
                    // Warm text/fallback-font caches and both visible boundaries.
                    for (var frame = 0; frame < 60; ++frame)
                        await AfterPaintAsync(native, () => view.SetVerticalViewport(100, height * (frame % 2 == 0 ? 0.25 : 0.5)));

                    if (native.EndStyled != length)
                        throw new InvalidOperationException("The plain-text performance fixture has unfinished styling.");
                    log.AppendLine($"CJK-STYLED {fixture.Name},{native.EndStyled},{length}");

                    using var process = Process.GetCurrentProcess();
                    for (var batch = 0; batch < 3; ++batch)
                    {
                        const int frames = 600;
                        var paints = 0;
                        var updates = 0;
                        var contentUpdates = 0;
                        var verticalUpdates = 0;
                        void Painted(Editor sender, PaintedEventArgs args) => ++paints;
                        void Updated(Editor sender, UpdateUIEventArgs args)
                        {
                            ++updates;
                            if (((Update)args.Updated & Update.Content) != 0) ++contentUpdates;
                            if (((Update)args.Updated & Update.VScroll) != 0) ++verticalUpdates;
                        }
                        native.Painted += Painted;
                        native.UpdateUI += Updated;
                        double calls = 0;
                        var cpu = process.TotalProcessorTime;
                        var watch = Stopwatch.StartNew();
                        try
                        {
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
                        }
                        finally { native.Painted -= Painted; native.UpdateUI -= Updated; }
                        var elapsed = watch.Elapsed.TotalMilliseconds;
                        var cpuMs = (process.TotalProcessorTime - cpu).TotalMilliseconds;
                        process.Refresh();
                        log.AppendLine(string.Create(CultureInfo.InvariantCulture,
                            $"CJK-PERF {fixture.Name},{batch},{frames},{paints},{elapsed:F3},{cpuMs:F3},{calls:F3},{updates},{contentUpdates},{verticalUpdates},{process.PrivateMemorySize64},{process.WorkingSet64}"));
                        if (native.FirstVisibleLine != 100 || native.Length != length || native.EndStyled != length ||
                            Math.Abs(view.DisplayVerticalOffset - height / 2) > 1e-8 || paints < frames)
                            throw new InvalidOperationException("The CJK paint workload did not preserve its document and stationary first row.");
                    }
                    Window.Current.Content = null;
                }
                await Window.Current.Dispatcher.RunAsync(Windows.UI.Core.CoreDispatcherPriority.Low, () => { });
                await Task.Run(() => { GC.Collect(); GC.WaitForPendingFinalizers(); GC.Collect(); });
                using var disposedProcess = Process.GetCurrentProcess();
                log.AppendLine(string.Create(CultureInfo.InvariantCulture,
                    $"CJK-DISPOSE {fixture.Name},{disposedProcess.PrivateMemorySize64},{disposedProcess.WorkingSet64}"));
            }
        }
        finally { Window.Current.Content = previous; }
    }

    private static async Task UntilLayoutAsync(Func<bool> ready)
    {
        if (ready()) return;
        var completion = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
        void Rendering(object sender, object args) { if (ready()) completion.TrySetResult(true); }
        CompositionTarget.Rendering += Rendering;
        try { await completion.Task.WaitAsync(TimeSpan.FromSeconds(30)); }
        finally { CompositionTarget.Rendering -= Rendering; }
    }
}
