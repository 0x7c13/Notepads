// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Notepads.Features.Documents.IO;
using Notepads.Presentation.Controls.Dialog;
using Notepads.Presentation.Controls.TextEditor;
using NotepadsIntegrationTests;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Media;
using static NotepadsWorkspaceTests.DocumentFeatureAssertions;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsWorkspaceTests;

internal static partial class SaveCloseRaceTests
{
    internal static async Task RunAsync(StringBuilder log)
    {
        var failures = new List<Exception>();
        async Task RunCaseAsync(string name, Func<Task> run)
        {
            try
            {
                await UntilAsync(() => VisibleDialog() == null, "The previous fixture's dialog must be fully dismissed.");
                await run();
            }
            catch (Exception error)
            {
                log.AppendLine($"FAIL: {name}: {error}");
                failures.Add(new InvalidOperationException(name, error));
            }
            finally
            {
                VisibleDialog()?.Hide();
                await UntilAsync(() => VisibleDialog() == null, "Fixture cleanup must release its native dialog popup.");
            }
        }
        foreach (var confirm in new[] { false, true })
            await RunCaseAsync($"Stale picker / explicit confirmation={confirm}", () => StalePickerAsync(log, confirm));
        foreach (var reload in new[] { false, true })
            await RunCaseAsync($"Stale picker / confirmed revision / reload={reload}", () => StaleConfirmedPickerAsync(log, reload));
        await RunCaseAsync("Drag capture during active save", () => ActiveSaveTransferAsync(log));
        await RunCaseAsync("Picked save / actual external change", () => CurrentPickedConflictAsync(log));
        await RunCaseAsync("Confirmation / externally moved receipt", () => MovedReceiptAsync(log));
        await RunCaseAsync("Picked version / unexpected provider cancellation", () => UnexpectedPickedCancellationAsync(log));
        await RunCaseAsync("Opening a file / active placeholder save", () => ActivePlaceholderSaveAsync(log));
        await RunCaseAsync("Revert confirmation / window close", () => RevertConfirmationDuringCloseAsync(log));
        foreach (var saveAll in new[] { true, false })
            await RunCaseAsync($"Close ownership / Save All={saveAll}", () => CloseOwnershipAsync(log, saveAll));
        foreach (var window in new[] { false, true })
            await RunCaseAsync($"Fallback error / window close={window}", () => FallbackDuringCloseAsync(log, window));
        await RunCaseAsync("Bulk tab close / save failure", () => BulkCloseFailureAsync(log));
        if (failures.Count != 0) throw new AggregateException("Save/close race regressions failed.", failures);
    }

    private static TaskCompletionSource Signal() => new(TaskCreationOptions.RunContinuationsAsynchronously);

