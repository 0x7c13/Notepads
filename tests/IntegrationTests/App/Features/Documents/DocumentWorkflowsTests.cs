// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Documents.IO;
using Notepads.Features.Sessions;
using Notepads.Features.Sessions.Storage;
using Notepads.Infrastructure.Storage;
using Notepads.Presentation.Controls.Dialog;
using Notepads.Presentation.Controls.TextEditor;
using NotepadsIntegrationTests;
using Windows.Storage;
using static NotepadsWorkspaceTests.DocumentFeatureAssertions;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsWorkspaceTests;

internal static class DocumentWorkflowsTests
{
    internal static async Task RunAsync(StringBuilder log)
    {
        await ContentModificationTests.RunAsync(log);
        await SaveCloseRaceTests.RunAsync(log);
        await using var host = await FeatureTestHost.CreateAsync();
        await EncodingMenuTests.RunAsync(host, log);
        await PendingSaveRegressionTests.RunAsync(host, log);
        await EncodingAndLineEndingsAsync(host, log);
        await EditDuringSaveAsync(host, log);
        await RecoverUndoDuringSaveAsync(host, log);
        await DeleteUnreadRevisionAsync(host, log);
        await FailedAndCanceledSaveAsync(host, log);
        await SaveAsAndSaveAllAsync(host, log);
        await UnrelatedSaveAsReceiptAsync(host, log);
        await SaveCollisionAsync(host, log);
        await ConcurrentSavesAsync(host, log);
        await IndependentSaveTargetsAsync(host, log);
        await DeletePendingSaveLockAsync(host, log);
        await UnverifiedVersionAsync(host, log);
        await PickedTargetChangedAsync(host, log);
        await ParentDirectoryCasingTests.RunAsync(host, log);
        await RetargetedSaveTests.RunAsync(host, log);
        await SaveIntentTests.RunAsync(host, log);
        await BrokeredDocumentTests.RunAsync(host, log);
        await EquivalentBindingDuringSaveAsAsync(host, log);
        foreach (var action in Enum.GetValues<VerificationAction>())
            await RetryCommittedVerificationAsync(host, log, action);
        await VerifyProviderRenameAsync(host, log, cancel: false);
        await VerifyProviderRenameAsync(host, log, cancel: true);
        foreach (var action in Enum.GetValues<ProviderRenameAction>())
            await ProviderRenameThroughAliasAsync(host, log, action);
        await VerifySavedSettingsTransferAsync(host, log);
        await SameTimestampConflictAsync(host, log);
        await SupersededCloseAsync(host, log, keepWriterOpen: false);
        await SupersededCloseAsync(host, log, keepWriterOpen: true);
        await DiscardDuringPickedReadAsync(host, log);
        await ReloadRevertRenameAndShareAsync(host, log);
        await RenameWithUnavailableMetadataAsync(host, log);
        await ExternalFileSignalsAsync(host, log);
        await DisposeDuringVerificationPromptAsync(host, log);
    }

    private static async Task EncodingAndLineEndingsAsync(FeatureTestHost host, StringBuilder log)
    {
        var editor = await OpenAsync(host, "encoding.txt", [0xef, 0xbb, 0xbf, 0x41, 0x0d, 0x0a, 0xce, 0xa9, 0x00]);
        Check(editor.GetText() == "A\rΩ\0" && editor.GetLineEnding() == LineEnding.Crlf && !editor.IsModified,
            "Open must decode BOM/Unicode/NUL and normalize text while retaining file EOL metadata.");
        var originalEncoding = editor.GetEncoding();
        Check(editor.TryChangeEncoding(new UnicodeEncoding(false, true, true)) && editor.IsModified,
            "Changing only encoding must mark the document modified.");
        Check(editor.TryChangeEncoding(originalEncoding) && !editor.IsModified,
            "Returning to the saved encoding must clear an encoding-only modification.");
        Check(editor.TryChangeLineEnding(LineEnding.Lf) && editor.IsModified &&
            editor.TryChangeLineEnding(LineEnding.Crlf) && !editor.IsModified,
            "Returning to the saved EOL must clear an EOL-only modification.");

        var saved = 0;
        editor.FileSaved += (_, _) => saved++;
        editor.TryChangeEncoding(new UnicodeEncoding(false, true, true));
        Check(await host.Page.TestSaveDocumentAsync(editor), "MainPage Save must persist an encoding-only edit.");
        await BytesAsync(editor.EditingFile, [0xff, 0xfe, 0x41, 0x00, 0x0d, 0x00, 0x0a, 0x00, 0xa9, 0x03, 0x00, 0x00],
            "Encoding-only save must write exactly UTF-16 LE BOM, CRLF, Unicode and NUL");
        Check(!editor.IsModified && editor.RequestedEncoding == null && saved == 1,
            "Successful encoding-only save must adopt the new baseline and fire one saved event.");
        editor.TryChangeLineEnding(LineEnding.Lf);
        Check(await host.Page.TestSaveDocumentAsync(editor), "MainPage Save must persist an EOL-only edit.");
        await BytesAsync(editor.EditingFile, [0xff, 0xfe, 0x41, 0x00, 0x0a, 0x00, 0xa9, 0x03, 0x00, 0x00],
            "EOL-only save must write exact LF bytes with the already saved encoding");
        Check(editor.GetText() == "A\rΩ\0" && !editor.IsModified && editor.RequestedLineEnding == null && saved == 2,
            "Metadata-only saves must preserve canonical editor text and establish a clean save point.");
        await editor.TestStopFileChecksAsync();
        log.AppendLine("PASS: documents/open-save handles BOM, Unicode, NUL and encoding/EOL-only changes with exact-byte oracles.");
    }

    private static async Task EditDuringSaveAsync(FeatureTestHost host, StringBuilder log)
    {
        var text = new string('a', 65534) + "😀\0" + new string('b', 65536);
        var editor = await OpenAsync(host, "editable-save.txt", [.. Encoding.UTF8.GetPreamble(), .. Encoding.UTF8.GetBytes(text)]);
        editor.TestNativeCore.SetTextSelectionPosition(0, 0);
        editor.TypeText("!");
        var captured = editor.GetText();
        var before = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var resumeBefore = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var after = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var resumeAfter = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        editor.TestSaveReaderCaptured = () =>
        {
            editor.TypeText("during capture");
            return Task.CompletedTask;
        };
        DocumentSaveTarget.TestOriginalVersionRead = (_, token) => { before.TrySetResult(); return resumeBefore.Task.WaitAsync(token); };
        DocumentSaveTarget.TestVersionRead = (_, token) => { after.TrySetResult(); return resumeAfter.Task.WaitAsync(token); };
        try
        {
            var save = editor.SaveContentToFileAndUpdateEditorStateAsync(editor.EditingFile);
            await before.Task.WaitAsync(TimeSpan.FromSeconds(15));
            editor.TypeText("before commit");
            resumeBefore.SetResult();
            await after.Task.WaitAsync(TimeSpan.FromSeconds(15));
            editor.TypeText("after commit");
            editor.TryChangeEncoding(Encoding.Unicode);
            resumeAfter.SetResult();
            await save.WaitAsync(TimeSpan.FromSeconds(15));
            await BytesAsync(editor.EditingFile, [.. Encoding.UTF8.GetPreamble(), .. Encoding.UTF8.GetBytes(captured)], "Only the captured revision reaches disk");
            Check(editor.GetText().StartsWith("!during capturebefore commitafter commit", StringComparison.Ordinal) &&
                editor.IsModified && editor.GetEncoding().CodePage == Encoding.Unicode.CodePage &&
                editor.LastSavedSnapshot.Encoding.CodePage == Encoding.UTF8.CodePage,
                "Typing during capture and both verification stages, and a later encoding choice, must survive publication.");
            using var recovery = editor.CaptureRecoveryState();
            Check(recovery.MetaData.IsModified && recovery.Checkpoint.CommittedSequence > recovery.Checkpoint.BaseSequence,
                "The live edits must remain available to crash recovery.");
        }
        finally
        {
            resumeBefore.TrySetResult();
            resumeAfter.TrySetResult();
            editor.TestSaveReaderCaptured = null;
            DocumentSaveTarget.TestOriginalVersionRead = null;
            DocumentSaveTarget.TestVersionRead = null;
        }
        log.AppendLine("PASS: edits during capture, pre-commit and post-commit verification survive in the tab and recovery; disk contains the captured bytes.");
    }

