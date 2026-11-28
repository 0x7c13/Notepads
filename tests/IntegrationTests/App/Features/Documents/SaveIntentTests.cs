// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Text;
using System.Threading.Tasks;
using Notepads.Features.Documents.IO;
using Notepads.Presentation.Controls.Dialog;
using Notepads.Presentation.Controls.TextEditor;
using NotepadsIntegrationTests;
using Windows.Storage;
using static NotepadsWorkspaceTests.DocumentFeatureAssertions;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsWorkspaceTests;

internal static class SaveIntentTests
{
    internal static async Task RunAsync(FeatureTestHost host, StringBuilder log)
    {
        await RejectQueuedRedirectAsync(host, log);
        await RejectMatchingBytesOnRedirectAsync(host, log);
        await IndependentReceiptAsync(host, log, confirm: false, edit: false);
        await IndependentReceiptAsync(host, log, confirm: false, edit: true);
        await IndependentReceiptAsync(host, log, confirm: true, edit: false);
    }

    private static async Task RejectMatchingBytesOnRedirectAsync(FeatureTestHost host, StringBuilder log)
    {
        var root = ApplicationData.Current.LocalFolder.Path;
        var selected = Path.Combine(root, "document-retarget");
        var standby = Path.Combine(root, "document-standby");
        var parked = Path.Combine(root, "document-parked");
        var name = $"matching-retarget-{Guid.NewGuid():N}.txt";
        var original = await StorageFile.GetFileFromPathAsync(await CreateFileAsync(root, "document-targets", name, "saved"));
        var other = await StorageFile.GetFileFromPathAsync(await CreateFileAsync(root, "document-other-targets", name, "picked"));
        var alias = await StorageFile.GetFileFromPathAsync(Path.Combine(selected, name));
        Check(await host.Page.OpenFileAsync(alias), "Open the picked-save source through its original junction.");
        var editor = (TextEditor)host.Core.GetSelectedReadyEditor();
        await UntilAsync(() => editor.TestNativeCore.IsLoaded && host.Page.IsEnabled, "The alias editor must be interactive.");
        await editor.TestStopFileChecksAsync();
        editor.TypeText("draft");
        bool parkedOriginal = false, movedStandby = false;
        try
        {
            Redirect(selected, standby, parked, ref parkedOriginal, ref movedStandby);
            var picked = await editor.ReadPickedFileVersionAsync(alias, default);
            await FileIO.WriteTextAsync(other, "saved");
            try
            {
                await editor.SaveContentToFileAndUpdateEditorStateAsync(alias, picked);
                throw new InvalidOperationException("A changed picked version on the redirected target must conflict.");
            }
            catch (DocumentSaveException error) when (error.Stage == DocumentSaveStage.Conflict) { }
            Check(editor.IsModified && editor.FileModificationState == FileModificationState.Modified,
                "Matching saved bytes on a different physical target must not preserve the original bound-file classification.");
            await BytesAsync(original, "saved"u8.ToArray(), "The original physical target remains unchanged");
            await BytesAsync(other, "saved"u8.ToArray(), "The redirected target preserves the external writer");
        }
        finally { Restore(selected, standby, parked, ref parkedOriginal, ref movedStandby); }
        log.AppendLine("PASS: a stale picked request rejects a redirected target with matching saved bytes and still reports the physical file change.");
    }

