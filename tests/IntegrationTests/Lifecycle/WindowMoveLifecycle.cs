// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Collections.Generic;
using System.Text.Json;
using System.Threading.Tasks;
using Notepads.Features.Sessions.Contracts;
using Notepads.Presentation.Controls.TextEditor;
using Notepads.Presentation.Views.MainPage;
using Notepads.Presentation.Workspace.Activation;
using Windows.ApplicationModel;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Media;

namespace NotepadsIntegrationTests;

// Runs in the real production App/InstanceRouter, with only the outbound private
// test protocol enabled. The payload crosses Windows' broker and two AOT processes.
internal static class WindowMoveLifecycle
{
    private const string Original = ".LOG\rabc אבג 中文\rtail";
    private const string Current = ".LOG\rXabc אבג 中文\rtail";
    private static TransferToken _request;
    private static int _launchSequence;
    internal static double MoveMilliseconds { get; private set; }
    internal static bool Verified { get; private set; }

    internal static void Configure()
    {
        TestLauncher.DataLaunch = async (uri, options, data) =>
        {
            if (uri.Scheme != "notepads-integration-tests" ||
                options.TargetApplicationPackageFamilyName != Package.Current.Id.FamilyName)
                throw new InvalidOperationException("The lifecycle probe may only launch its own private protocol.");
            _request = JsonSerializer.Deserialize((string)data[WindowMoveActivation.DataKey], SessionJsonContext.Default.TransferToken);
            var sequence = ++_launchSequence;
            var result = false;
            try { result = await Windows.System.Launcher.LaunchUriAsync(uri, options, data); return result; }
            finally
            {
                using var process = Process.GetCurrentProcess();
                var path = Path.Combine(ApplicationData.Current.LocalFolder.Path, $"move-broker-{process.Id}-{sequence}.txt");
                File.WriteAllText(path + ".tmp", process.StartTime.ToUniversalTime().Ticks + "\n" + _request.TransferId.ToString("N") + "\n" + result);
                File.Move(path + ".tmp", path);
            }
        };
    }