    private static async Task RecoverUndoDuringSaveAsync(FeatureTestHost host, StringBuilder log)
    {
        var editor = await OpenAsync(host, "checkpoint-during-save.txt", "base"u8.ToArray());
        editor.TypeText("draft");
        var written = editor.GetText();
        var captured = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var resumeCapture = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var committed = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var resumeVerification = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        editor.TestResolvingSaveTarget = () => { captured.SetResult(); return resumeCapture.Task; };
        DocumentSaveTarget.TestVersionRead = (_, token) => { committed.TrySetResult(); return resumeVerification.Task.WaitAsync(token); };
        var controller = host.Page.TestController;
        var backupEnabled = controller.IsBackupEnabled;
        controller.IsBackupEnabled = true;
        var save = editor.SaveContentToFileAndUpdateEditorStateAsync(editor.EditingFile);
        try
        {
            await captured.Task.WaitAsync(TimeSpan.FromSeconds(15));
            editor.TestNativeCore.Undo();
            Check(editor.GetText() == "base" && !editor.IsModified, "Undo during target lookup must reach the old save point.");
            Check(await controller.SaveSessionAsync().WaitAsync(TimeSpan.FromSeconds(15)), "A checkpoint can publish while target lookup is pending.");
            var owner = host.Core.DocumentOwnerId;
            using var plan = await host.Page.TestDiscoverRecoveryAsync();
            var source = plan.Sources.Single(item => item.Id == editor.Id);
            resumeCapture.SetResult();
            await committed.Task.WaitAsync(TimeSpan.FromSeconds(15));
            var metadata = editor.GetTextEditorStateMetaData();
            Check(metadata.IsModified && metadata.HasUnconfirmedFileSave, "A checkpoint after commit still protects the undone revision.");
            resumeVerification.SetResult();
            await save.WaitAsync(TimeSpan.FromSeconds(15));
            await BytesAsync(editor.EditingFile, editor.GetEncoding().GetBytes(written), "Disk holds the saved revision after undo");
            using var prepared = await RecoveryDocumentPreparation.PrepareAsync(source, owner, new DocumentLoadOptions(), CancellationToken.None);
            Check(prepared.EditingFile?.IsEqual(editor.EditingFile) == true && !prepared.InitializeFromFile &&
                prepared.Checkpoint != null, "Restart must use the captured journal even when the committed file opens successfully.");
            var restored = (TextEditor)host.Core.CreateTextEditor(Guid.NewGuid(), prepared.EditingFile, "Recovered");
            try
            {
                await restored.RestoreV2Async(prepared.SavedSnapshot, prepared.RecoveryBaseline, prepared.Journal,
                    prepared.Checkpoint, prepared.EditingFile, prepared.Metadata, prepared.TextDirty);
                Check(restored.GetText() == "base" && restored.IsModified, "Restart must restore the user's undo as a protected draft.");
            }
            finally { await host.Core.DiscardUnattachedEditorAsync(restored); }
            Check(!editor.GetTextEditorStateMetaData().HasUnconfirmedFileSave, "A verified save must clear the in-flight recovery marker.");
        }
        finally
        {
            resumeCapture.TrySetResult();
            resumeVerification.TrySetResult();
            editor.TestResolvingSaveTarget = null;
            DocumentSaveTarget.TestVersionRead = null;
            controller.IsBackupEnabled = backupEnabled;
            await save.WaitAsync(TimeSpan.FromSeconds(15));
        }
        editor.TypeText("cancel");
        using var cancel = new CancellationTokenSource();
        editor.TestSaveReaderCaptured = () => { cancel.Cancel(); return Task.CompletedTask; };
        try
        {
            await ThrowsAsync<OperationCanceledException>(() => editor.TestSaveWithCancellationAsync(editor.EditingFile, cancel.Token),
                "Cancellation during revision capture must stop the save.");
            Check(!editor.GetTextEditorStateMetaData().HasUnconfirmedFileSave, "A cancelled save must clear the in-flight recovery marker.");
        }
        finally { editor.TestSaveReaderCaptured = null; }
        controller.IsBackupEnabled = true;
        try
        {
            Check(await controller.SaveSessionAsync(), "Publish a new checkpoint after the cancelled write.");
            using var plan = await host.Page.TestDiscoverRecoveryAsync();
            Check(!plan.Sources.Single(item => item.Id == editor.Id).Metadata.HasUnconfirmedFileSave,
                "The next checkpoint must clear a transient save marker without another edit.");
        }
        finally { controller.IsBackupEnabled = backupEnabled; }
        log.AppendLine("PASS: slow target lookup saves the revision at admission; a checkpoint restores a later undo, and success and cancellation release the marker.");
    }

    private static async Task DeleteUnreadRevisionAsync(FeatureTestHost host, StringBuilder log)
    {
        var bytes = new byte[16 * 1024 * 1024 + 1];
        Array.Fill(bytes, (byte)'a');
        for (var i = 511; i < bytes.Length; i += 512) bytes[i] = (byte)'\n';
        var editor = await OpenAsync(host, "snapshot-large-delete.txt", bytes);
        editor.TestNativeCore.SetTextSelectionPosition(0, 0);
        editor.TypeText("!");
        using var expected = IncrementalHash.CreateHash(HashAlgorithmName.SHA256);
        expected.AppendData("!"u8);
        expected.AppendData(bytes);
        var hash = Convert.ToHexString(expected.GetHashAndReset());
        editor.TestSaveReaderCaptured = () =>
        {
            editor.TestNativeCore.SelectAll();
            editor.TypeText("later replacement");
            return Task.CompletedTask;
        };
        try
        {
            await editor.SaveContentToFileAndUpdateEditorStateAsync(editor.EditingFile);
            Check(editor.LastSavedSnapshot.FileVersion.Sha256 == hash && editor.GetText() == "later replacement" && editor.IsModified,
                "A deletion beyond the cursor budget must preserve the original revision on disk and the newer live draft.");
        }
        finally { editor.TestSaveReaderCaptured = null; }
        log.AppendLine("PASS: a concurrent deletion exceeding 16 MiB replays the original journal prefix without locking input.");
    }

    private static async Task FailedAndCanceledSaveAsync(FeatureTestHost host, StringBuilder log)
    {
        var editor = await OpenAsync(host, "preserve.txt", [0xef, 0xbb, 0xbf, 0x6f, 0x6c, 0x64]);
        var file = editor.EditingFile;
        var baseline = editor.LastSavedSnapshot;
        editor.TestNativeCore.SetTextSelectionPosition(0, 3);
        editor.TypeText("new Ω");
        editor.TestNativeCore.SetTextSelectionPosition(1, 3);
        editor.TryChangeEncoding(Encoding.GetEncoding(20127, EncoderFallback.ExceptionFallback, DecoderFallback.ExceptionFallback));
        var saved = 0;
        editor.FileSaved += (_, _) => saved++;
        await ThrowsAsync<EncoderFallbackException>(() => editor.SaveContentToFileAndUpdateEditorStateAsync(file),
            "An unrepresentable strict ASCII save must fail instead of losing Unicode.");
        await BytesAsync(file, [0xef, 0xbb, 0xbf, 0x6f, 0x6c, 0x64], "Failed save must preserve original disk bytes");
        Check(editor.GetText() == "new Ω" && editor.IsModified && ReferenceEquals(baseline, editor.LastSavedSnapshot) &&
            editor.GetEncoding().CodePage == 20127 && editor.EditingFile.IsEqual(file) && saved == 0,
            "Failed save must preserve pending content, metadata, baseline and file identity without reporting success.");
        Selection(editor, 1, 3, "Failed save must preserve selection");
        editor.TryChangeEncoding(new UTF8Encoding(true));

        var admitted = new TaskCompletionSource<object>(TaskCreationOptions.RunContinuationsAsynchronously);
        var release = new TaskCompletionSource<object>(TaskCreationOptions.RunContinuationsAsynchronously);
        var holding = editor.TestHoldDocumentOperationAsync(async () => { admitted.TrySetResult(null); await release.Task; });
        await admitted.Task;
        using var cancellation = new CancellationTokenSource();
        var canceled = editor.TestSaveWithCancellationAsync(file, cancellation.Token);
        try
        {
            Check(!canceled.IsCompleted, "The document coordinator must queue save behind admitted work.");
            cancellation.Cancel();
            await ThrowsAsync<OperationCanceledException>(() => canceled, "A canceled queued save must report cancellation.");
            Check(editor.GetText() == "new Ω" && editor.IsModified && ReferenceEquals(baseline, editor.LastSavedSnapshot) && saved == 0,
                "Canceling queued save must leave content and baseline intact.");
            Selection(editor, 1, 3, "Canceled save must preserve selection");
            await BytesAsync(file, [0xef, 0xbb, 0xbf, 0x6f, 0x6c, 0x64], "Canceled queued save must leave disk unchanged");

            var queuedSave = editor.SaveContentToFileAndUpdateEditorStateAsync(file);
            editor.TestNativeCore.SetTextSelectionPosition(0, 5);
            editor.TypeText("latest");
            editor.TryChangeLineEnding(LineEnding.Lf);
            release.TrySetResult(null);
            await holding;
            await queuedSave;
            await BytesAsync(file, [0xef, 0xbb, 0xbf, 0x6c, 0x61, 0x74, 0x65, 0x73, 0x74],
                "Queued save must capture edits made before its coordinator admission");
            Check(editor.GetText() == "latest" && !editor.IsModified && saved == 1,
                "A successful retry must clear dirty state only for the actually saved revision.");
        }
        finally { release.TrySetResult(null); await holding; }
        await editor.TestStopFileChecksAsync();
        log.AppendLine("PASS: documents/save-failure and queued-cancellation preserve content, selection and disk; queued save captures intervening edits.");
    }

    private static async Task ConcurrentSavesAsync(FeatureTestHost host, StringBuilder log)
    {
        var left = await OpenAsync(host, "parallel.txt", "base"u8.ToArray());
        var file = left.EditingFile;
        var right = (TextEditor)await host.Core.CreateTextEditorAsync(Guid.NewGuid(), file);
        host.Core.OpenTextEditor(right);
        left.TypeText("left");
        right.TypeText("right");

        // Hold their target until both real saves have observed contention. Without
        // target coordination this gate is never observed and the test fails.
        Task leftSave, rightSave;
        using (await DocumentSaveTarget.AcquireAsync(file, CancellationToken.None))
        {
            var waiting = 0;
            var bothWaiting = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
            FileLockLease.TestLeaseWait = path =>
            {
                if (Path.GetFileName(Path.GetDirectoryName(path)) == "DocumentSaveLocks" &&
                    Interlocked.Increment(ref waiting) == 2) bothWaiting.TrySetResult();
            };
            try
            {
                leftSave = left.SaveContentToFileAndUpdateEditorStateAsync(file);
                rightSave = right.SaveContentToFileAndUpdateEditorStateAsync(right.EditingFile);
                await bothWaiting.Task.WaitAsync(TimeSpan.FromSeconds(4));
            }
            finally { FileLockLease.TestLeaseWait = null; }
        }

        try { await Task.WhenAll(leftSave, rightSave); }
        catch (DocumentSaveException ex) when (ex.Stage == DocumentSaveStage.Conflict) { }
        Check(leftSave.IsCompletedSuccessfully != rightSave.IsCompletedSuccessfully, "Exactly one competing editor commits.");
        var winner = leftSave.IsCompletedSuccessfully ? left : right;
        var loser = leftSave.IsCompletedSuccessfully ? right : left;
        Check(!winner.IsModified && loser.IsModified, "The competing revision stays dirty.");
        await BytesAsync(file, Encoding.UTF8.GetBytes(winner.GetText()), "The successful editor owns the bytes on disk");
        log.AppendLine("PASS: two admitted saves on one target serialize and reject the stale revision.");
    }