    private static async Task RejectQueuedRedirectAsync(FeatureTestHost host, StringBuilder log)
    {
        var root = ApplicationData.Current.LocalFolder.Path;
        var selected = Path.Combine(root, "document-retarget");
        var standby = Path.Combine(root, "document-standby");
        var parked = Path.Combine(root, "document-parked");
        var name = $"queued-intent-{Guid.NewGuid():N}.txt";
        var original = await StorageFile.GetFileFromPathAsync(await CreateFileAsync(root, "document-targets", name, "original"));
        var other = await StorageFile.GetFileFromPathAsync(await CreateFileAsync(root, "document-other-targets", name, "other"));
        var alias = await StorageFile.GetFileFromPathAsync(Path.Combine(selected, name));
        Check(await host.Page.OpenFileAsync(alias), "Open the queued-save source through the real junction.");
        var editor = (TextEditor)host.Core.GetSelectedReadyEditor();
        await editor.TestStopFileChecksAsync();
        editor.TypeText(" first");
        var firstBytes = editor.GetEncoding().GetBytes(editor.GetText());
        var admitted = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var release = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var admissions = 0;
        bool parkedOriginal = false, movedStandby = false;
        Task<bool> first = null;
        Task<bool> queued = null;
        editor.TestResolvingSaveTarget = () =>
        {
            if (++admissions == 1)
            {
                admitted.SetResult();
                return release.Task;
            }
            Redirect(selected, standby, parked, ref parkedOriginal, ref movedStandby);
            return Task.CompletedTask;
        };
        try
        {
            TestFileSavePicker.Reset();
            TestFileSavePicker.Results.Enqueue(original);
            first = host.Page.TestSaveDocumentAsync(editor, saveAs: true);
            await admitted.Task.WaitAsync(TimeSpan.FromSeconds(15));
            editor.TypeText(" later");
            var draft = editor.GetText();
            var previousDialog = DialogManager.ActiveDialog;
            queued = host.Page.TestSaveDocumentAsync(editor);
            Check(!queued.IsCompleted, "The ordinary save must wait behind the equivalent picked save.");
            release.SetResult();
            Check(await first.WaitAsync(TimeSpan.FromSeconds(15)), "The explicitly selected original file must save first.");
            await RejectSaveAsync(queued, previousDialog);
            Check(movedStandby && admissions == 2 && ReferenceEquals(editor.EditingFile, original) && editor.IsModified && editor.GetText() == draft,
                "The queued ordinary save must retain the rebound original and the later edit.");
            await BytesAsync(original, firstBytes, "The original retains the first admitted revision");
            await BytesAsync(other, "other"u8.ToArray(), "A stale request grant never overwrites the other file");
        }
        finally
        {
            try
            {
                release.TrySetResult();
                editor.TestResolvingSaveTarget = null;
                if (first != null) await first.WaitAsync(TimeSpan.FromSeconds(15));
                if (queued != null && queued.IsCompleted) await queued;
            }
            finally { Restore(selected, standby, parked, ref parkedOriginal, ref movedStandby); }
        }
        log.AppendLine("PASS: a queued ordinary Save rejects its stale redirected grant after an equivalent picked rebind; both files and the later draft survive.");
    }

