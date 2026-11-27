// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Text;
using System.Threading.Tasks;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Documents.IO;
using Notepads.Infrastructure.Storage;
using Notepads.Presentation.Controls.Dialog;
using Notepads.Presentation.Controls.TextEditor;
using NotepadsIntegrationTests;
using Windows.ApplicationModel.Resources;
using Windows.Storage;
using Windows.Storage.Provider;
using static NotepadsWorkspaceTests.DocumentFeatureAssertions;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsWorkspaceTests;

internal static class PendingSaveRegressionTests
{
    internal static async Task RunAsync(FeatureTestHost host, StringBuilder log)
    {
        await QueuedProviderConfirmationAsync(host, log);
        for (var mode = 0; mode < 3; mode++) await RetryNewRevisionAsync(host, log, mode);
        await RenameWithoutMetadataAsync(host, log);
        await FailedRenameAfterProviderMoveAsync(host, log);
    }

    private static async Task QueuedProviderConfirmationAsync(FeatureTestHost host, StringBuilder log)
    {
        var root = ApplicationData.Current.LocalFolder.Path;
        var folder = await StorageFolder.GetFolderFromPathAsync(Path.Combine(root, "document-targets"));
        var original = await folder.CreateFileAsync($"queued-provider-{Guid.NewGuid():N}.txt");
        var oldName = original.Name;
        await FileIO.WriteBytesAsync(original, "base"u8.ToArray());
        Check(await host.Page.OpenFileAsync(original), "Open the original grant before queueing an alias save.");
        var editor = (TextEditor)host.Core.GetSelectedReadyEditor();
        await UntilAsync(() => editor.TestNativeCore.IsLoaded && host.Page.IsEnabled,
            "The queued provider fixture must be interactive before stopping its status worker.");
        await editor.TestStopFileChecksAsync();
        var alias = await StorageFile.GetFileFromPathAsync(Path.Combine(root, "document-alias", oldName));
        editor.TypeText("first");
        var firstAdmitted = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var secondAdmitted = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var releaseFirst = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var releaseSecond = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var waiting = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var admissions = 0;
        editor.TestResolvingSaveTarget = () =>
        {
            if (++admissions == 1) { firstAdmitted.SetResult(); return releaseFirst.Task; }
            secondAdmitted.TrySetResult();
            return releaseSecond.Task;
        };
        var commits = 0;
        FileStream blocker = null;
        FileLockLease held = null;
        DocumentFileWriter.TestUpdates = new NotepadsEditorTests.DocumentFileWriterTests.TestFileUpdates
            { CompletionStatus = FileUpdateStatus.CompleteAndRenamed };
        DocumentFileWriter.TestLocalCommit = file =>
        {
            if (++commits == 2) blocker = File.Open(file.Path, FileMode.Open, FileAccess.Read, FileShare.None);
        };
        var stage = "first save";
        try
        {
            TestFileSavePicker.Reset();
            TestFileSavePicker.Results.Enqueue(alias);
            var first = host.Page.TestSaveDocumentAsync(editor, saveAs: true);
            await firstAdmitted.Task.WaitAsync(TimeSpan.FromSeconds(15));
            editor.TypeText("second");
            var expected = editor.GetEncoding().GetBytes(editor.GetText());
            var second = host.Page.TestSaveDocumentAsync(editor);
            releaseFirst.SetResult();
            await secondAdmitted.Task.WaitAsync(TimeSpan.FromSeconds(15));
            await ChooseDialogAsync(first, "CloseButton");
            Check(!await first && ReferenceEquals(editor.EditingFile, alias), "The first unconfirmed save publishes its equivalent picker grant.");
            var previous = DialogManager.ActiveDialog;
            stage = "hold original lease";
            held = await DocumentSaveTarget.AcquireAsync(original, default);
            FileLockLease.TestLeaseWait = path =>
            {
                if (path.Contains("\\DocumentSaveLocks\\", StringComparison.Ordinal)) waiting.TrySetResult();
            };
            releaseSecond.SetResult();
            stage = "wait for queued lease";
            await waiting.Task.WaitAsync(TimeSpan.FromSeconds(5));
            stage = "provider rename";
            await alias.RenameAsync("moved-" + oldName);
            stage = "recreate old name";
            var reused = await folder.CreateFileAsync(oldName);
            stage = "write reused entry";
            await FileIO.WriteBytesAsync(reused, "base"u8.ToArray());
            DocumentFileWriter.TestUpdates = null;
            held.Dispose();
            held = null;
            stage = "confirm queued commit";
            await UntilAsync(() => second.IsCompleted || ActiveVisibleDialog() is FileSaveErrorDialog next && !ReferenceEquals(next, previous),
                "The queued save must offer confirmation of its new commit at the provider-renamed grant.");
            Check(commits == 2 && blocker != null, "The queued request reached its own local commit before its verification failed.");
            await ChooseDialogAsync(second, "PrimaryButton", alias.Name, beforeInvoke: () => blocker.Dispose());
            Check(await second && commits == 2 && !editor.IsModified && editor.EditingFileName == alias.Name,
                "A queued request retains the observed provider binding when a subsequent non-renaming commit is unverified.");
            await BytesAsync(alias, expected, "The queued revision survives and Check Again does not write a third time");
            await BytesAsync(reused, "base"u8.ToArray(), "The reused original name remains untouched");
        }
        catch (Exception error)
        {
            throw new InvalidOperationException($"Queued provider fixture failed during {stage} (0x{error.HResult:X8}).", error);
        }
        finally
        {
            releaseFirst.TrySetResult();
            releaseSecond.TrySetResult();
            held?.Dispose();
            blocker?.Dispose();
            FileLockLease.TestLeaseWait = null;
            DocumentFileWriter.TestUpdates = null;
            DocumentFileWriter.TestLocalCommit = null;
            editor.TestResolvingSaveTarget = null;
            TestFileSavePicker.Reset();
        }
        log.AppendLine("PASS: a queued save preserves a delayed provider binding through a second unverified non-renaming commit; Check Again confirms it without overwriting the reused old name.");
    }