    private static async Task IndependentSaveTargetsAsync(FeatureTestHost host, StringBuilder log)
    {
        var seen = new Dictionary<byte, string>();
        for (var i = 0; i <= byte.MaxValue + 1; i++)
        {
            var name = $"independent-{i}.txt";
            var path = Path.Combine(host.Folder.Path, name).ToUpperInvariant();
            var oldStripe = SHA256.HashData(Encoding.UTF8.GetBytes(path))[0];
            if (seen.TryAdd(oldStripe, name)) continue;

            var first = await host.Folder.CreateFileAsync(seen[oldStripe]);
            var second = await OpenAsync(host, name, "base"u8.ToArray());
            using (await DocumentSaveTarget.AcquireAsync(first, CancellationToken.None))
            {
                second.TypeText("other");
                await second.SaveContentToFileAndUpdateEditorStateAsync(second.EditingFile);
                await BytesAsync(second.EditingFile, Encoding.UTF8.GetBytes(second.GetText()), "independent save");
            }
            log.AppendLine("PASS: files colliding under the old 256-stripe key can save independently.");
            return;
        }
        throw new InvalidOperationException("257 paths must include a stripe collision.");
    }

    private static async Task DeletePendingSaveLockAsync(FeatureTestHost host, StringBuilder log)
    {
        var path = Path.Combine(host.Folder.Path, "delete-pending.lock");
        var timeout = TimeSpan.FromSeconds(5);
        using var first = await FileLockLease.AcquireTemporaryExclusiveAsync(path, timeout, CancellationToken.None);
        using var metadata = DocumentSaveTarget.TestPinLockMetadata(path);
        Check(!metadata.IsInvalid, "A metadata-only handle must pin the temporary lock name.");
        first.Dispose();

        var waiting = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        FileLockLease.TestLeaseWait = observed => { if (observed == path) waiting.TrySetResult(); };
        try
        {
            var next = FileLockLease.AcquireTemporaryExclusiveAsync(path, timeout, CancellationToken.None);
            try { await waiting.Task.WaitAsync(timeout); }
            finally { metadata.Dispose(); }
            using var acquired = await next;
        }
        finally { FileLockLease.TestLeaseWait = null; }
        Check(!File.Exists(path), "The final lock handle removes the temporary name.");

        // This name yields the same Windows access denial while File.Exists is already false.
        // Remove it at the retry boundary, just as a pending delete can finish after open fails.
        Directory.CreateDirectory(path);
        try
        {
            Check(!File.Exists(path), "A denied open can have no visible lock file.");
            await ThrowsAsync<UnauthorizedAccessException>(
                () => FileLockLease.AcquireTemporaryExclusiveAsync(path, TimeSpan.Zero, CancellationToken.None),
                "A persistent denial must retain its error at the deadline.");
            FileLockLease.TestLeaseWait = observed => { if (observed == path) Directory.Delete(path); };
            using var next = await FileLockLease.AcquireTemporaryExclusiveAsync(path, timeout, CancellationToken.None);
        }
        finally
        {
            FileLockLease.TestLeaseWait = null;
            if (Directory.Exists(path)) Directory.Delete(path);
        }
        Check(!File.Exists(path), "The retried open releases its temporary name.");
        log.AppendLine("PASS: a delete-pending document lock recovers after its metadata reader exits.");
    }

    private static async Task UnverifiedVersionAsync(FeatureTestHost host, StringBuilder log)
    {
        var source = await OpenAsync(host, "unverified.txt", "base"u8.ToArray());
        var file = source.EditingFile;
        var saved = source.LastSavedSnapshot;
        // A dirty recovery row from before byte fingerprints has only its timestamp.
        using var unknown = new DocumentSnapshot(saved.Baseline, saved.Encoding, saved.LineEnding, saved.DateModifiedFileTime);
        var editor = (TextEditor)host.Core.CreateTextEditor(Guid.NewGuid(), file, file.Name);
        await editor.InitAsync(unknown, file);
        host.Core.OpenTextEditor(editor);
        await editor.TestStopFileChecksAsync();
        await editor.TestCheckFileStatusAsync();
        var message = Windows.ApplicationModel.Resources.ResourceLoader.GetForCurrentView().GetString("FileSaveErrorDialog_Unverified");
        Check(editor.FileModificationState == FileModificationState.Unverified && !string.IsNullOrEmpty(message),
            "An unknown saved version must remain visibly unverified after polling.");
        await UntilAsync(() => host.Page.TestFileStatusMessage == message, "Status indicator must explain the unverified version.");
        editor.TypeText("edit");
        var save = host.Page.TestSaveDocumentAsync(editor);
        await ChooseDialogAsync(save, "CloseButton", message);
        Check(!await save && editor.IsModified, "Canceling an unverified save preserves the dirty editor.");
        await BytesAsync(file, "base"u8.ToArray(), "Unverified saves preserve on-disk contents");
        await editor.ReloadFromEditingFileAsync();
        await editor.TestStopFileChecksAsync();
        Check(editor.FileModificationState == FileModificationState.Untouched && editor.LastSavedSnapshot.DateModifiedFileTime >= 0,
            "Reload establishes a new verified version.");
        log.AppendLine("PASS: unverified status, save dialog, disk preservation and reload agree.");
    }

    private static async Task PickedTargetChangedAsync(FeatureTestHost host, StringBuilder log)
    {
        var editor = await OpenAsync(host, "picker-race-source.txt", "base"u8.ToArray());
        editor.TypeText("draft");
        var destination = await host.Folder.CreateFileAsync("picker-race-target.txt");
        await FileIO.WriteBytesAsync(destination, "old"u8.ToArray());
        TestFileSavePicker.Reset();
        TestFileSavePicker.Results.Enqueue(destination);
        bool writerExcluded = false;
        DocumentSaveTarget.TestVersionRead = (target, _) =>
        {
            try { using var writer = new FileStream(target.Path, FileMode.Open, FileAccess.Write, FileShare.ReadWrite | FileShare.Delete); }
            catch (Exception ex) when (ex.HResult is unchecked((int)0x80070020) or unchecked((int)0x80070005)) { writerExcluded = true; }
            return Task.CompletedTask;
        };
        host.Page.TestPickedVersionCaptured = target =>
        {
            var stamp = File.GetLastWriteTimeUtc(target.Path);
            File.WriteAllBytes(target.Path, "new"u8.ToArray());
            File.SetLastWriteTimeUtc(target.Path, stamp);
        };
        try
        {
            var save = host.Page.TestSaveDocumentAsync(editor, saveAs: true);
            var conflict = Windows.ApplicationModel.Resources.ResourceLoader.GetForCurrentView().GetString("FileSaveErrorDialog_VersionConflict");
            await ChooseDialogAsync(save, "CloseButton", conflict);
            Check(!await save && writerExcluded && editor.IsModified && !editor.EditingFile.IsEqual(destination),
                "A picked target must exclude competing writers while observed and reject later changes.");
        }
        finally
        {
            DocumentSaveTarget.TestVersionRead = null;
            host.Page.TestPickedVersionCaptured = null;
        }
        await BytesAsync(destination, "new"u8.ToArray(), "Save As must preserve the intervening update");
        log.AppendLine("PASS: Save As pins the picked version and detects a same-length, same-timestamp replacement.");
    }

    private static async Task EquivalentBindingDuringSaveAsAsync(FeatureTestHost host, StringBuilder log)
    {
        var editor = await OpenAsync(host, "same-binding.txt", "base"u8.ToArray());
        var original = editor.EditingFile;
        var equivalent = await StorageFile.GetFileFromPathAsync(original.Path);
        Check(!ReferenceEquals(original, equivalent) && original.IsEqual(equivalent), "The test requires distinct handles for the same file.");
        var destination = await host.Folder.CreateFileAsync("same-binding-target.txt");
        await FileIO.WriteBytesAsync(destination, "target"u8.ToArray());
        editor.TypeText("edit");
        var pinned = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var resume = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        TestFileSavePicker.Reset();
        TestFileSavePicker.Results.Enqueue(destination);
        DocumentSaveTarget.TestVersionRead = (file, token) =>
        {
            if (!file.IsEqual(destination)) return Task.CompletedTask;
            pinned.TrySetResult();
            return resume.Task.WaitAsync(token);
        };
        try
        {
            var saveAs = host.Page.TestSaveDocumentAsync(editor, saveAs: true);
            await pinned.Task.WaitAsync(TimeSpan.FromSeconds(15));
            await editor.SaveContentToFileAndUpdateEditorStateAsync(equivalent);
            Check(!ReferenceEquals(editor.EditingFile, original), "The concurrent save must replace the file object.");
            resume.SetResult();
            Check(await saveAs.WaitAsync(TimeSpan.FromSeconds(15)) && editor.EditingFile.IsEqual(destination),
                "An equivalent binding must allow the pending Save As to finish.");
        }
        finally
        {
            resume.TrySetResult();
            DocumentSaveTarget.TestVersionRead = null;
        }
        await BytesAsync(destination, editor.GetEncoding().GetBytes(editor.GetText()), "Equivalent binding retained Save As content");
        log.AppendLine("PASS: a concurrent save through an equivalent file handle retains the picked destination.");
    }

    private enum VerificationAction { CheckAgain, BusyCheckAgain, EditDuringCheckAgain, EditDuringRetryLookup, CancelAndSave, AliasCancelAndSave, CancelAndRename, CancelAndEdit, CancelAndUndo, AliasCancelAndUndo, CancelAndRevert, SaveAs }