    private static async Task IndependentReceiptAsync(FeatureTestHost host, StringBuilder log, bool confirm, bool edit)
    {
        var root = ApplicationData.Current.LocalFolder.Path;
        var selected = Path.Combine(root, "document-retarget");
        var standby = Path.Combine(root, "document-standby");
        var parked = Path.Combine(root, "document-parked");
        var name = $"receipt-intent-{Guid.NewGuid():N}.txt";
        var original = await StorageFile.GetFileFromPathAsync(await CreateFileAsync(root, "document-targets", name, "original"));
        var destination = await StorageFile.GetFileFromPathAsync(await CreateFileAsync(root, "document-other-targets", name, "other"));
        var alias = await StorageFile.GetFileFromPathAsync(Path.Combine(selected, name));
        Check(await host.Page.OpenFileAsync(alias), "Open the receipt source through a real junction.");
        var editor = (TextEditor)host.Core.GetSelectedReadyEditor();
        await editor.TestStopFileChecksAsync();
        editor.TypeText(" copy");
        var copyBytes = editor.GetEncoding().GetBytes(editor.GetText());
        var snapshot = editor.LastSavedSnapshot;
        FileStream blocker = null;
        var commits = 0;
        bool parkedOriginal = false, movedStandby = false;
        DocumentFileWriter.TestLocalCommit = file =>
        {
            commits++;
            if (file.IsEqual(destination)) blocker = File.Open(destination.Path, FileMode.Open, FileAccess.Read, FileShare.None);
        };
        try
        {
            TestFileSavePicker.Reset();
            TestFileSavePicker.Results.Enqueue(destination);
            var saveAs = host.Page.TestSaveDocumentAsync(editor, saveAs: true);
            if (confirm)
            {
                await ChooseDialogAsync(saveAs, "PrimaryButton", destination.Name, beforeInvoke: () => blocker.Dispose());
                Check(await saveAs && ReferenceEquals(editor.EditingFile, destination) && !editor.IsModified && commits == 1,
                    "An explicit Check Again must confirm the independent picked receipt without a second write.");
            }
            else
            {
                await ChooseDialogAsync(saveAs, "CloseButton", destination.Name);
                Check(!await saveAs && ReferenceEquals(editor.EditingFile, alias), "Canceling independent confirmation must retain the original binding.");
                blocker.Dispose();
                blocker = null;
                Redirect(selected, standby, parked, ref parkedOriginal, ref movedStandby);
                if (edit) editor.TypeText(" later");
                var draft = editor.GetText();
                var previousDialog = DialogManager.ActiveDialog;
                await RejectSaveAsync(host.Page.TestSaveDocumentAsync(editor), previousDialog);
                Check(commits == 1 && editor.IsModified && editor.GetText() == draft && ReferenceEquals(editor.LastSavedSnapshot, snapshot),
                    "Ordinary Save must not publish or rewrite the abandoned independent receipt, with or without newer edits.");
                Check(editor.FileModificationState == FileModificationState.Modified,
                    "Retaining an independent receipt must still report the genuinely redirected bound file as changed.");
            }
            await BytesAsync(original, "original"u8.ToArray(), "The original remains untouched by the independent receipt");
            await BytesAsync(destination, copyBytes, "The picked target retains exactly its first commit");
        }
        finally
        {
            try
            {
                blocker?.Dispose();
            }
            finally
            {
                DocumentFileWriter.TestLocalCommit = null;
                Restore(selected, standby, parked, ref parkedOriginal, ref movedStandby);
            }
        }
        log.AppendLine($"PASS: independent receipt confirm={confirm}, later edit={edit} preserves both files and scopes confirmation to the explicit request.");
    }

    private static void Redirect(string selected, string standby, string parked, ref bool parkedOriginal, ref bool movedStandby)
    {
        Directory.Move(selected, parked);
        parkedOriginal = true;
        try
        {
            Directory.Move(standby, selected);
            movedStandby = true;
        }
        catch (Exception redirectError)
        {
            try { Restore(selected, standby, parked, ref parkedOriginal, ref movedStandby); }
            catch (Exception rollbackError) { throw new AggregateException(redirectError, rollbackError); }
            throw;
        }
    }

    private static void Restore(string selected, string standby, string parked, ref bool parkedOriginal, ref bool movedStandby)
    {
        if (movedStandby)
        {
            Directory.Move(selected, standby);
            movedStandby = false;
        }
        if (parkedOriginal)
        {
            Directory.Move(parked, selected);
            parkedOriginal = false;
        }
    }

    private static async Task<string> CreateFileAsync(string root, string folder, string name, string text)
    {
        var file = await (await StorageFolder.GetFolderFromPathAsync(Path.Combine(root, folder))).CreateFileAsync(name);
        await FileIO.WriteTextAsync(file, text);
        return file.Path;
    }

    private static async Task RejectSaveAsync(Task<bool> save, NotepadsDialog previous)
    {
        await UntilAsync(() => save.IsCompleted || ActiveVisibleDialog() is FileSaveErrorDialog next && !ReferenceEquals(previous, next),
            "The ordinary save must reject the unauthorized target.");
        if (!save.IsCompleted)
        {
            var message = Windows.ApplicationModel.Resources.ResourceLoader.GetForCurrentView().GetString("FileSaveErrorDialog_VersionConflict");
            await ChooseDialogAsync(save, "CloseButton", message);
        }
        Check(!await save, "An ordinary save must reject target changes that only a picked request or its confirmation can authorize.");
    }
}