    internal static async Task CommandAsync(NotepadsMainPage page, string action)
    {
        if (action == "measure-move-state")
        {
            await MeasureCaptureAsync(page);
        }
        else if (action == "move-last-tab")
        {
            if (page.TestCore.GetTabCount() != 1 || !await page.TestCore.MoveTabToNewWindowAsync(page.TestCore.GetSelectedTab()))
                throw new InvalidOperationException("The last tab did not complete its acknowledged move.");
        }
        else if (action == "move-large-tab")
        {
            var core = page.TestCore;
            var previous = core.GetSelectedTab();
            var editor = (TextEditor)await core.CreateNewTextEditorAsync("large move");
            editor.TestNativeCore.TextWrapping = TextWrapping.NoWrap;
            var native = editor.TestNativeCore.TestFeatureNative;
            native.SetText(string.Concat(Enumerable.Repeat(new string('x', 127) + "\r", 65536)));
            for (var i = 0; i < 8192; ++i)
            {
                native.BeginUndoAction(); native.InsertText(native.Length, "x"); native.EndUndoAction();
            }
            core.OpenTextEditor(editor);
            var tab = core.GetSelectedTab();
            core.SwitchTo(previous);
            var watch = Stopwatch.StartNew();
            if (!await core.MoveTabToNewWindowAsync(tab) || core.ContainsTab(tab) || core.GetSelectedTab() != previous)
                throw new InvalidOperationException("The large-history move did not commit the requested tab.");
            MoveMilliseconds = watch.Elapsed.TotalMilliseconds;
            Verified = true;
        }
        else if (action == "verify-large-move")
        {
            var editor = (TextEditor)page.TestCore.GetSelectedReadyEditor();
            var native = editor.TestNativeCore.TestFeatureNative;
            var expected = string.Concat(Enumerable.Repeat(new string('x', 127) + "\r", 65536)) + new string('x', 8192);
            if (page.TestCore.GetTabCount() != 1 || !editor.IsModified || editor.GetText() != expected || native.UndoActions != 8193)
                throw new InvalidOperationException("The large receiver lost text or native history.");
            native.Undo();
            if (editor.GetText() != expected[..^1]) throw new InvalidOperationException("Large moved undo failed.");
            native.Redo();
            if (editor.GetText() != expected) throw new InvalidOperationException("Large moved redo failed.");
            Verified = true;
        }
        else if (action == "move-tab")
        {
            var core = page.TestCore;
            var previous = core.GetSelectedTab();
            var editor = (TextEditor)await core.CreateNewTextEditorAsync("Untitled");
            core.OpenTextEditor(editor);
            await FrameAsync();
            editor.TypeText(Original);
            var native = editor.TestNativeCore.TestFeatureNative;
            native.EmptyUndoBuffer();
            native.BeginUndoAction(); native.InsertText(5, "X"); native.EndUndoAction();
            native.BeginUndoAction(); native.InsertText(6, "Y"); native.EndUndoAction();
            native.Undo();
            native.SetSelection(8, 6); native.AddSelection(3, 1); native.MainSelection = 1;
            editor.TestNativeCore.RightToLeftReadingOrder = true;
            editor.TestNativeCore.TextWrapping = TextWrapping.NoWrap;
            editor.TestNativeCore.SetFontZoomFactor(150);
            var tab = core.GetSelectedTab();
            core.SwitchTo(previous);
            var watch = Stopwatch.StartNew();
            if (!await core.MoveTabToNewWindowAsync(tab) || core.ContainsTab(tab) || core.GetSelectedTab() != previous)
                throw new InvalidOperationException("The real broker move did not commit only the background source tab.");
            MoveMilliseconds = watch.Elapsed.TotalMilliseconds;
            Verified = true;
        }
        else if (action == "verify-moved-tab")
        {
            var editor = (TextEditor)page.TestCore.GetSelectedReadyEditor();
            var native = editor.TestNativeCore.TestFeatureNative;
            if (page.TestCore.GetTabCount() != 1 || editor.GetText() != Current || !editor.IsModified ||
                !editor.TestNativeCore.RightToLeftReadingOrder || editor.TestNativeCore.TextWrapping != TextWrapping.NoWrap ||
                editor.TestNativeCore.GetFontZoomFactor() != 150 || native.Selections != 2 || native.MainSelection != 1 ||
                native.GetSelectionNCaret(0) != 8 || native.GetSelectionNAnchor(0) != 6 ||
                native.GetSelectionNCaret(1) != 3 || native.GetSelectionNAnchor(1) != 1)
                throw new InvalidOperationException("The real receiving process lost text, .LOG, editing modes or selections.");
            native.Redo();
            if (editor.GetText() != ".LOG\rXYabc אבג 中文\rtail") throw new InvalidOperationException("Moved redo tail failed.");
            native.Undo(); native.Undo();
            if (editor.GetText() != Original) throw new InvalidOperationException("Moved grouped undo failed.");
            native.Redo();
            if (editor.GetText() != Current) throw new InvalidOperationException("Moved redo failed.");
            Verified = true;
        }
        else if (action == "repeat-move")
        {
            if (_request == null || !await WindowMoveActivation.LaunchAsync(_request, "notepads-integration-tests"))
                throw new InvalidOperationException("The duplicate move activation was refused by Windows.");
        }
        else if (action == "focus-moved-input")
        {
            page.TestCore.FocusOnSelectedTextEditor();
        }
        else if (action == "verify-move-input")
        {
            var editor = (TextEditor)page.TestCore.GetSelectedReadyEditor();
            var input = Stopwatch.StartNew();
            while (editor.GetText() != Current + "Q" && input.Elapsed < TimeSpan.FromSeconds(5))
                await Task.Delay(25);
            if (editor.GetText() != Current + "Q") throw new InvalidOperationException("Real keyboard input did not reach the moved document.");
            editor.TestNativeCore.TestFeatureNative.Undo();
            if (editor.GetText() != Current) throw new InvalidOperationException("Real keyboard input did not join the moved undo history.");
        }
    }