    private static async Task RetryCommittedVerificationAsync(FeatureTestHost host, StringBuilder log, VerificationAction action)
    {
        StorageFile alias = null;
        TextEditor editor;
        if (action is VerificationAction.AliasCancelAndUndo or VerificationAction.AliasCancelAndSave)
        {
            var root = ApplicationData.Current.LocalFolder.Path;
            var folder = await StorageFolder.GetFolderFromPathAsync(Path.Combine(root, "document-targets"));
            var original = await folder.CreateFileAsync($"alias-save-{Guid.NewGuid():N}.txt");
            await FileIO.WriteBytesAsync(original, "base"u8.ToArray());
            Check(await host.Page.OpenFileAsync(original), "Open the original directory entry.");
            editor = (TextEditor)host.Core.GetSelectedReadyEditor();
            await editor.TestStopFileChecksAsync();
            alias = await StorageFile.GetFileFromPathAsync(Path.Combine(root, "document-alias", original.Name));
            Check(!original.IsEqual(alias), "The fixture must exercise the WinRT directory-junction mismatch.");
            TestFileSavePicker.Reset();
            TestFileSavePicker.Results.Enqueue(alias);
        }
        else editor = await OpenAsync(host, $"confirmation-{action}.txt", "base"u8.ToArray());
        editor.TypeText("draft");
        var expected = editor.GetText();
        var transfer = action == VerificationAction.CancelAndSave ? host.Page.TestCaptureTransfer(editor) : null;
        int commits = 0;
        FileStream blocker = null;
        DocumentFileWriter.TestLocalCommit = target =>
        {
            commits++;
            blocker = new FileStream(target.Path, FileMode.Open, FileAccess.Read, FileShare.None);
        };
        try
        {
            var save = host.Page.TestSaveDocumentAsync(editor, saveAs: alias != null);
            var warning = string.Format(Windows.ApplicationModel.Resources.ResourceLoader.GetForCurrentView().GetString(
                "FileSaveErrorDialog_CommittedUnverified"), (alias ?? editor.EditingFile).Path);
            var expectedCommits = 1;
            if (action == VerificationAction.SaveAs)
            {
                var alternative = await host.Folder.CreateFileAsync("confirmed-copy.txt");
                TestFileSavePicker.Reset();
                TestFileSavePicker.Results.Enqueue(alternative);
                await ChooseDialogAsync(save, "SecondaryButton", warning, beforeInvoke: () => DocumentFileWriter.TestLocalCommit = _ => commits++);
                Check(editor.EditingFile.IsEqual(alternative), "Save As must bind the explicitly chosen copy.");
                expectedCommits++;
            }
            else if (action == VerificationAction.BusyCheckAgain)
            {
                FileSaveErrorDialog first = null;
                FileLockLease held = null;
                try
                {
                    var nextDialog = UntilAsync(() => save.IsCompleted ||
                        first != null && ActiveVisibleDialog() is FileSaveErrorDialog next && !ReferenceEquals(first, next),
                        "Check Again should report contention while verifying the commit.");
                    await ChooseDialogAsync(nextDialog, "PrimaryButton", warning, whileDialogOpen: async () =>
                    {
                        first = (FileSaveErrorDialog)DialogManager.ActiveDialog;
                        blocker.Dispose();
                        held = await DocumentSaveTarget.AcquireAsync(editor.EditingFile, CancellationToken.None);
                        Check(held != null, "The competing editor owns the same save lease.");
                    });
                    var resources = Windows.ApplicationModel.Resources.ResourceLoader.GetForCurrentView();
                    await ChooseDialogAsync(save, "PrimaryButton", warning, beforeInvoke: () =>
                    {
                        Check(DialogManager.ActiveDialog.Title as string == resources.GetString("FileSaveErrorDialog_Unverified") &&
                            DialogManager.ActiveDialog.PrimaryButtonText == resources.GetString("FileSaveErrorDialog_CheckAgain"),
                            "The verification retry must still describe the completed write and offer Check Again.");
                        held.Dispose();
                    });
                }
                finally { held?.Dispose(); }
            }
            else if (action is not (VerificationAction.CheckAgain or VerificationAction.EditDuringCheckAgain))
            {
                await ChooseDialogAsync(save, "CloseButton", warning);
                Check(!await save && editor.IsModified, "Cancel must leave the draft recoverable.");
                if (transfer != null) Check(!transfer.IsSourceUnchanged(), "A transfer captured before an unconfirmed save must retain the source tab.");
                blocker.Dispose();
                await editor.TestCheckFileStatusAsync();
                Check(editor.FileModificationState == FileModificationState.Unverified, "Polling must distinguish an unconfirmed local commit from an external edit.");
                if (action is VerificationAction.CancelAndUndo or VerificationAction.AliasCancelAndUndo or VerificationAction.CancelAndRevert)
                {
                    if (action is VerificationAction.CancelAndUndo or VerificationAction.AliasCancelAndUndo) editor.TestNativeCore.Undo();
                    else await editor.RevertAllChangesAsync();
                    await editor.TestCompareContentAsync();
                    Check(editor.GetText() == "base" && editor.IsModified, "Returning to the old save point must protect the draft.");
                    using var capture = editor.CaptureRecoveryState();
                    await capture.Checkpoint.FlushAsync();
                    Check(capture.MetaData.IsModified && capture.MetaData.HasUnconfirmedFileSave, "Recovery must retain the unconfirmed draft.");
                    var restored = (TextEditor)host.Core.CreateTextEditor(Guid.NewGuid(), null, "Recovered draft");
                    try
                    {
                        await restored.RestoreV2Async(capture.SavedSnapshot, capture.RecoveryBaseline, capture.DocumentJournal,
                            capture.Checkpoint, editor.EditingFile, capture.MetaData, capture.TextDirty);
                        await restored.TestCompareContentAsync();
                        Check(restored.GetText() == "base" && restored.IsModified, "The restored old save point must remain an unsaved draft.");
                        try
                        {
                            await restored.SaveContentToFileAndUpdateEditorStateAsync(restored.EditingFile);
                            throw new InvalidOperationException("An unconfirmed recovery cannot reuse a disk pathname without a commit receipt.");
                        }
                        catch (DocumentSaveException ex) when (ex.Stage == DocumentSaveStage.Unverified) { }
                        await ThrowsAsync<InvalidOperationException>(() => restored.RenameAsync("must-not-rename.txt", restored.EditingFile),
                            "An unconfirmed recovery cannot move an unverified directory entry.");
                    }
                    finally { await host.Core.DiscardUnattachedEditorAsync(restored); }
                    var close = host.Page.TestCloseTabAsync(host.Core.GetTab(editor));
                    await ChooseDialogAsync(close, "CloseButton", editor.EditingFile.Name);
                    Check(host.Core.GetTab(editor) != null, "Closing the old save point must allow the user to retain the draft.");
                }
                if (action == VerificationAction.CancelAndRename)
                {
                    await editor.RenameAsync("locally-renamed.txt", editor.EditingFile);
                    Check(editor.EditingFileName == "locally-renamed.txt" && editor.IsModified, "Renaming must retain the unverified draft.");
                }
                if (action == VerificationAction.EditDuringRetryLookup)
                {
                    editor.TestResolvingSaveTarget = () => { editor.TypeText("later"); return Task.CompletedTask; };
                    editor.TestSaveReaderCaptured = () => throw new InvalidOperationException("Rechecking an unchanged commit copied another revision.");
                }
                else if (action is not (VerificationAction.CancelAndSave or VerificationAction.AliasCancelAndSave or VerificationAction.CancelAndRename))
                {
                    if (action == VerificationAction.CancelAndEdit) editor.TypeText("more");
                    expected = editor.GetText();
                    DocumentFileWriter.TestLocalCommit = _ => commits++;
                    expectedCommits++;
                }
                if (action == VerificationAction.AliasCancelAndSave)
                    Check(!ReferenceEquals(alias, editor.EditingFile), "The resumed save starts with the original grant.");
                save = host.Page.TestSaveDocumentAsync(editor, ignoreUnmodified: true);
            }
            else await ChooseDialogAsync(save, "PrimaryButton", warning, beforeInvoke: () =>
                {
                    blocker.Dispose();
                    if (action == VerificationAction.EditDuringCheckAgain)
                    {
                        editor.TypeText("later");
                        editor.TestSaveReaderCaptured = () => throw new InvalidOperationException("Check Again must only verify the previous commit.");
                    }
                },
                whileDialogOpen: () => editor.WaitForDocumentOperationsAsync().WaitAsync(TimeSpan.FromSeconds(5)));
            var laterEdit = action is VerificationAction.EditDuringCheckAgain or VerificationAction.EditDuringRetryLookup;
            Check(await save && commits == expectedCommits && editor.IsModified == laterEdit && editor.LastSavedSnapshot.FileVersion.IsVerified,
                $"{action} must verify the existing commit and write only newly requested edits or a new destination.");
            if (laterEdit) Check(editor.GetText().Contains("later", StringComparison.Ordinal), "The edit survives as a dirty revision.");
            if (action == VerificationAction.AliasCancelAndSave)
            {
                var recent = await Notepads.Infrastructure.Shell.MRUService.GetMostRecentlyUsedListAsync(1);
                Check(ReferenceEquals(recent.Single(), alias),
                    "Recent files must remember the grant whose transaction was confirmed.");
            }
        }
        finally
        {
            DocumentFileWriter.TestLocalCommit = null;
            editor.TestResolvingSaveTarget = null;
            editor.TestSaveReaderCaptured = null;
            blocker?.Dispose();
            if (transfer != null) await transfer.CompleteAsync(Notepads.Presentation.Workspace.TextEditorTransferResult.Cancelled, () => true);
        }
        await BytesAsync(editor.EditingFile, editor.GetEncoding().GetBytes(expected), "Retry verifies the existing commit");
        log.AppendLine($"PASS: {action} after a persistent lock preserved the expected contents and commit count.");
    }