    private static async Task RetryNewRevisionAsync(FeatureTestHost host, StringBuilder log, int mode)
    {
        var editor = await OpenAsync(host, $"pending-new-revision-{mode}.txt", "base"u8.ToArray());
        var file = editor.EditingFile;
        var tab = host.Core.GetTab(editor);
        editor.TypeText("v1\rnext");
        editor.TryChangeLineEnding(LineEnding.Crlf);
        FileStream blocker = null;
        var commits = 0;
        var published = 0;
        editor.FileSaved += (_, _) => published++;
        DocumentFileWriter.TestLocalCommit = target =>
        {
            if (++commits == 1) blocker = File.Open(target.Path, FileMode.Open, FileAccess.Read, FileShare.None);
        };
        try
        {
            var first = host.Page.TestSaveDocumentAsync(editor);
            await ChooseDialogAsync(first, "CloseButton");
            Check(!await first && published == 0 && commits == 1, "The old revision committed but remains unconfirmed.");
            if (mode == 0) editor.TypeText("v2");
            else if (mode == 1) editor.TryChangeEncoding(new UnicodeEncoding(false, false, true));
            else editor.TryChangeLineEnding(LineEnding.Lf);
            var ending = editor.GetLineEnding() switch { LineEnding.Crlf => "\r\n", LineEnding.Lf => "\n", _ => "\r" };
            var text = editor.GetText().Replace("\r", ending);
            var expected = editor.GetEncoding().GetBytes(text);
            Task<bool> save = null;
            Task request;
            if (mode == 2)
            {
                request = host.Page.TestCloseTabAsync(tab);
                FileSaveErrorDialog retryDialog = null;
                var reminder = DialogManager.ActiveDialog;
                var showingRetry = UntilAsync(() => request.IsCompleted ||
                    ActiveVisibleDialog() is FileSaveErrorDialog next && !ReferenceEquals(next, reminder),
                    "Saving on close must report the previous unconfirmed commit.");
                await ChooseDialogAsync(showingRetry, "PrimaryButton", whileDialogOpen: () =>
                {
                    retryDialog = DialogManager.ActiveDialog as FileSaveErrorDialog;
                    return Task.CompletedTask;
                });
                Check(retryDialog == null, "Close first presents its save reminder.");
            }
            else request = save = mode == 0 ? host.Page.TestSaveDocumentAsync(editor) : host.Page.TestSaveAllDocumentsAsync();
            string retryLabel = null;
            await ChooseDialogAsync(request, "PrimaryButton", beforeInvoke: () => blocker.Dispose(), whileDialogOpen: () =>
            {
                retryLabel = ((FileSaveErrorDialog)DialogManager.ActiveDialog).PrimaryButtonText;
                return Task.CompletedTask;
            });
            if (save != null) Check(await save, "Retry must complete the current save request.");
            await BytesAsync(file, expected, "Retry confirms the old receipt and writes the newly requested text/encoding/EOL");
            Check(commits == 2 && published == 1 && (mode == 2 ? !host.Core.ContainsTab(tab) : !editor.IsModified),
                "A successful current request publishes its own revision exactly once and completes Save All/close.");
            Check(retryLabel == ResourceLoader.GetForCurrentView().GetString("FileSaveErrorDialog_Retry"),
                "An older unconfirmed receipt offers Retry rather than claiming that the current changes committed.");
        }
        finally
        {
            blocker?.Dispose();
            DocumentFileWriter.TestLocalCommit = null;
        }
        log.AppendLine($"PASS: pending old receipt retry mode={mode} saves the newer text/encoding/EOL through actual MainPage Save, Save All or tab close.");
    }

