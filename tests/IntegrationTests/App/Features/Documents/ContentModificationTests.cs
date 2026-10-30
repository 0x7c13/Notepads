// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents.Contracts;
using Notepads.Presentation.Controls.TextEditor;
using NotepadsEditorTests;
using NotepadsIntegrationTests;
using Windows.Foundation;
using Windows.Storage;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using static NotepadsWorkspaceTests.DocumentFeatureAssertions;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsWorkspaceTests;

internal static class ContentModificationTests
{
    internal static async Task RunAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        await EmptyAndMetadataAsync(host);
        await EqualityUndoRecoveryCloseAsync(host);
        await StreamingAndRacesAsync(host, log);
        await SupersessionAndReloadAsync(host);
        await FailedReloadAsync(host);
        await DisposalAsync(host);
        log.AppendLine("PASS: exact saved-content equality clears manual reversions without moving undo save points; empty/metadata, UI, close, recovery, cancellation and disposal contracts hold.");
    }

    private static void Replace(TextEditor editor, string text)
    {
        var native = editor.TestNativeCore.TestFeatureNative;
        native.BeginUndoAction();
        try { native.SetSel(0, native.Length); editor.TypeText(text); }
        finally { native.EndUndoAction(); }
    }

    private static void ReplaceFirst(TextEditor editor, string text)
    {
        editor.TestNativeCore.SetTextSelectionPosition(0, 1);
        editor.TypeText(text);
    }

    private static void CheckIndicators(FeatureTestHost host, TextEditor editor, bool modified)
    {
        var tab = host.Core.GetTab(editor);
        host.Page.TestRefreshMenu();
        Check(editor.IsModified == modified && tab.IsModified == modified && host.Page.TestStatusModified == modified &&
            (host.Page.TestContainer(tab).Icon.Visibility == Visibility.Visible) == modified &&
            ((MenuFlyoutItem)host.Page.TestFeatureControl("MenuSaveButton")).IsEnabled == modified,
            "Tab dot, status and Save action agree with the document's modification state.");
    }

    private static async Task EmptyAndMetadataAsync(FeatureTestHost host)
    {
        var editor = (TextEditor)host.Core.GetSelectedReadyEditor();
        Check(editor.IsDocumentEmpty, "The manual-empty fixture starts with an untitled empty baseline.");
        Replace(editor, "test");
        CheckIndicators(host, editor, true);
        Replace(editor, "");
        Check(editor.TestNativeCore.IsDocumentModified && !editor.IsModified && editor.TestComparedChunks.Count == 0,
            "Deleting back to empty clears dirty state immediately, without scanning or changing the native save point.");
        CheckIndicators(host, editor, false);
        editor.TestNativeCore.Undo();
        Check(editor.GetText() == "test" && editor.IsModified, "Undo manual deletion restores unsaved text.");
        editor.TestNativeCore.Redo();
        Check(editor.IsDocumentEmpty && !editor.IsModified, "Redo manual deletion restores a clean empty document.");
        editor.TryChangeEncoding(Encoding.Unicode);
        Check(editor.IsModified, "An encoding-only change remains dirty even when both texts are empty.");
        editor.TryChangeEncoding(editor.LastSavedSnapshot.Encoding);
        editor.TryChangeLineEnding(editor.LastSavedSnapshot.LineEnding == LineEnding.Lf ? LineEnding.Crlf : LineEnding.Lf);
        Check(editor.IsModified, "An EOL-only change remains dirty when both texts are empty.");
        editor.TryChangeLineEnding(editor.LastSavedSnapshot.LineEnding);
        Check(!editor.IsModified, "Returning metadata to its saved values restores clean state.");
        Replace(editor, " \r");
        Check(editor.IsModified, "Whitespace and line endings are content, not an empty-document shortcut.");
        Replace(editor, "");
        var emptyFile = await OpenAsync(host, "manual-empty.txt", []);
        Replace(emptyFile, "x");
        Replace(emptyFile, "");
        Check(!emptyFile.IsModified && emptyFile.TestComparedChunks.Count == 0,
            "The same immediate empty check applies to an actual empty saved file.");
        var core = emptyFile.TestNativeCore;
        var view = (WinUIEditor.EditorBaseControl)((Grid)core.Content).Children[0];
        await UntilAsync(() => view.ActualHeight > 0 && core.ActualHeight > 12, "The editor inset fixture must be arranged.");
        var surface = Descendants(view).OfType<Border>().Single(element => element.Name == "ImageTarget");
        var horizontal = Descendants(view).OfType<Windows.UI.Xaml.Controls.Primitives.ScrollBar>()
            .Single(element => element.Name == "HorizontalScrollBar");
        var top = surface.TransformToVisual(core).TransformPoint(new Point()).Y;
        Check(Math.Abs(top - 6) < 0.01 && Math.Abs(core.ActualHeight - surface.ActualHeight - top - 6) < 0.01,
            "The actual native viewport has six DIP of painted space above and below it.");
        var wrapping = core.TextWrapping;
        try
        {
            await EditorTestLayout.AfterPaintAsync(view.Editor, () =>
            {
                core.TextWrapping = TextWrapping.NoWrap;
                Replace(emptyFile, new string('W', 512));
            });
            await UntilAsync(() => horizontal.Visibility == Visibility.Visible && horizontal.ActualHeight > 0,
                "A long line exposes the actual horizontal scrollbar.");
            core.UpdateLayout();
            var barTop = horizontal.TransformToVisual(core).TransformPoint(new Point()).Y;
            var textBottom = surface.TransformToVisual(core).TransformPoint(new Point(0, surface.ActualHeight)).Y;
            Check(Math.Abs(barTop + horizontal.ActualHeight - core.ActualHeight) < 0.01 &&
                Math.Abs(barTop - textBottom - 6) < 0.01,
                "The horizontal scrollbar reaches the editor bottom while text retains its six DIP inset above it.");
        }
        finally
        {
            Replace(emptyFile, string.Empty);
            core.TextWrapping = wrapping;
        }
    }

    private static async Task EqualityUndoRecoveryCloseAsync(FeatureTestHost host)
    {
        const string source = "A\rΩ😀\0Z";
        var editor = await OpenAsync(host, "manual-equal.txt", Encoding.UTF8.GetBytes(source));
        ReplaceFirst(editor, "B");
        await editor.TestCompareContentAsync();
        Check(editor.IsModified, "Equal-length but different text remains dirty.");
        ReplaceFirst(editor, "A");
        await UntilAsync(() => !editor.IsModified, "The normal debounce clears a manually restored nonempty document.");
        Check(editor.TestNativeCore.IsDocumentModified && editor.GetText() == source,
            "Nonempty equality is independent of the native undo-history position.");
        CheckIndicators(host, editor, false);
        var reads = editor.TestComparedChunks.Count;
        await editor.TestCompareContentAsync();
        Check(editor.TestComparedChunks.Count == reads, "An unchanged comparison result is reused without rereading the document.");
        editor.TestNativeCore.Undo();
        Check(editor.GetText().StartsWith("B", StringComparison.Ordinal) && editor.IsModified, "Undo from a content-equivalent state is dirty.");
        editor.TestNativeCore.Undo();
        Check(editor.GetText() == source && !editor.IsModified && !editor.TestNativeCore.IsDocumentModified,
            "The original native save point remains reachable and clean.");
        editor.TestNativeCore.Redo();
        editor.TestNativeCore.Redo();
        await editor.TestCompareContentAsync();
        using var capture = editor.CaptureRecoveryState();
        Check(!capture.TextDirty && !capture.MetaData.IsModified, "Recovery captures semantic text equality as well as the UI state.");
        await capture.Checkpoint.FlushAsync();
        foreach (var missingFile in new[] { false, true })
        {
            var restored = (TextEditor)host.Core.CreateTextEditor(Guid.NewGuid(), null, "Recovered equality");
            try
            {
                await restored.RestoreV2Async(capture.SavedSnapshot, capture.RecoveryBaseline, capture.DocumentJournal,
                    capture.Checkpoint, missingFile ? null : editor.EditingFile, capture.MetaData, capture.TextDirty);
                Check(restored.GetText() == source && restored.IsModified == missingFile,
                    "Recovery preserves content equality, while a missing file still requires Save As.");
            }
            finally { await host.Core.DiscardUnattachedEditorAsync(restored); }
        }

        ReplaceFirst(editor, "B");
        ReplaceFirst(editor, "A");
        editor.TestComparisonPause = _ => throw new IOException("Injected baseline comparison failure.");
        try { await editor.TestCompareContentAsync(); }
        finally { editor.TestComparisonPause = null; }
        Check(editor.IsModified, "Failure to prove equality leaves the document conservatively dirty.");
        await editor.TestCompareContentAsync();
        Check(!editor.IsModified, "An explicit recheck can recover after a comparison failure.");

        ReplaceFirst(editor, "B");
        ReplaceFirst(editor, "A");
        var tab = host.Core.GetTab(editor);
        // Close immediately, before the debounce. A spurious modal prevents completion.
        await host.Page.TestCloseTabAsync(tab).WaitAsync(TimeSpan.FromSeconds(15));
        Check(!host.Core.ContainsTab(tab), "Immediate close settles equality and closes without a save reminder.");
    }

    private static async Task StreamingAndRacesAsync(FeatureTestHost host, StringBuilder log)
    {
        // A four-byte scalar crosses the first 64 KiB boundary.
        var source = string.Concat(Enumerable.Repeat("a\r", 32767)) + "😀\r" +
            string.Concat(Enumerable.Repeat(new string('x', 118) + "Ω😀\r", 9000));
        var editor = await OpenAsync(host, "manual-large.txt", Encoding.UTF8.GetBytes(source));
        ReplaceFirst(editor, "b");
        ReplaceFirst(editor, "a");
        editor.TestComparedChunks.Clear();
        var watch = Stopwatch.StartNew();
        await editor.TestCompareContentAsync();
        Check(!editor.IsModified && editor.TestComparedChunks.Sum(count => (long)count) == Encoding.UTF8.GetByteCount(source) &&
            editor.TestComparedChunks[0] == 65534 && editor.TestComparedChunks.All(count => count <= 65536) &&
            editor.TestComparisonBufferSizes.Count == 1 && editor.TestComparisonBufferSizes[0] == 65536,
            "Large Unicode equality reads each byte once in bounded, character-aligned chunks.");
        log.AppendLine($"MEASURE: saved-content comparison {editor.DocumentLength} UTF-8 bytes, {editor.TestComparedChunks.Count} chunks, {watch.Elapsed.TotalMilliseconds:F1} ms (evidence, not a timing gate).");
        ReplaceFirst(editor, "b");
        editor.TestComparedChunks.Clear();
        await editor.TestCompareContentAsync();
        Check(editor.IsModified && editor.TestComparedChunks.Count == 1, "Comparison stops at the first unequal chunk.");
        editor.TestNativeCore.SetTextSelectionPosition(0, 0);
        editor.TypeText("longer");
        editor.TestComparedChunks.Clear();
        await editor.TestCompareContentAsync();
        Check(editor.IsModified && editor.TestComparedChunks.Count == 0, "A length mismatch never starts a document scan.");
        Replace(editor, source);

        var entered = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var release = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        editor.TestComparisonPause = offset => { entered.TrySetResult(); return release.Task; };
        var comparing = editor.TestCompareContentAsync();
        try
        {
            await entered.Task.WaitAsync(TimeSpan.FromSeconds(10));
            Check(!editor.TestNativeCore.TestFeatureNative.ReadOnly, "Background comparison does not freeze the native document.");
            ReplaceFirst(editor, "b");
            release.TrySetResult();
            await comparing;
            Check(editor.IsModified && editor.GetText()[0] == 'b', "A canceled old equality result cannot clear a newer edit.");
        }
        finally { editor.TestComparisonPause = null; release.TrySetResult(); await comparing; }

        ReplaceFirst(editor, "a");
        entered = new(TaskCreationOptions.RunContinuationsAsynchronously);
        release = new(TaskCreationOptions.RunContinuationsAsynchronously);
        editor.TestComparisonPause = offset => { entered.TrySetResult(); return release.Task; };
        comparing = editor.TestCompareContentAsync();
        try
        {
            await entered.Task.WaitAsync(TimeSpan.FromSeconds(10));
            ReplaceFirst(editor, "b");
            await editor.SaveContentToFileAndUpdateEditorStateAsync(editor.EditingFile);
            ReplaceFirst(editor, "a");
            release.TrySetResult();
            await comparing;
            Check(editor.IsModified, "An old baseline's equality cannot clear content differing from a newly committed save.");
        }
        finally { editor.TestComparisonPause = null; release.TrySetResult(); await comparing; }
        await editor.TestCompareContentAsync();
        Check(editor.IsModified, "The new baseline independently confirms the post-save edit is dirty.");
    }

    private static async Task SupersessionAndReloadAsync(FeatureTestHost host)
    {
        var editor = await OpenAsync(host, "manual-races.txt", Encoding.UTF8.GetBytes("abc"));
        ReplaceFirst(editor, "b");
        ReplaceFirst(editor, "a");
        var entered = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var release = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        editor.TestComparisonPause = _ => { entered.TrySetResult(); return release.Task; };
        var requests = new List<Task> { editor.TestCompareContentAsync() };
        try
        {
            await entered.Task.WaitAsync(TimeSpan.FromSeconds(10));
            for (var i = 0; i < 20; i++)
            {
                ReplaceFirst(editor, i % 2 == 0 ? "b" : "a");
                requests.Add(editor.TestCompareContentAsync());
            }
            Check(editor.TestComparisonBufferSizes.Count == 1 && editor.TestComparedChunks.Count == 1,
                "Superseding requests cannot overlap the paused reader or allocate additional content buffers.");
        }
        finally
        {
            editor.TestComparisonPause = null;
            release.TrySetResult();
            await Task.WhenAll(requests).WaitAsync(TimeSpan.FromSeconds(10));
        }
        Check(!editor.IsModified && editor.TestComparisonBufferSizes.Count == 2 &&
            editor.TestComparisonBufferSizes.All(size => size == 4) && editor.TestComparedChunks.Count == 2,
            "Only the final superseding request reads text; short UTF-8 documents use scalar-sized buffers.");

        ReplaceFirst(editor, "b");
        ReplaceFirst(editor, "a");
        entered = new(TaskCreationOptions.RunContinuationsAsynchronously);
        release = new(TaskCreationOptions.RunContinuationsAsynchronously);
        editor.TestComparisonPause = _ => { entered.TrySetResult(); return release.Task; };
        var comparing = editor.TestCompareContentAsync();
        try
        {
            await entered.Task.WaitAsync(TimeSpan.FromSeconds(10));
            editor.TryChangeEncoding(Encoding.Unicode);
            release.TrySetResult();
            await comparing;
            using var capture = editor.CaptureRecoveryState();
            Check(editor.IsModified && !capture.TextDirty && capture.MetaData.IsModified,
                "An equality result cannot clear encoding changes made while comparison was pending.");
        }
        finally { editor.TestComparisonPause = null; release.TrySetResult(); await comparing; }
        editor.TryChangeEncoding(editor.LastSavedSnapshot.Encoding);
        Check(!editor.IsModified, "Restoring saved metadata reuses the already proven text equality.");

        ReplaceFirst(editor, "b");
        ReplaceFirst(editor, "a");
        entered = new(TaskCreationOptions.RunContinuationsAsynchronously);
        release = new(TaskCreationOptions.RunContinuationsAsynchronously);
        editor.TestComparisonPause = _ => { entered.TrySetResult(); return release.Task; };
        comparing = editor.TestCompareContentAsync();
        try
        {
            await entered.Task.WaitAsync(TimeSpan.FromSeconds(10));
            await FileIO.WriteBytesAsync(editor.EditingFile, Encoding.UTF8.GetBytes("bbc"));
            await editor.ReloadFromEditingFileAsync();
            Check(editor.GetText() == "bbc" && !editor.IsModified, "Reload adopts the new saved baseline while the old comparison is paused.");
            ReplaceFirst(editor, "a");
            release.TrySetResult();
            await comparing;
            Check(editor.IsModified, "An old equality result cannot clear a post-reload edit against a different baseline.");
        }
        finally { editor.TestComparisonPause = null; release.TrySetResult(); await comparing; }
        await editor.TestCompareContentAsync();
        Check(editor.IsModified, "The reloaded baseline independently rejects equality with the old file content.");
    }

    private static async Task FailedReloadAsync(FeatureTestHost host)
    {
        var editor = await OpenAsync(host, "manual-failed-reload.txt", Encoding.UTF8.GetBytes("abc"));
        ReplaceFirst(editor, "b");
        ReplaceFirst(editor, "a");
        var entered = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var release = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        editor.TestComparisonPause = _ => { entered.TrySetResult(); return release.Task; };
        var comparing = editor.TestCompareContentAsync();
        using var releaseLoad = new ManualResetEventSlim();
        var loading = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var originalBaseline = editor.LastSavedSnapshot.Baseline.File.Path;
        Task reload = Task.CompletedTask;
        try
        {
            await entered.Task.WaitAsync(TimeSpan.FromSeconds(10));
            SessionRecoveryCancellationHooks.LeasedRead = (path, _, _) =>
            {
                if (path == originalBaseline) return;
                if (editor.Dispatcher.HasThreadAccess) throw new InvalidOperationException("Native load must read on a worker thread.");
                loading.TrySetResult();
                if (!releaseLoad.Wait(TimeSpan.FromSeconds(10))) throw new TimeoutException("The reload fixture did not release I/O.");
                throw new IOException("Injected native load read failure.");
            };
            reload = editor.ReloadFromEditingFileAsync();
            await loading.Task.WaitAsync(TimeSpan.FromSeconds(10));
            release.TrySetResult();
            await comparing;
            Check(editor.IsModified, "An in-flight reload prevents publication of the old equality result.");
            releaseLoad.Set();
            bool failed = false;
            try { await reload; }
            catch (Exception) { failed = true; } // The real native read projects the I/O HRESULT.
            Check(failed && editor.GetText() == "abc", "Failed native reload preserves the original live document.");
            await UntilAsync(() => !editor.IsModified,
                "After a failed reload, the normal debounce must resume equality checking without another edit or forced comparison.");
        }
        finally
        {
            SessionRecoveryCancellationHooks.Reset();
            editor.TestComparisonPause = null;
            release.TrySetResult();
            releaseLoad.Set();
            await comparing;
            try { await reload; } catch (Exception) { }
        }
    }

    private static async Task DisposalAsync(FeatureTestHost host)
    {
        var editor = await OpenAsync(host, "manual-dispose.txt", Encoding.UTF8.GetBytes("abc"));
        ReplaceFirst(editor, "b");
        ReplaceFirst(editor, "a");
        var entered = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var release = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        editor.TestComparisonPause = offset => { entered.TrySetResult(); return release.Task; };
        var comparing = editor.TestCompareContentAsync();
        Task superseding = Task.CompletedTask;
        try
        {
            await entered.Task.WaitAsync(TimeSpan.FromSeconds(10));
            ReplaceFirst(editor, "b");
            ReplaceFirst(editor, "a");
            superseding = editor.TestCompareContentAsync();
            host.Core.DeleteTab(host.Core.GetTab(editor));
            Check(!editor.DisposalCompletion.IsCompleted, "Disposal waits for in-flight comparison to release its baseline reader.");
        }
        finally { release.TrySetResult(); await Task.WhenAll(comparing, superseding); }
        await editor.DisposalCompletion.WaitAsync(TimeSpan.FromSeconds(10));
        Check(editor.TestComparisonBufferSizes.Count == 1 && editor.TestComparedChunks.Count == 1,
            "Disposal cancels queued comparisons before they allocate buffers or read a released native document.");
    }
}