    private static async Task VerifyProviderRenameAsync(FeatureTestHost host, StringBuilder log, bool cancel)
    {
        var editor = await OpenAsync(host, $"provider-original-{cancel}.txt", "base"u8.ToArray());
        editor.TypeText("edit");
        var renamed = $"provider-renamed-{cancel}.txt";
        FileStream blocker = null;
        var updates = new NotepadsEditorTests.DocumentFileWriterTests.TestFileUpdates
        {
            CompletionStatus = Windows.Storage.Provider.FileUpdateStatus.CompleteAndRenamed,
            ProviderMutation = async file =>
            {
                await file.RenameAsync(renamed);
                blocker = new FileStream(file.Path, FileMode.Open, FileAccess.Read, FileShare.None);
            }
        };
        DocumentFileWriter.TestUpdates = updates;
        try
        {
            var save = host.Page.TestSaveDocumentAsync(editor);
            if (cancel)
            {
                await ChooseDialogAsync(save, "CloseButton");
                Check(!await save && editor.IsModified && editor.EditingFileName == renamed && editor.EditingFilePath == editor.EditingFile.Path,
                    "Cancelling confirmation must leave a recoverable draft bound to the provider's new location.");
                blocker.Dispose();
                await VerifyRenamedRecoveryAsync(host, editor);
                var transfer = host.Page.TestCaptureTransfer(editor);
                try { await transfer.PrepareAsync(); }
                finally { await transfer.CompleteAsync(Notepads.Presentation.Workspace.TextEditorTransferResult.Cancelled, () => true); }
                save = host.Page.TestSaveDocumentAsync(editor);
            }
            else await ChooseDialogAsync(save, "PrimaryButton", beforeInvoke: () => blocker.Dispose());
            Check(await save && editor.EditingFile.Name == renamed && !editor.IsModified &&
                updates.Deferred == 1 && updates.Completed == 1, "A provider rename must verify the existing commit using its new identity.");
            await BytesAsync(editor.EditingFile, editor.GetEncoding().GetBytes(editor.GetText()), "The renamed entry contains the original committed bytes");
        }
        finally
        {
            DocumentFileWriter.TestUpdates = null;
            blocker?.Dispose();
        }
        log.AppendLine("PASS: read-only confirmation follows the provider-renamed grant without a second write.");
    }

    private static async Task VerifyRenamedRecoveryAsync(FeatureTestHost host, TextEditor editor)
    {
        var controller = host.Page.TestController;
        var enabled = controller.IsBackupEnabled;
        controller.IsBackupEnabled = true;
        try
        {
            Check(await controller.SaveSessionAsync(), "Publish the pending provider rename.");
            using var plan = await host.Page.TestDiscoverRecoveryAsync();
            var source = plan.Sources.Single(item => item.Id == editor.Id);
            using var prepared = await RecoveryDocumentPreparation.PrepareAsync(source, host.Core.DocumentOwnerId, new DocumentLoadOptions(), default);
            Check(prepared.Checkpoint != null && prepared.Metadata.HasUnconfirmedFileSave &&
                prepared.EditingFile?.Path == editor.EditingFile.Path,
                "The restored draft must retain the provider's renamed grant without authorizing an unconfirmed overwrite.");
        }
        finally { controller.IsBackupEnabled = enabled; }
    }

    private enum ProviderRenameAction { CheckAgain, Superseded, DelayedSave, DelayedEdit, DelayedRename, DelayedSuperseded, MoveAcknowledgement }

    private static async Task ProviderRenameThroughAliasAsync(FeatureTestHost host, StringBuilder log, ProviderRenameAction action)
    {
        var delayed = action is ProviderRenameAction.DelayedSave or ProviderRenameAction.DelayedEdit or
            ProviderRenameAction.DelayedRename or ProviderRenameAction.DelayedSuperseded or ProviderRenameAction.MoveAcknowledgement;
        var root = ApplicationData.Current.LocalFolder.Path;
        var folder = await StorageFolder.GetFolderFromPathAsync(Path.Combine(root, "document-targets"));
        var original = await folder.CreateFileAsync($"provider-alias-{Guid.NewGuid():N}.txt");
        var oldName = original.Name;
        var renamed = "moved-" + oldName;
        await FileIO.WriteBytesAsync(original, "base"u8.ToArray());
        Check(await host.Page.OpenFileAsync(original), "Open the bound grant before selecting its alias.");
        var editor = (TextEditor)host.Core.GetSelectedReadyEditor();
        await UntilAsync(() => editor.TestNativeCore.IsLoaded && host.Page.IsEnabled,
            "The provider fixture must be interactive before stopping its status worker.");
        await editor.TestStopFileChecksAsync();
        editor.TypeText("mine");
        var draft = editor.GetText();
        var alias = await StorageFile.GetFileFromPathAsync(Path.Combine(root, "document-alias", oldName));
        Check(!original.IsEqual(alias), "The picker grant must be a distinct directory alias.");
        TestFileSavePicker.Reset();
        TestFileSavePicker.Results.Enqueue(alias);
        FileStream blocker = null;
        async Task MoveAndReuseNameAsync(StorageFile file)
        {
            await file.RenameAsync(renamed);
            var other = await folder.CreateFileAsync(oldName);
            // Identical bytes at the reused name must never authorize an overwrite.
            await FileIO.WriteBytesAsync(other, "base"u8.ToArray());
        }
        int commits = 0;
        DocumentFileWriter.TestLocalCommit = _ => commits++;
        var updates = new NotepadsEditorTests.DocumentFileWriterTests.TestFileUpdates
        {
            CompletionStatus = Windows.Storage.Provider.FileUpdateStatus.CompleteAndRenamed,
            ProviderMutation = async file =>
            {
                if (delayed) return;
                await MoveAndReuseNameAsync(file);
                if (action == ProviderRenameAction.Superseded) await FileIO.WriteBytesAsync(file, "peer"u8.ToArray());
                else blocker = new FileStream(file.Path, FileMode.Open, FileAccess.Read, FileShare.None);
            }
        };
        DocumentFileWriter.TestUpdates = updates;
        try
        {
            var save = host.Page.TestSaveDocumentAsync(editor, saveAs: true);
            if (action == ProviderRenameAction.DelayedSuperseded)
            {
                FileSaveErrorDialog first = null;
                var nextDialog = UntilAsync(() => save.IsCompleted ||
                    first != null && ActiveVisibleDialog() is FileSaveErrorDialog next && !ReferenceEquals(first, next),
                    "Check Again must report the conflicting provider-renamed commit.");
                await ChooseDialogAsync(nextDialog, "PrimaryButton", whileDialogOpen: async () =>
                {
                    first = (FileSaveErrorDialog)DialogManager.ActiveDialog;
                    await MoveAndReuseNameAsync(alias);
                    await FileIO.WriteBytesAsync(alias, "peer"u8.ToArray());
                });
                await ChooseDialogAsync(save, "CloseButton", renamed);
                Check(!await save && editor.EditingFileName == renamed && editor.FileModificationState == FileModificationState.Modified &&
                    editor.IsModified && editor.GetText() == draft,
                    "A read-only retry must keep the renamed binding, show the conflict and preserve the draft.");
                await BytesAsync(alias, "peer"u8.ToArray(), "A superseded confirmation never writes again");
            }
            else if (delayed)
            {
                await ChooseDialogAsync(save, "CloseButton");
                Check(!await save && editor.IsModified, "An opaque rename keeps the draft pending.");
                await ThrowsAsync<InvalidOperationException>(() => editor.RenameAsync("unconfirmed.txt", editor.EditingFile),
                    "An opaque provider location must block a destructive rename.");
                if (action == ProviderRenameAction.MoveAcknowledgement)
                {
                    var acknowledging = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
                    var release = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
                    var removed = false;
                    var pendingName = editor.EditingFileName;
                    var move = editor.TryCommitTransferAsync(() => true, () =>
                    {
                        acknowledging.SetResult();
                        return release.Task;
                    }, () => removed = true);
                    try
                    {
                        await acknowledging.Task.WaitAsync(TimeSpan.FromSeconds(10));
                        await MoveAndReuseNameAsync(alias);
                        await editor.TestCheckFileStatusAsync().WaitAsync(TimeSpan.FromSeconds(10));
                        Check(!removed && editor.EditingFileName == pendingName,
                            "Polling must leave the old binding published while the move acknowledges it.");
                    }
                    finally { release.TrySetResult(); }
                    Check(await move && removed, "The frozen transfer acknowledgement must complete.");
                    await editor.TestCheckFileStatusAsync();
                    Check(editor.EditingFileName == renamed, "A later idle poll must publish the pending provider rename.");
                }
                else await MoveAndReuseNameAsync(alias);
                if (action == ProviderRenameAction.DelayedSave) await VerifyRenamedRecoveryAsync(host, editor);
                var transfer = host.Page.TestCaptureTransfer(editor);
                try { await transfer.PrepareAsync(); }
                finally { await transfer.CompleteAsync(Notepads.Presentation.Workspace.TextEditorTransferResult.Cancelled, () => true); }
                DocumentFileWriter.TestUpdates = null;
                if (action == ProviderRenameAction.DelayedRename)
                {
                    renamed = "local-" + oldName;
                    await editor.RenameAsync(renamed, editor.EditingFile);
                }
                if (action == ProviderRenameAction.DelayedEdit) editor.TypeText("further edits");
                var previousDialog = DialogManager.ActiveDialog;
                var retry = host.Page.TestSaveDocumentAsync(editor);
                await UntilAsync(() => retry.IsCompleted || DialogManager.ActiveDialog is FileSaveErrorDialog next &&
                    !ReferenceEquals(next, previousDialog), "The provider-renamed save must complete or offer confirmation of its own commit.");
                Check(retry.IsCompleted,
                    $"An uncontended {action} save must not need a confirmation dialog: {DialogManager.ActiveDialog?.Content}");
                Check(await retry &&
                    editor.EditingFileName == renamed && !editor.IsModified, "A bound save must follow the delayed provider rename.");
                await BytesAsync(editor.EditingFile, editor.GetEncoding().GetBytes(editor.GetText()), "Only the provider grant receives later edits");
            }
            else if (action == ProviderRenameAction.Superseded)
            {
                await ChooseDialogAsync(save, "CloseButton", renamed);
                Check(!await save && editor.IsModified && editor.GetText() == draft && editor.EditingFileName == renamed &&
                    editor.EditingFilePath == editor.EditingFile.Path && editor.FileModificationState == FileModificationState.Modified,
                    "A superseded renamed commit must keep both the current binding and the conflicted draft.");
                var transfer = host.Page.TestCaptureTransfer(editor);
                try { await transfer.PrepareAsync(); }
                finally { await transfer.CompleteAsync(Notepads.Presentation.Workspace.TextEditorTransferResult.Cancelled, () => true); }
                await BytesAsync(editor.EditingFile, "peer"u8.ToArray(), "Preserve the concurrent writer at the renamed entry");
            }
            else
            {
                await ChooseDialogAsync(save, "PrimaryButton", renamed, beforeInvoke: () => blocker.Dispose());
                Check(await save && !editor.IsModified && editor.EditingFileName == renamed,
                    "The alias request must follow its own renamed grant into read-only confirmation.");
            }
            Check(updates.Completed == 1 && updates.Deferred == 1 && commits == (action == ProviderRenameAction.DelayedEdit ? 2 : 1),
                "A renamed save may write again only for a newly edited revision.");
            await BytesAsync(await folder.GetFileAsync(oldName), "base"u8.ToArray(), "The reused old name must remain untouched");
        }
        finally
        {
            DocumentFileWriter.TestUpdates = null;
            DocumentFileWriter.TestLocalCommit = null;
            blocker?.Dispose();
        }
        log.AppendLine($"PASS: alias save follows the provider grant through {action} and preserves an independently reused name.");
    }