    private static async Task MeasureCaptureAsync(NotepadsMainPage page)
    {
        var samples = new List<WindowMoveCaptureSample>();
        foreach (var large in new[] { false, true })
        {
            var preparation = Stopwatch.StartNew();
            var progress = Path.Combine(ApplicationData.Current.LocalFolder.Path, "window-move-capture-progress.txt");
            void Record(string stage) => File.AppendAllText(progress, $"large={large} {stage}: {preparation.Elapsed.TotalMilliseconds:F1} ms\n");
            var editor = (TextEditor)await page.TestCore.CreateNewTextEditorAsync("capture measurement");
            try
            {
                var native = editor.TestNativeCore.TestFeatureNative;
                editor.TestNativeCore.TextWrapping = TextWrapping.NoWrap;
                var text = string.Concat(Enumerable.Repeat(new string('x', 127) + "\r", large ? 65536 : 8));
                // Fixture preparation is not a paste/reveal benchmark. An unattached
                // editor has no viewport in which to wrap or reveal an 8 MiB paste.
                native.SetText(text);
                Record("loaded");
                for (var i = 0; i < (large ? 8192 : 8); ++i)
                {
                    native.BeginUndoAction(); native.InsertText(native.Length, "x"); native.EndUndoAction();
                }
                Record("history prepared");
                using var process = Process.GetCurrentProcess();
                process.Refresh();
                var before = process.PrivateMemorySize64;
                var watch = Stopwatch.StartNew();
                using var reader = native.AcquireStateReader();
                var acquire = watch.Elapsed.TotalMilliseconds;
                var advertised = reader.Length;
                ulong total = 0;
                while (true)
                {
                    var chunk = await reader.ReadAsync();
                    if (chunk.Length == 0) break;
                    if (chunk.Length > 65536) throw new InvalidOperationException("Capture allocated an unbounded chunk.");
                    total += chunk.Length;
                }
                if (total != advertised) throw new InvalidOperationException("Capture measurement read the wrong state length.");
                watch.Stop();
                Record("reader drained");
                process.Refresh();
                samples.Add(new WindowMoveCaptureSample
                {
                    LogicalDpi = Windows.Graphics.Display.DisplayInformation.GetForCurrentView().LogicalDpi,
                    DocumentBytes = native.Length, Actions = native.UndoActions, StateBytes = total,
                    AcquireMilliseconds = acquire, ReadMilliseconds = watch.Elapsed.TotalMilliseconds - acquire,
                    PrivateBytesBefore = before, PrivateBytesAfter = process.PrivateMemorySize64,
                    ProcessPeakWorkingSet = process.PeakWorkingSet64
                });
            }
            finally { await page.TestCore.DiscardUnattachedEditorAsync(editor); Record("disposed"); }
            using (var process = Process.GetCurrentProcess())
            {
                process.Refresh();
                samples[^1].PrivateBytesAfterDispose = process.PrivateMemorySize64;
                // WinRT finalization may marshal native releases back to this UI.
                // Waiting on that thread would deadlock the measurement itself.
                await Task.Run(() => { GC.Collect(); GC.WaitForPendingFinalizers(); });
                await FrameAsync();
                process.Refresh();
                samples[^1].PrivateBytesAfterCollection = process.PrivateMemorySize64;
                Record("collected");
            }
        }
        File.WriteAllText(Path.Combine(ApplicationData.Current.LocalFolder.Path, "window-move-capture.json"),
            JsonSerializer.Serialize(samples, LifecycleJsonContext.Default.ListWindowMoveCaptureSample));
    }

    private static async Task FrameAsync()
    {
        var frame = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        void Render(object sender, object args) => frame.TrySetResult();
        CompositionTarget.Rendering += Render;
        try { await frame.Task.WaitAsync(TimeSpan.FromSeconds(10)); }
        finally { CompositionTarget.Rendering -= Render; }
    }
}

internal sealed class WindowMoveCaptureSample
{
    public double LogicalDpi { get; set; }
    public long DocumentBytes { get; set; }
    public long Actions { get; set; }
    public ulong StateBytes { get; set; }
    public double AcquireMilliseconds { get; set; }
    public double ReadMilliseconds { get; set; }
    public long PrivateBytesBefore { get; set; }
    public long PrivateBytesAfter { get; set; }
    public long ProcessPeakWorkingSet { get; set; }
    public long PrivateBytesAfterDispose { get; set; }
    public long PrivateBytesAfterCollection { get; set; }
}