    private static async Task StalePickerAsync(StringBuilder log, bool confirm)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var editor = await OpenAsync(host, $"stale-picked-{confirm}.txt", "base"u8.ToArray());
        var file = editor.EditingFile;
        editor.TypeText("draft");
        var expected = editor.GetEncoding().GetBytes(editor.GetText());
        var firstAdmitted = Signal();
        var secondAdmitted = Signal();
        var releaseFirst = Signal();
        var releaseSecond = Signal();
        var admissions = 0;
        editor.TestResolvingSaveTarget = () =>
        {
            if (++admissions == 1) { firstAdmitted.SetResult(); return releaseFirst.Task; }
            secondAdmitted.TrySetResult();
            return releaseSecond.Task;
        };
        FileStream blocker = null;
        var commits = 0;
        DocumentFileWriter.TestLocalCommit = target =>
        {
            if (++commits == 1) blocker = File.Open(target.Path, FileMode.Open, FileAccess.Read, FileShare.None);
        };
        try
        {
            var first = editor.SaveContentToFileAndUpdateEditorStateAsync(file);
            await firstAdmitted.Task.WaitAsync(TimeSpan.FromSeconds(10));
            var picked = await editor.ReadPickedFileVersionAsync(file, default);
            var queued = editor.SaveContentToFileAndUpdateEditorStateAsync(file, picked);
            releaseFirst.SetResult();
            Guid receipt;
            try
            {
                await first;
                throw new InvalidOperationException("The actual post-commit read lock must leave a receipt.");
            }
            catch (DocumentSaveException error) when (error.Stage == DocumentSaveStage.ConfirmCommit && error.VerificationId.HasValue)
            { receipt = error.VerificationId.Value; }
            await secondAdmitted.Task.WaitAsync(TimeSpan.FromSeconds(10));
            blocker.Dispose();
            releaseSecond.SetResult();
            try
            {
                await queued;
                throw new InvalidOperationException("The picked old disk version must conflict with the first commit.");
            }
            catch (DocumentSaveException error) when (error.Stage == DocumentSaveStage.Conflict) { }
            var status = editor.FileModificationState;
            editor.TestSaveReaderCaptured = () => throw new InvalidOperationException("Confirming the retained revision must not copy it again.");
            await editor.SaveContentToFileAndUpdateEditorStateAsync(file, verificationId: confirm ? receipt : null);
            Check(status == FileModificationState.Unverified && commits == 1 && !editor.IsModified &&
                editor.LastSavedSnapshot.FileVersion.IsVerified,
                "A stale picked request cannot retire or supersede another valid bound commit receipt.");
            await BytesAsync(file, expected, "The original commit is confirmed without rewriting it");
        }
        finally
        {
            releaseFirst.TrySetResult();
            releaseSecond.TrySetResult();
            blocker?.Dispose();
            DocumentFileWriter.TestLocalCommit = null;
            editor.TestResolvingSaveTarget = null;
            editor.TestSaveReaderCaptured = null;
        }
        log.AppendLine($"PASS: stale picked save preserves a valid bound receipt; explicit confirmation={confirm}, one commit and no repeated snapshot copy.");
    }

    private static async Task FallbackDuringCloseAsync(StringBuilder log, bool window)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var editor = await OpenAsync(host, $"fallback-close-{window}.txt", "base"u8.ToArray());
        var file = editor.EditingFile;
        editor.TypeText("Ω");
        Check(editor.TryChangeEncoding(Encoding.GetEncoding(20127, EncoderFallback.ExceptionFallback, DecoderFallback.ExceptionFallback)),
            "The unrepresentable text must use a genuinely strict ASCII encoder.");
        var captured = Signal();
        var release = Signal();
        editor.TestSaveReaderCaptured = () => { captured.SetResult(); return release.Task; };
        try
        {
            var save = host.Page.TestSaveDocumentAsync(editor);
            await captured.Task.WaitAsync(TimeSpan.FromSeconds(10));
            Task close = window ? host.Page.TestCloseWindowAsync() : host.Page.TestCloseTabAsync(host.Core.GetTab(editor));
            Check(!host.Page.IsEnabled && !close.IsCompleted, "Close owns the page while the admitted save unwinds.");
            release.SetResult();
            await CancelInterruptedCloseAsync(save, close);
            Check(editor.IsModified && host.Core.GetTab(editor) != null && host.Page.IsEnabled,
                "Canceling close keeps the draft and restores interaction after a failed encoder.");
            await BytesAsync(file, "base"u8.ToArray(), "A strict encoder failure leaves the old file intact");
        }
        finally { release.TrySetResult(); editor.TestSaveReaderCaptured = null; }
        log.AppendLine($"PASS: a real encoder fallback during window close={window} cannot block or abort the close reminder.");
    }

    private static async Task CloseOwnershipAsync(StringBuilder log, bool saveAll)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var first = await OpenAsync(host, "close-owner-first.txt", "first"u8.ToArray());
        var second = await OpenAsync(host, "close-owner-second.txt", "second"u8.ToArray());
        first.TypeText("draft");
        second.TypeText("draft");
        var firstFile = first.EditingFile;
        var secondFile = second.EditingFile;
        await Windows.Storage.FileIO.WriteBytesAsync(firstFile, "external-first"u8.ToArray());
        await Windows.Storage.FileIO.WriteBytesAsync(secondFile, "external-second"u8.ToArray());
        var admitted = Signal();
        var release = Signal();
        var saving = saveAll ? first : second;
        saving.TestResolvingSaveTarget = () => { admitted.TrySetResult(); return release.Task; };
        try
        {
            var save = saveAll ? host.Page.TestSaveAllDocumentsAsync() : host.Page.TestSaveDocumentAsync(second);
            await admitted.Task.WaitAsync(TimeSpan.FromSeconds(10));
            var close = host.Page.TestCloseTabAsync(host.Core.GetTab(first));
            Check(!host.Page.IsEnabled, "A tab close takes page ownership away from unrelated save dialogs.");
            release.SetResult();
            await CancelInterruptedCloseAsync(save, close);
            Check(host.Page.IsEnabled && first.IsModified && second.IsModified &&
                host.Core.GetTab(first) != null && host.Core.GetTab(second) != null,
                "Cancel keeps both drafts and Save All does not continue prompting while another close owns the page.");
            await BytesAsync(firstFile, "external-first"u8.ToArray(), "The first concurrent writer survives close");
            await BytesAsync(secondFile, "external-second"u8.ToArray(), "The second concurrent writer survives close");
        }
        finally { release.TrySetResult(); saving.TestResolvingSaveTarget = null; }
        log.AppendLine($"PASS: close reminder owns the page against save-all={saveAll} errors from the same or another tab; both external files and drafts survive.");
    }

    private static NotepadsDialog VisibleDialog() => VisualTreeHelper.GetOpenPopups(Window.Current)
        .SelectMany(popup => Descendants(popup.Child)).OfType<NotepadsDialog>().FirstOrDefault();

    private static NotepadsDialog ActiveVisibleDialog() => VisibleDialog() is { } dialog &&
        ReferenceEquals(dialog, DialogManager.ActiveDialog) ? dialog : null;

    private static async Task BulkCloseFailureAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var first = await OpenAsync(host, "bulk-close-first.txt", "first"u8.ToArray());
        var second = await OpenAsync(host, "bulk-close-second.txt", "second"u8.ToArray());
        first.TypeText("draft");
        second.TypeText("draft");
        var firstFile = first.EditingFile;
        var secondFile = second.EditingFile;
        await Windows.Storage.FileIO.WriteBytesAsync(firstFile, "peer"u8.ToArray());
        // Close Others/Close Right starts these page handlers without awaiting them.
        var firstClose = host.Page.TestCloseTabAsync(host.Core.GetTab(first));
        var secondClose = host.Page.TestCloseTabAsync(host.Core.GetTab(second));
        var failedSave = UntilAsync(() => ActiveVisibleDialog() is FileSaveErrorDialog,
            "Saving the first closing tab must show its actual version conflict.");
        await ChooseDialogAsync(failedSave, "PrimaryButton", firstFile.Name);
        var queued = !secondClose.IsCompleted;
        await ChooseDialogAsync(firstClose, "CloseButton", firstFile.Name);
        await UntilAsync(() => secondClose.IsCompleted || ActiveVisibleDialog() is SetCloseSaveReminderDialog,
            "The next close reminder must become active only after the first workflow settles.");
        var secondPrompted = false;
        if (!secondClose.IsCompleted)
        {
            await ChooseDialogAsync(secondClose, "CloseButton", secondFile.Name);
            secondPrompted = true;
        }
        await Task.WhenAll(firstClose, secondClose).WaitAsync(TimeSpan.FromSeconds(10));
        Check(queued && secondPrompted && host.Page.IsEnabled && first.IsModified && second.IsModified &&
            host.Core.GetTab(first) != null && host.Core.GetTab(second) != null,
            "A closing tab retains dialog ownership through saving; its error cannot abort the next queued close.");
        await BytesAsync(firstFile, "peer"u8.ToArray(), "A failed bulk-close save keeps the concurrent writer");
        await BytesAsync(secondFile, "second"u8.ToArray(), "Canceling the next queued close keeps its file unchanged");
        log.AppendLine("PASS: bulk tab-close workflows remain serialized through save failures; both drafts and file bytes survive cancel.");
    }

    private static async Task CancelInterruptedCloseAsync(Task<bool> save, Task close)
    {
        await UntilAsync(() => ActiveVisibleDialog() is FileSaveErrorDialog || save.IsCompleted &&
            ActiveVisibleDialog() is SetCloseSaveReminderDialog or AppCloseSaveReminderDialog,
            "An interrupted save must settle while the close reminder remains usable.");
        var lateError = ActiveVisibleDialog() is FileSaveErrorDialog;
        var reminder = ActiveVisibleDialog();
        if (lateError) await ChooseDialogAsync(save, "CloseButton");
        if (!close.IsCompleted) await ChooseDialogAsync(close, "CloseButton");
        await close.WaitAsync(TimeSpan.FromSeconds(10));
        Check(!await save, "The interrupted or failed save must return false.");
        Check(!lateError && reminder?.IsAborted != true,
            "A save error must not replace, abort or delay the close reminder that owns the page.");
    }
}