    private static async Task VerifySavedSettingsTransferAsync(FeatureTestHost host, StringBuilder log)
    {
        var editor = await OpenAsync(host, "metadata-only.txt", "plain"u8.ToArray());
        var version = editor.LastSavedSnapshot.FileVersion;
        var generation = editor.LastSavedSnapshot.Baseline.GenerationId;
        var stamp = DateTime.FromFileTimeUtc(version.ModifiedTime);
        var transfer = host.Page.TestCaptureTransfer(editor);
        try
        {
            // Both EOL settings encode the same single-line file. Preserve the original timestamp
            // after commit to exercise the saved-setting comparison independently of the disk version.
            File.SetLastWriteTimeUtc(editor.EditingFilePath, stamp.AddMinutes(1));
            var ending = editor.GetLineEnding() == LineEnding.Lf ? LineEnding.Crlf : LineEnding.Lf;
            Check(editor.TryChangeLineEnding(ending), "The transfer fixture must change only its line-ending preference.");
            DocumentFileWriter.TestUpdates = new NotepadsEditorTests.DocumentFileWriterTests.TestFileUpdates
            {
                ProviderMutation = file => { File.SetLastWriteTimeUtc(file.Path, stamp); return Task.CompletedTask; }
            };
            Check(await host.Page.TestSaveDocumentAsync(editor).WaitAsync(TimeSpan.FromSeconds(10)), "A timestamp-only touch must allow a byte-verified save.");
            Check(editor.LastSavedSnapshot.FileVersion == version && editor.LastSavedSnapshot.Baseline.GenerationId == generation &&
                editor.LastSavedSnapshot.LineEnding == ending && !transfer.IsSourceUnchanged(),
                "A transfer must preserve a newer saved EOL even when text, baseline generation and disk version match.");
        }
        finally
        {
            DocumentFileWriter.TestUpdates = null;
            await transfer.CompleteAsync(Notepads.Presentation.Workspace.TextEditorTransferResult.Cancelled, () => true);
        }
        log.AppendLine("PASS: a metadata-only touch permits a save and an earlier transfer detects its saved format.");
    }

    private static async Task DisposeDuringVerificationPromptAsync(FeatureTestHost host, StringBuilder log)
    {
        var editor = await OpenAsync(host, "dispose-confirmation.txt", "base"u8.ToArray());
        editor.TypeText("draft");
        FileStream blocker = null;
        DocumentFileWriter.TestLocalCommit = file => blocker = new FileStream(file.Path, FileMode.Open, FileAccess.Read, FileShare.None);
        try
        {
            var previous = DialogManager.ActiveDialog;
            var save = host.Page.TestSaveDocumentAsync(editor);
            await UntilAsync(() => DialogManager.ActiveDialog is FileSaveErrorDialog && DialogManager.ActiveDialog != previous,
                "The committed file must prompt for verification.");
            await host.Page.TestDisposeWindowAsync().WaitAsync(TimeSpan.FromSeconds(10));
            Check(!await save.WaitAsync(TimeSpan.FromSeconds(5)) && editor.DisposalCompletion.IsCompleted,
                "Closing the workspace must dismiss verification and release the editor without a dialog response.");
        }
        finally
        {
            DocumentFileWriter.TestLocalCommit = null;
            blocker?.Dispose();
        }
        log.AppendLine("PASS: closing the workspace dismisses pending save verification.");
    }

    private static async Task SameTimestampConflictAsync(FeatureTestHost host, StringBuilder log)
    {
        var editor = await OpenAsync(host, "same-time.txt", "base"u8.ToArray());
        var file = editor.EditingFile;
        editor.TypeText("edit");
        var stamp = editor.LastSavedSnapshot.DateModifiedFileTime;
        await FileIO.WriteBytesAsync(file, "peer"u8.ToArray());
        File.SetLastWriteTimeUtc(file.Path, DateTime.FromFileTimeUtc(stamp));
        var save = host.Page.TestSaveDocumentAsync(editor);
        await ChooseDialogAsync(save, "CloseButton", file.Name);
        Check(!await save && editor.IsModified, "An equal-length external change with a preserved timestamp must conflict.");
        await BytesAsync(file, "peer"u8.ToArray(), "Protected the other writer's same-timestamp contents");
        await editor.TestCheckFileStatusAsync();
        var modified = Windows.ApplicationModel.Resources.ResourceLoader.GetForCurrentView().GetString("TextEditor_FileModifiedOutsideIndicator_ToolTip");
        Check(editor.FileModificationState == FileModificationState.Modified && host.Page.TestFileStatusMessage == modified,
            "Polling an unchanged timestamp must retain the confirmed conflict and its reload action.");

        TestFileSavePicker.Reset();
        TestFileSavePicker.Results.Enqueue(file);
        var previousDialog = DialogManager.ActiveDialog;
        var replace = host.Page.TestSaveDocumentAsync(editor, saveAs: true);
        await UntilAsync(() => replace.IsCompleted || (DialogManager.ActiveDialog is FileSaveErrorDialog && DialogManager.ActiveDialog != previousDialog),
            "The explicit replacement did not complete after dismissing the conflict.");
        if (!replace.IsCompleted && DialogManager.ActiveDialog is FileSaveErrorDialog failure)
        {
            var message = failure.Content?.ToString();
            await ChooseDialogAsync(replace, "CloseButton");
            throw new InvalidOperationException("Explicit replacement displayed a save error: " + message);
        }
        Check(await replace, "An explicit replacement establishes a new version.");
        await editor.TestCheckFileStatusAsync();
        Check(editor.FileModificationState == FileModificationState.Untouched, "A successful replacement clears the conflict warning.");
        var committed = editor.LastSavedSnapshot.FileVersion;
        var foreign = Encoding.UTF8.GetBytes(editor.GetText());
        foreign[0] ^= 1;
        await FileIO.WriteBytesAsync(file, foreign);
        File.SetLastWriteTimeUtc(file.Path, DateTime.FromFileTimeUtc(committed.ModifiedTime));
        editor.TypeText("!");
        var next = host.Page.TestSaveDocumentAsync(editor);
        await ChooseDialogAsync(next, "CloseButton", file.Name);
        Check(!await next && editor.IsModified, "A saved revision must retain its encoded fingerprint for the next save.");
        await BytesAsync(file, foreign, "Post-save same-time conflict preserved external bytes");
        log.AppendLine("PASS: opened and saved revisions reject external equal-length, equal-timestamp replacements.");
    }