    private static async Task RenameWithoutMetadataAsync(FeatureTestHost host, StringBuilder log)
    {
        var editor = await OpenAsync(host, "pending-rename-metadata.txt", "base"u8.ToArray());
        editor.TypeText("draft");
        var expected = editor.GetEncoding().GetBytes(editor.GetText());
        FileStream blocker = null;
        var commits = 0;
        DocumentFileWriter.TestLocalCommit = file =>
        {
            if (++commits == 1) blocker = File.Open(file.Path, FileMode.Open, FileAccess.Read, FileShare.None);
        };
        try
        {
            var save = host.Page.TestSaveDocumentAsync(editor);
            await ChooseDialogAsync(save, "CloseButton");
            Check(!await save, "Retain a bound unconfirmed commit before renaming.");
            blocker.Dispose();
            editor.TestReadingRenamedLocation = () => throw new DocumentSaveException(DocumentSaveStage.Coordinate, new UnauthorizedAccessException());
            await editor.RenameAsync("pending-renamed-metadata.txt", editor.EditingFile);
            editor.TestReadingRenamedLocation = null;
            await ThrowsAsync<InvalidOperationException>(() => editor.RenameAsync("must-not-rename-pending.txt", editor.EditingFile),
                "A missing location after explicit rename cannot authorize a second rename through its old receipt.");
            try
            {
                await editor.SaveContentToFileAndUpdateEditorStateAsync(editor.EditingFile);
                throw new InvalidOperationException("An ordinary Save must require explicit confirmation after losing post-rename metadata.");
            }
            catch (DocumentSaveException ex) when (ex.Stage == DocumentSaveStage.Unverified) { }
            Check(editor.IsModified && commits == 1 && editor.EditingFileName == "pending-renamed-metadata.txt",
                "The renamed draft and committed bytes survive without publishing an unknown-location receipt.");
            await BytesAsync(editor.EditingFile, expected, "Unknown-location operations leave the first commit intact");
            TestFileSavePicker.Reset();
            TestFileSavePicker.Results.Enqueue(editor.EditingFile);
            Check(await host.Page.TestSaveDocumentAsync(editor, saveAs: true) && !editor.IsModified && commits == 2,
                "An explicit picked confirmation recovers the renamed file and allows saving.");
        }
        finally
        {
            blocker?.Dispose();
            DocumentFileWriter.TestLocalCommit = null;
            editor.TestReadingRenamedLocation = null;
        }
        log.AppendLine("PASS: a bound pending receipt loses overwrite/rename authority after explicit rename metadata denial; Save As recovers it.");
    }

    private static async Task FailedRenameAfterProviderMoveAsync(FeatureTestHost host, StringBuilder log)
    {
        var editor = await OpenAsync(host, "pending-provider-rename.txt", "base"u8.ToArray());
        editor.TypeText("draft");
        var file = editor.EditingFile;
        var oldPath = file.Path;
        var updates = new NotepadsEditorTests.DocumentFileWriterTests.TestFileUpdates { CompletionStatus = FileUpdateStatus.CompleteAndRenamed };
        DocumentFileWriter.TestUpdates = updates;
        try
        {
            var save = host.Page.TestSaveDocumentAsync(editor);
            await ChooseDialogAsync(save, "CloseButton");
            Check(!await save && editor.IsModified, "The provider has acknowledged but not exposed a rename.");
            await file.RenameAsync("pending-provider-moved.txt");
            var collision = await host.Folder.CreateFileAsync("pending-provider-collision.txt");
            var renames = 0;
            editor.FileRenamed += (_, _) => renames++;
            await ThrowsAsync<Exception>(() => editor.RenameAsync(collision.Name, file), "The user's rename must fail on the existing file.");
            Check(!File.Exists(oldPath) && File.Exists(collision.Path) && editor.EditingFileName == file.Name &&
                editor.EditingFilePath == file.Path && editor.DisplayTitle == file.Name && editor.IsModified && renames == 1,
                "A failed user rename must still publish the preceding acknowledged provider move and keep the draft.");
            DocumentFileWriter.TestUpdates = null;
            Check(await host.Page.TestSaveDocumentAsync(editor) && !editor.IsModified && updates.Completed == 1,
                "The observed provider receipt can still be confirmed without writing again.");
            await BytesAsync(file, editor.GetEncoding().GetBytes(editor.GetText()), "Confirmation follows the provider's moved grant");
        }
        finally { DocumentFileWriter.TestUpdates = null; }
        log.AppendLine("PASS: a failed explicit rename publishes a preceding provider rename without losing the pending receipt.");
    }
}
