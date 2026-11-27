// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Documents.IO;
using Notepads.Infrastructure.Storage;
using Notepads.Presentation.Controls.Dialog;
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
        await ConcurrentFileObservationsAsync(host, log);
        await EmptyAndMetadataAsync(host);
        await SaveDuringFileObservationsAsync(host, log);
        var statusFailures = new List<Exception>();
        async Task RunStatusCaseAsync(string name, Func<Task> run)
        {
            try { await run(); }
            catch (Exception error)
            {
                log.AppendLine($"FAIL: {name}: {error}");
                statusFailures.Add(new InvalidOperationException(name, error));
            }
        }
        await RunStatusCaseAsync("Status check after owned commit", () => SuppressOwnedSaveStatusAsync(host, log));
        await RunStatusCaseAsync("Earlier status observation during save", () => SuppressInFlightStatusDuringSaveAsync(host, log));
        await RunStatusCaseAsync("Unknown saved version after conflict", () => PreserveUnknownVersionConflictAsync(host, log));
        if (statusFailures.Count != 0) throw new AggregateException("File-status regressions failed.", statusFailures);
        await EqualityUndoRecoveryCloseAsync(host);
        await StreamingAndRacesAsync(host, log);
        await SupersessionAndReloadAsync(host);
        await FailedReloadAsync(host);
        await DisposalAsync(host);
        log.AppendLine("PASS: exact saved-content equality clears manual reversions without moving undo save points; empty/metadata, UI, close, recovery, cancellation and disposal contracts hold.");
    }

    private static async Task ConcurrentFileObservationsAsync(FeatureTestHost host, StringBuilder log)
    {
        var bytes = Encoding.UTF8.GetBytes(string.Concat(Enumerable.Repeat("Ω😀 observation\r\n", 65536)));
        var file = await host.Folder.CreateFileAsync("concurrent-observation.txt");
        await FileIO.WriteBytesAsync(file, bytes);
        var digest = Convert.ToHexString(SHA256.HashData(bytes));
        const int iterations = 128;
        static void Require(bool condition, string message)
        {
            if (!condition) throw new InvalidOperationException(message);
        }
        for (var phase = 0; phase < 2; phase++)
        {
            using var stopPolling = new CancellationTokenSource();
            var pollCount = 0;
            var pinned = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
            var pollingStarted = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
            DocumentSaveTarget.TestVersionRead = (target, _) =>
            {
                Require(ReferenceEquals(file, target), "The observation test must use the same StorageFile grant.");
                pinned.TrySetResult();
                return phase == 0 ? Task.CompletedTask : pollingStarted.Task;
            };
            var poller = phase == 0 ? Task.CompletedTask : Task.Run(async () =>
            {
                await pinned.Task;
                while (!stopPolling.IsCancellationRequested)
                {
                    var metadata = FileStorage.GetDateModifiedAsync(file);
                    pollingStarted.TrySetResult();
                    Require(await metadata > 0, "Concurrent status metadata must remain readable.");
                    // Use the same open as FileExistsAsync, without its error-swallowing policy.
                    using var reader = await FileStorage.OpenStreamForReadAsync(file);
                    Require(reader.Length == bytes.Length, "Concurrent existence probes must observe the full file.");
                    var parent = await FileStorage.GetParentAsync(file);
                    Require(parent.Path == host.Folder.Path, "Concurrent relative-path lookup must preserve the document grant.");
                    pollCount++;
                }
            });
            Exception readError = null;
            try
            {
                for (var i = 0; i < iterations; i++)
                {
                    var observed = await DocumentSaveTarget.ReadVersionAsync(file);
                    Require(observed.Sha256 == digest, "Every stable version observation must verify the exact bytes.");
                }
            }
            catch (Exception ex) { readError = ex; throw; }
            finally
            {
                DocumentSaveTarget.TestVersionRead = null;
                stopPolling.Cancel();
                pinned.TrySetResult();
                pollingStarted.TrySetResult();
                try { await poller; }
                catch (Exception ex) when (readError != null) { throw new AggregateException(readError, ex); }
            }
            Check(phase == 0 || pollCount > 0, "The concurrent observation phase must execute real metadata/existence probes.");
            log.AppendLine($"PASS: stable observation phase={phase}, versionReads={iterations}, statusProbes={pollCount}.");
        }
        log.AppendLine("PASS: 128 stable version reads with and without concurrent status metadata/existence probes preserve exact hashes on the same StorageFile grant (bounded stress evidence).");
    }

    private static async Task SaveDuringFileObservationsAsync(FeatureTestHost host, StringBuilder log)
    {
        var editor = await OpenAsync(host, "save-with-status-probes.txt", "base"u8.ToArray());
        editor.TypeText("draft Ω😀");
        var expected = editor.GetEncoding().GetBytes(editor.GetText());
        var checks = new ConcurrentQueue<Task>();
        async Task CheckStatusAsync()
        {
            await Task.Yield();
            await editor.TestCheckFileStatusAsync();
        }
        Task StartStatusCheck()
        {
            var check = CheckStatusAsync();
            checks.Enqueue(check);
            return check;
        }
        editor.TestSaveReaderCaptured = StartStatusCheck;
        DocumentFileWriter.TestLocalCommit = file => { _ = StartStatusCheck(); };
        DocumentSaveTarget.TestVersionRead = (_, _) => StartStatusCheck();
        Task<bool> saving = null;
        Exception saveError = null;
        try
        {
            saving = host.Page.TestSaveDocumentAsync(editor);
            await UntilAsync(() => saving.IsCompleted || DialogManager.ActiveDialog is FileSaveErrorDialog,
                "Concurrent status checks must not turn a successful Save into a confirmation dialog.");
            Check(saving.IsCompleted && await saving, "Save must complete without a confirmation/error dialog during status probes.");
            await Task.WhenAll(checks).WaitAsync(TimeSpan.FromSeconds(20));
            Check(checks.Count == 3 && !editor.IsModified && editor.FileModificationState == FileModificationState.Untouched,
                "Status requests during capture, commit and verification must preserve the published clean save state.");
            await BytesAsync(editor.EditingFile, expected, "Status probes preserve the exact committed draft bytes");
        }
        catch (Exception ex) { saveError = ex; throw; }
        finally
        {
            var cleanupErrors = new List<Exception>();
            if (saving != null)
            {
                try
                {
                    await UntilAsync(() => saving.IsCompleted || ActiveVisibleDialog() is FileSaveErrorDialog,
                        "The status-probe save must settle before its hooks are removed.");
                    if (!saving.IsCompleted) await ChooseDialogAsync(saving, "CloseButton");
                    await saving.WaitAsync(TimeSpan.FromSeconds(20));
                }
                catch (Exception ex)
                {
                    if (!ReferenceEquals(ex, saveError)) cleanupErrors.Add(ex);
                }
            }
            editor.TestSaveReaderCaptured = null;
            DocumentFileWriter.TestLocalCommit = null;
            DocumentSaveTarget.TestVersionRead = null;
            try { await Task.WhenAll(checks).WaitAsync(TimeSpan.FromSeconds(20)); }
            catch (Exception ex)
            {
                if (!ReferenceEquals(ex, saveError)) cleanupErrors.Add(ex);
            }
            if (cleanupErrors.Count > 0)
            {
                if (saveError != null) cleanupErrors.Insert(0, saveError);
                throw new AggregateException(cleanupErrors);
            }
        }
        log.AppendLine("PASS: actual MainPage Save verifies exact bytes without an error dialog when status checks are requested during capture, commit and verification; an active owned save may suppress those checks.");
    }

    private static async Task SuppressOwnedSaveStatusAsync(FeatureTestHost host, StringBuilder log)
    {
        var editor = await OpenAsync(host, "owned-save-status.txt", "base"u8.ToArray());
        var file = editor.EditingFile;
        var savedTime = editor.LastSavedSnapshot.DateModifiedFileTime;
        editor.TypeText("draft");
        var expected = editor.GetEncoding().GetBytes(editor.GetText());
        var entered = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var release = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var observedStates = new List<FileModificationState>();
        void Observe(object sender, EventArgs args) => observedStates.Add(editor.FileModificationState);
        editor.FileModificationStateChanged += Observe;
        DocumentFileWriter.TestLocalCommit = target =>
        {
            Check(ReferenceEquals(target, file), "The owned-save fixture must commit its own bound grant.");
            File.SetLastWriteTimeUtc(target.Path, DateTime.FromFileTimeUtc(savedTime).AddMinutes(1));
        };
        DocumentSaveTarget.TestVersionRead = (target, token) =>
        {
            Check(ReferenceEquals(target, file), "Pause verification only for the owned-save fixture.");
            entered.TrySetResult();
            return release.Task.WaitAsync(token);
        };
        Task saving = null;
        try
        {
            saving = editor.SaveContentToFileAndUpdateEditorStateAsync(file);
            await entered.Task.WaitAsync(TimeSpan.FromSeconds(15));
            Check(editor.IsFileSaveActive && !saving.IsCompleted &&
                await FileStorage.GetDateModifiedAsync(file) != savedTime,
                "The status check must run after commit with a changed timestamp and before save publication.");
            await editor.TestCheckFileStatusAsync().WaitAsync(TimeSpan.FromSeconds(15));
            Check(editor.FileModificationState == FileModificationState.Untouched &&
                !observedStates.Contains(FileModificationState.Modified),
                "An owned save must not publish an external-modification state or warning while its commit is being verified.");
        }
        finally
        {
            release.TrySetResult();
            try { if (saving != null) await saving.WaitAsync(TimeSpan.FromSeconds(20)); }
            finally
            {
                DocumentFileWriter.TestLocalCommit = null;
                DocumentSaveTarget.TestVersionRead = null;
                editor.FileModificationStateChanged -= Observe;
                await editor.TestStopFileChecksAsync();
            }
        }
        Check(!editor.IsModified && editor.FileModificationState == FileModificationState.Untouched,
            "A verified owned save must finish with the correct clean file status.");
        await BytesAsync(file, expected, "Suppressing an owned-save status check must preserve the committed draft");
        log.AppendLine("PASS: a status check after the owned local commit emits no external-modification state or warning before publication.");
    }

    private static async Task SuppressInFlightStatusDuringSaveAsync(FeatureTestHost host, StringBuilder log)
    {
        var editor = await OpenAsync(host, "in-flight-save-status.txt", "base"u8.ToArray());
        var file = editor.EditingFile;
        editor.TypeText("draft");
        var expected = editor.GetEncoding().GetBytes(editor.GetText());
        // Give the poller a definite timestamp mismatch without changing the bytes
        // that the following transaction must verify.
        File.SetLastWriteTimeUtc(file.Path,
            DateTime.FromFileTimeUtc(editor.LastSavedSnapshot.DateModifiedFileTime).AddMinutes(1));
        var entered = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var release = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var observedStates = new List<FileModificationState>();
        void Observe(object sender, EventArgs args) => observedStates.Add(editor.FileModificationState);
        editor.FileModificationStateChanged += Observe;
        editor.TestResolvingSaveTarget = () => { entered.TrySetResult(); return release.Task; };
        Task checking = null;
        Task saving = null;
        try
        {
            // Both callbacks use Normal priority. The poll captures first; save
            // admission is already queued before the poll can publish its result.
            checking = editor.TestCheckFileStatusAsync();
            await editor.Dispatcher.RunAsync(Windows.UI.Core.CoreDispatcherPriority.Normal,
                () => saving = editor.SaveContentToFileAndUpdateEditorStateAsync(file));
            await entered.Task.WaitAsync(TimeSpan.FromSeconds(15));
            Check(editor.IsFileSaveActive && saving != null && !saving.IsCompleted,
                "The earlier status observation must finish while save admission is held.");
            await checking.WaitAsync(TimeSpan.FromSeconds(15));
            Check(editor.FileModificationState == FileModificationState.Untouched &&
                !observedStates.Contains(FileModificationState.Modified),
                "A status check captured before save admission must not publish an external-modification warning during the save.");
        }
        finally
        {
            release.TrySetResult();
            try
            {
                await Task.WhenAll(new[] { saving, checking }.Where(work => work != null))
                    .WaitAsync(TimeSpan.FromSeconds(20));
            }
            finally
            {
                editor.TestResolvingSaveTarget = null;
                editor.FileModificationStateChanged -= Observe;
                await editor.TestStopFileChecksAsync();
            }
        }
        await BytesAsync(file, expected, "An earlier status observation must not interfere with the admitted save");
        log.AppendLine("PASS: a status check captured before save admission cannot publish a stale warning while the save is active.");
    }

    private static async Task PreserveUnknownVersionConflictAsync(FeatureTestHost host, StringBuilder log)
    {
        var editor = await OpenAsync(host, "unknown-version-conflict.txt", "base"u8.ToArray());
        var file = editor.EditingFile;
        editor.TypeText("draft");
        var draft = editor.GetText();
        editor.TestMakeSavedFileVersionUnknown();
        await editor.TestCheckFileStatusAsync();
        Check(editor.FileModificationState == FileModificationState.Unverified,
            "The legacy-style fixture must begin without a verified saved file version.");
        var snapshot = editor.LastSavedSnapshot;
        var picked = await editor.ReadPickedFileVersionAsync(file, default);
        await FileIO.WriteBytesAsync(file, "external"u8.ToArray());
        try
        {
            await editor.SaveContentToFileAndUpdateEditorStateAsync(file, picked);
            throw new InvalidOperationException("The picked destination changed before its transaction and must conflict.");
        }
        catch (DocumentSaveException error) when (error.Stage == DocumentSaveStage.Conflict) { }
        Check(editor.FileModificationState == FileModificationState.Modified,
            "A proven picked-save conflict must initially report an external modification.");
        await editor.TestCheckFileStatusAsync();
        Check(editor.FileModificationState == FileModificationState.Modified && editor.IsModified &&
            editor.GetText() == draft && ReferenceEquals(editor.LastSavedSnapshot, snapshot),
            "Polling must preserve a proven file conflict even when the saved snapshot has no historical fingerprint.");
        await BytesAsync(file, "external"u8.ToArray(), "Conflict status must preserve the external writer's bytes");
        log.AppendLine("PASS: a legacy-style unknown saved version retains its proven external-conflict status across polling without changing either draft or disk.");
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