    private static async Task SupersededCloseAsync(FeatureTestHost host, StringBuilder log, bool keepWriterOpen)
    {
        var name = keepWriterOpen ? "locked-after-commit.txt" : "superseded-close.txt";
        var editor = await OpenAsync(host, name, "base"u8.ToArray());
        var file = editor.EditingFile;
        var tab = host.Core.GetTab(editor);
        editor.TypeText("draft");
        var draft = editor.GetText();
        var baseline = editor.LastSavedSnapshot;
        int saved = 0, commits = 0;
        editor.FileSaved += (_, _) => saved++;
        var committed = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        FileStream competingWriter = null;
        DocumentFileWriter.TestLocalCommit = target =>
        {
            commits++;
            competingWriter = new FileStream(target.Path, FileMode.Truncate, FileAccess.Write, FileShare.None);
            competingWriter.Write("external"u8);
            competingWriter.Flush();
            if (!keepWriterOpen) competingWriter.Dispose();
            committed.SetResult();
        };
        try
        {
            var close = host.Page.TestCloseTabAsync(tab);
            await ChooseDialogAsync(committed.Task, "PrimaryButton", file.Name);
            var key = keepWriterOpen ? "FileSaveErrorDialog_CommittedUnverified" : "FileSaveErrorDialog_VersionConflict";
            var warning = Windows.ApplicationModel.Resources.ResourceLoader.GetForCurrentView().GetString(key);
            await ChooseDialogAsync(close, "CloseButton", keepWriterOpen ? string.Format(warning, file.Path) : warning);
        }
        finally
        {
            DocumentFileWriter.TestLocalCommit = null;
            competingWriter?.Dispose();
        }
        Check(host.Core.ContainsTab(tab) && editor.IsModified && editor.GetText() == draft &&
            ReferenceEquals(baseline, editor.LastSavedSnapshot) && saved == 0 && commits == 1,
            "A superseded save-on-close must retain the unsaved tab, baseline and edits.");
        using var recovery = editor.CaptureRecoveryState();
        await recovery.Checkpoint.FlushAsync();
        Check(recovery.TextDirty && recovery.MetaData.IsModified, "The superseded draft is still recoverable.");
        await BytesAsync(file, "external"u8.ToArray(), "Closing must not replay a superseded commit");
        if (!keepWriterOpen)
        {
            await editor.RevertAllChangesAsync();
            Check(editor.GetText() == "base" && editor.IsModified && editor.FileModificationState == FileModificationState.Modified,
                "Reverting to the old baseline must keep a superseded file visibly conflicted.");
            var modified = Windows.ApplicationModel.Resources.ResourceLoader.GetForCurrentView().GetString("TextEditor_FileModifiedOutsideIndicator_ToolTip");
            await UntilAsync(() => host.Page.TestFileStatusMessage == modified, "Revert must retain the external-change indicator.");
            await BytesAsync(file, "external"u8.ToArray(), "Reverting the draft must preserve the other writer's bytes");
        }
        log.AppendLine($"PASS: {name} retained the tab and recoverable draft after a competing write.");
    }

    private static async Task UnrelatedSaveAsReceiptAsync(FeatureTestHost host, StringBuilder log)
    {
        var editor = await OpenAsync(host, "original-with-opaque-location.txt", "base"u8.ToArray());
        await editor.TestStopFileChecksAsync();
        editor.TypeText("draft");
        var bound = editor.EditingFile;
        var path = bound.Path;
        var parked = path + ".parked";
        var destination = await host.Folder.CreateFileAsync("independent-receipt.txt", CreationCollisionOption.FailIfExists);
        FileStream blocker = null;
        File.Move(path, parked);
        try
        {
            Check((await DocumentSaveTarget.ResolveAsync(bound, default)).FinalPath == null,
                "The original binding must be temporarily opaque while the unrelated target is available.");
            TestFileSavePicker.Reset();
            TestFileSavePicker.Results.Enqueue(destination);
            DocumentFileWriter.TestLocalCommit = committed =>
            {
                if (committed.IsEqual(destination)) blocker = File.Open(destination.Path, FileMode.Open, FileAccess.Read, FileShare.None);
            };
            var save = host.Page.TestSaveDocumentAsync(editor, saveAs: true);
            await ChooseDialogAsync(save, "CloseButton", destination.Name);
            Check(!await save && blocker != null && ReferenceEquals(editor.EditingFile, bound) &&
                !editor.GetTextEditorStateMetaData().HasUnconfirmedFileSave && editor.FileModificationState == FileModificationState.Untouched,
                "An independent pending Save As must leave the original binding and its recovery status unchanged.");

            blocker.Dispose();
            blocker = null;
            File.Move(parked, path);
            editor.TypeText("more");
            Check(await host.Page.TestSaveDocumentAsync(editor).WaitAsync(TimeSpan.FromSeconds(15)),
                "The original file must still support ordinary Save after the abandoned confirmation.");
            await BytesAsync(bound, editor.GetEncoding().GetBytes(editor.GetText()), "The original receives the later draft");
            Check((await FileIO.ReadTextAsync(destination)).Contains("draft", StringComparison.Ordinal) &&
                  !(await FileIO.ReadTextAsync(destination)).Contains("more", StringComparison.Ordinal),
                "The independent destination must retain its earlier commit.");
        }
        finally
        {
            DocumentFileWriter.TestLocalCommit = null;
            blocker?.Dispose();
            if (File.Exists(parked)) File.Move(parked, path);
        }
        log.AppendLine("PASS: an unverified independent Save As does not poison the original binding, recovery metadata, or next save.");
    }

    private static async Task DiscardDuringPickedReadAsync(FeatureTestHost host, StringBuilder log)
    {
        var editor = await OpenAsync(host, "picker-source.txt", "source"u8.ToArray());
        editor.TypeText("draft");
        var tab = host.Core.GetTab(editor);
        var destination = await host.Folder.CreateFileAsync("slow-picker-target.txt", CreationCollisionOption.FailIfExists);
        await FileIO.WriteBytesAsync(destination, "target"u8.ToArray());
        var pinned = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var canceled = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var release = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        DocumentSaveTarget.TestVersionRead = async (file, token) =>
        {
            if (!file.IsEqual(destination)) return;
            using var registration = token.Register(() => canceled.TrySetResult());
            pinned.SetResult();
            await release.Task;
            token.ThrowIfCancellationRequested();
        };
        try
        {
            TestFileSavePicker.Reset();
            TestFileSavePicker.Results.Enqueue(destination);
            var save = host.Page.TestSaveDocumentAsync(editor, saveAs: true);
            await pinned.Task.WaitAsync(TimeSpan.FromSeconds(15));
            var previousDialog = DialogManager.ActiveDialog;
            var close = host.Page.TestCloseTabAsync(tab);
            await canceled.Task.WaitAsync(TimeSpan.FromSeconds(10));
            Check(!save.IsCompleted && !close.IsCompleted && host.Core.ContainsTab(tab) &&
                ReferenceEquals(previousDialog, DialogManager.ActiveDialog),
                $"Close must wait for the canceled picker read: save={save.Status}, close={close.Status}, " +
                $"tab present={host.Core.ContainsTab(tab)}, dialog changed={!ReferenceEquals(previousDialog, DialogManager.ActiveDialog)}.");
            release.SetResult();
            await ChooseDialogAsync(close, "SecondaryButton");
            Check(!await save, "The canceled Save As must dismiss without saving.");
            await BytesAsync(destination, "target"u8.ToArray(), "Discard must preserve the picked target");
            await FileIO.WriteBytesAsync(destination, "released"u8.ToArray());
            await editor.DisposalCompletion;
            Check(!host.Core.ContainsTab(tab), "The discarded tab closes after its durable decision.");
        }
        finally
        {
            release.TrySetResult();
            DocumentSaveTarget.TestVersionRead = null;
        }
        log.AppendLine("PASS: close cancels the picked-file read and waits for delayed provider unwind before discard.");
    }

    private static async Task SaveCollisionAsync(FeatureTestHost host, StringBuilder log)
    {
        var first = await OpenAsync(host, "contended.txt", "base"u8.ToArray());
        var file = first.EditingFile;
        var second = (TextEditor)await host.Core.CreateTextEditorAsync(Guid.NewGuid(), file);
        host.Core.OpenTextEditor(second);
        first.TestNativeCore.SetTextSelectionPosition(0, 4);
        first.TypeText("first");
        second.TestNativeCore.SetTextSelectionPosition(0, 4);
        second.TypeText("second");

        TestFileSavePicker.Reset();
        using (var blocker = File.Open(file.Path, FileMode.Open, FileAccess.Read, FileShare.Read))
        {
            var retry = host.Page.TestSaveDocumentAsync(first);
            await ChooseDialogAsync(retry, "PrimaryButton", file.Name, beforeInvoke: blocker.Dispose);
            Check(await retry && TestFileSavePicker.LastCreated == null, "Retry a busy writable file in place without a picker.");
        }
        await BytesAsync(file, "first"u8.ToArray(), "Retry publishes the first editor's bytes");

        var conflict = host.Page.TestSaveDocumentAsync(second);
        await ChooseDialogAsync(conflict, "CloseButton", file.Name);
        Check(!await conflict && second.IsModified && second.GetText() == "second" && TestFileSavePicker.LastCreated == null,
            "A stale editor keeps its binding and edits without invoking a picker.");
        await BytesAsync(file, "first"u8.ToArray(), "Dismissing the conflict preserves the first commit");

        TestFileSavePicker.Results.Enqueue(file);
        var replace = host.Page.TestSaveDocumentAsync(second);
        await ChooseDialogAsync(replace, "SecondaryButton", file.Name);
        Check(await replace && !second.IsModified, "An explicit picker choice may replace the current disk version.");
        await BytesAsync(file, "second"u8.ToArray(), "Explicit replacement saves the conflicting edits");
        second.TypeText("!");
        var attributes = File.GetAttributes(file.Path);
        try
        {
            File.SetAttributes(file.Path, attributes | System.IO.FileAttributes.ReadOnly);
            TestFileSavePicker.Reset();
            TestFileSavePicker.Results.Enqueue(null);
            var denied = host.Page.TestSaveDocumentAsync(second);
            await ChooseDialogAsync(denied, "SecondaryButton", file.Name);
            Check(!await denied && TestFileSavePicker.LastCreated != null && second.IsModified &&
                second.EditingFile.IsEqual(file), "Canceling an explicit Save As after denial keeps the original binding and dirty edits.");
            await BytesAsync(file, "second"u8.ToArray(), "A read-only target is preserved");
        }
        finally { File.SetAttributes(file.Path, attributes); }
        log.AppendLine("PASS: busy-file retry, stale writer conflict/cancel, explicit replacement and read-only Save As cancellation.");
    }

    private static async Task SaveAsAndSaveAllAsync(FeatureTestHost host, StringBuilder log)
    {
        var first = await OpenAsync(host, "save-as-original.txt", [0x6f, 0x6e, 0x65]);
        var original = first.EditingFile;
        first.TestNativeCore.SetTextSelectionPosition(3, 3);
        first.TypeText("!");
        TestFileSavePicker.Reset();
        TestFileSavePicker.Results.Enqueue(null);
        Check(!await host.Page.TestSaveDocumentAsync(first, saveAs: true), "Canceling the Save As picker must report no save.");
        Check(TestFileSavePicker.LastCreated.SuggestedFileName == "save-as-original.txt" &&
            TestFileSavePicker.LastCreated.FileTypeChoices.ContainsKey("Text Documents"),
            "The real save-picker factory must use the document filename and supported text choices.");
        Check(first.GetText() == "one!" && first.IsModified && first.EditingFile.IsEqual(original),
            "Save As cancellation must preserve text, dirty state and original file binding.");
        await BytesAsync(original, [0x6f, 0x6e, 0x65], "Canceled Save As must preserve original file bytes");
        var destination = await host.Folder.CreateFileAsync("save-as-destination.txt", CreationCollisionOption.FailIfExists);
        TestFileSavePicker.Results.Enqueue(destination);
        Check(await host.Page.TestSaveDocumentAsync(first, saveAs: true), "MainPage Save As must complete with the supplied picker destination.");
        Check(first.EditingFile.IsEqual(destination) && first.EditingFileName == destination.Name && !first.IsModified,
            "Save As must rebind file identity, title and clean state after committing.");
        await BytesAsync(original, [0x6f, 0x6e, 0x65], "Save As must preserve the original file");
        await BytesAsync(destination, [0x6f, 0x6e, 0x65, 0x21], "Save As must write the edited content to the selected destination");

        var second = await OpenAsync(host, "save-all-second.txt", [0x74, 0x77, 0x6f]);
        first.TestNativeCore.SetTextSelectionPosition(4, 4);
        first.TypeText("1");
        second.TestNativeCore.SetTextSelectionPosition(3, 3);
        second.TypeText("2");
        var firstEvents = 0;
        var secondEvents = 0;
        first.FileSaved += (_, _) => firstEvents++;
        second.FileSaved += (_, _) => secondEvents++;
        Check(await host.Page.TestSaveAllDocumentsAsync(), "MainPage Save All must save all dirty ready tabs.");
        await BytesAsync(destination, [0x6f, 0x6e, 0x65, 0x21, 0x31], "Save All must save the unselected dirty document");
        await BytesAsync(second.EditingFile, [0x74, 0x77, 0x6f, 0x32], "Save All must save the selected dirty document");
        Check(!first.IsModified && !second.IsModified && firstEvents == 1 && secondEvents == 1,
            "Save All must commit each dirty document once.");
        Check(await host.Page.TestSaveAllDocumentsAsync() && firstEvents == 1 && secondEvents == 1,
            "A second Save All must skip clean documents.");
        await first.TestStopFileChecksAsync();
        await second.TestStopFileChecksAsync();
        TestFileSavePicker.Reset();
        log.AppendLine("PASS: documents/save-as cancellation/rebinding and save-all exercise real MainPage workflows with a declared picker boundary.");
    }

    private static async Task RenameWithUnavailableMetadataAsync(FeatureTestHost host, StringBuilder log)
    {
        var editor = await OpenAsync(host, "rename-without-metadata.txt", "base"u8.ToArray());
        editor.TypeText("draft");
        var oldPath = editor.EditingFilePath;
        int renames = 0;
        editor.FileRenamed += (_, _) => renames++;
        editor.TestReadingRenamedLocation = () => throw new DocumentSaveException(DocumentSaveStage.Coordinate, new UnauthorizedAccessException());
        try { await editor.RenameAsync("renamed-without-metadata.txt", editor.EditingFile); }
        finally { editor.TestReadingRenamedLocation = null; }
        Check(!File.Exists(oldPath) && editor.EditingFileName == "renamed-without-metadata.txt" &&
            editor.EditingFilePath == editor.EditingFile.Path && editor.IsModified && renames == 1 &&
            editor.LastSavedSnapshot.FileVersion.TargetPath == null,
            "A committed rename must update the tab even if the provider withholds the new target metadata.");
        TestFileSavePicker.Reset();
        TestFileSavePicker.Results.Enqueue(editor.EditingFile);
        Check(await host.Page.TestSaveDocumentAsync(editor, saveAs: true) && !editor.IsModified &&
            editor.LastSavedSnapshot.FileVersion.TargetPath != null, "Explicit confirmation must recover the new file's identity.");
        await BytesAsync(editor.EditingFile, editor.GetEncoding().GetBytes(editor.GetText()), "The confirmed grant received the draft");
        log.AppendLine("PASS: post-rename metadata denial preserves the successful rename and permits explicit confirmation.");
    }

    private static async Task ReloadRevertRenameAndShareAsync(FeatureTestHost host, StringBuilder log)
    {
        var editor = await OpenAsync(host, "reload.txt", [0x6f, 0x6c, 0x64, 0x0a, 0x74, 0x65, 0x78, 0x74]);
        editor.TestNativeCore.SetTextSelectionPosition(0, 8);
        editor.TypeText("local edit");
        await FileIO.WriteBytesAsync(editor.EditingFile, [0xef, 0xbb, 0xbf, 0x64, 0x69, 0x73, 0x6b, 0x0d, 0x0a, 0xce, 0xa9]);
        var reloads = 0;
        editor.FileReloaded += (_, _) => reloads++;
        await editor.ReloadFromEditingFileAsync();
        Check(editor.GetText() == "disk\rΩ" && !editor.IsModified && reloads == 1 && editor.GetLineEnding() == LineEnding.Crlf,
            "Reload must use current disk bytes and reset encoding/EOL and dirty state.");
        editor.TestNativeCore.Undo();
        Check(editor.GetText() == "local edit" && editor.IsModified, "Reload must preserve a single undo back to unsaved work.");
        editor.TestNativeCore.Redo();
        Check(editor.GetText() == "disk\rΩ" && !editor.IsModified, "Redo reload must return to the new saved baseline.");
        editor.TestNativeCore.SetTextSelectionPosition(0, 6);
        editor.TypeText("later edit");
        editor.TryChangeLineEnding(LineEnding.Lf);
        editor.TryChangeEncoding(Encoding.Unicode);
        var reverts = 0;
        editor.ChangeReverted += (_, _) => reverts++;
        await editor.RevertAllChangesAsync();
        Check(editor.GetText() == "disk\rΩ" && !editor.IsModified && editor.RequestedEncoding == null &&
            editor.RequestedLineEnding == null && reverts == 1, "Revert must restore saved content and clear pending metadata choices.");
        editor.TestNativeCore.Undo();
        Check(editor.GetText() == "later edit" && editor.IsModified, "Revert must remain undoable.");
        editor.TestNativeCore.Redo();
        var file = editor.EditingFile;
        var oldPath = file.Path;
        var renames = 0;
        editor.FileRenamed += (_, _) => renames++;
        await editor.RenameAsync("renamed.md", file);
        Check(!File.Exists(oldPath) && File.Exists(editor.EditingFilePath) && editor.EditingFileName == "renamed.md" && renames == 1,
            "Rename must commit disk identity and refresh editor title/path once.");
        Check(editor.GetText() == "disk\rΩ" && !editor.IsModified, "Rename must preserve document contents and save point.");
        editor.TestNativeCore.SetTextSelectionPosition(5, 6);
        Check(host.Core.TryGetSharingContent(editor, out var title, out var content) && title == "renamed.md" && content == "Ω",
            "Sharing must expose the renamed title and selected text only.");
        editor.TestNativeCore.SetTextSelectionPosition(0, 0);
        Check(host.Core.TryGetSharingContent(editor, out title, out content) && content == "disk\rΩ",
            "Sharing with no selection must expose the complete canonical document.");
        Check(!host.Core.TryGetSharingContent(null, out _, out _), "Sharing admission must reject an absent ready editor.");
        await editor.TestStopFileChecksAsync();
        log.AppendLine("PASS: documents/reload-revert preserve undo; rename refreshes disk/title; sharing exports selected or complete content.");
    }

    private static async Task ExternalFileSignalsAsync(FeatureTestHost host, StringBuilder log)
    {
        var changed = await OpenAsync(host, "outside-change.txt", [0x6f, 0x6c, 0x64]);
        var signals = 0;
        changed.FileModificationStateChanged += (_, _) => signals++;
        await FileIO.WriteBytesAsync(changed.EditingFile, [0x6e, 0x65, 0x77]);
        File.SetLastWriteTimeUtc(changed.EditingFilePath,
            DateTime.FromFileTimeUtc(changed.LastSavedSnapshot.DateModifiedFileTime).AddMinutes(2));
        await changed.TestCheckFileStatusAsync();
        Check(changed.FileModificationState == FileModificationState.Modified && signals == 1 && changed.GetText() == "old",
            "One explicit production status check must signal external modification without overwriting editor content.");
        await changed.TestCheckFileStatusAsync();
        Check(signals == 1, "Repeated unchanged status observations must not emit duplicate modification events.");
        await changed.ReloadFromEditingFileAsync();
        await changed.TestStopFileChecksAsync();
        await changed.TestCheckFileStatusAsync();
        Check(changed.GetText() == "new" && changed.FileModificationState == FileModificationState.Untouched && signals == 2,
            "Reload followed by status observation must clear the external change signal.");

        foreach (var move in new[] { false, true })
        {
            var missing = await OpenAsync(host, move ? "outside-move.txt" : "outside-delete.txt", [0x6b, 0x65, 0x65, 0x70]);
            var path = missing.EditingFilePath;
            if (move) File.Move(path, Path.Combine(host.Folder.Path, "moved-target.txt"));
            else File.Delete(path);
            await missing.TestCheckFileStatusAsync();
            Check(missing.FileModificationState == FileModificationState.RenamedMovedOrDeleted && missing.GetText() == "keep",
                "External " + (move ? "move" : "delete") + " must signal missing file without discarding its in-memory text.");
        }
        log.AppendLine("PASS: documents/external-file checks observe modified/deleted/moved disk files without timer sleeps or content loss.");
    }
}
