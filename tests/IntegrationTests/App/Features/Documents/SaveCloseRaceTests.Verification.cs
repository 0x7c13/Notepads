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

internal static partial class SaveCloseRaceTests
{
    private static async Task MovedReceiptAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var editor = await OpenAsync(host, "receipt-original-name.txt", "base"u8.ToArray());
        var file = editor.EditingFile;
        editor.TypeText("draft");
        var expected = editor.GetEncoding().GetBytes(editor.GetText());
        FileStream blocker = null;
        DocumentFileWriter.TestLocalCommit = target => blocker = File.Open(target.Path, FileMode.Open, FileAccess.Read, FileShare.None);
        Guid id;
        try
        {
            try
            {
                await editor.SaveContentToFileAndUpdateEditorStateAsync(file);
                throw new InvalidOperationException("A blocked post-commit read must retain a confirmation receipt.");
            }
            catch (DocumentSaveException error) when (error.Stage == DocumentSaveStage.ConfirmCommit && error.VerificationId.HasValue)
            { id = error.VerificationId.Value; }
        }
        finally { blocker?.Dispose(); DocumentFileWriter.TestLocalCommit = null; }
        Check(editor.FileModificationState == FileModificationState.Unverified, "The bound commit starts unconfirmed.");
        // Change the grant outside the editor's acknowledged rename flow.
        await file.RenameAsync("receipt-external-name.txt");
        try
        {
            await editor.SaveContentToFileAndUpdateEditorStateAsync(file, verificationId: id);
            throw new InvalidOperationException("Confirmation must reject a receipt whose original target moved.");
        }
        catch (DocumentSaveException error) when (error.Stage is DocumentSaveStage.Conflict or DocumentSaveStage.Superseded) { }
        await editor.TestCheckFileStatusAsync();
        Check(editor.FileModificationState == FileModificationState.Modified && editor.GetTextEditorStateMetaData().HasUnconfirmedFileSave,
            "A superseded bound commit must remain Modified after polling and retain its restart safety flag.");
        Check(editor.TestPendingSaveId == null, "Target mismatch must release the obsolete receipt and its retained revision.");
        await BytesAsync(file, expected, "The externally renamed file keeps the exact committed revision");
        log.AppendLine("PASS: confirming an externally moved bound receipt retires its revision and preserves the Modified warning and file bytes.");
    }

    private static async Task UnexpectedPickedCancellationAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var editor = await OpenAsync(host, "unexpected-cancel-source.txt", "base"u8.ToArray());
        editor.TypeText("draft");
        var target = await host.Folder.CreateFileAsync("unexpected-cancel-target.txt");
        TestFileSavePicker.Reset();
        TestFileSavePicker.Results.Enqueue(target);
        DocumentSaveTarget.TestVersionRead = (file, _) => ReferenceEquals(file, target)
            ? Task.FromException(new OperationCanceledException("Provider canceled without an application cancellation."))
            : Task.CompletedTask;
        Task<bool> saving = null;
        try
        {
            saving = host.Page.TestSaveDocumentAsync(editor, saveAs: true);
            await UntilAsync(() => saving.IsCompleted || ActiveVisibleDialog() is FileSaveErrorDialog,
                "An unexpected provider cancellation must produce a save error.");
            Check(!saving.IsCompleted && ActiveVisibleDialog() is FileSaveErrorDialog,
                "A provider cancellation with a live application token must not silently drop Save As.");
            await ChooseDialogAsync(saving, "CloseButton");
            Check(!await saving && editor.IsModified, "Dismissing the error preserves the draft.");
            await BytesAsync(target, Array.Empty<byte>(), "Unexpected cancellation must not write the picked destination");
        }
        finally
        {
            DocumentSaveTarget.TestVersionRead = null;
            if (saving != null)
            {
                if (!saving.IsCompleted && ActiveVisibleDialog() is FileSaveErrorDialog) await ChooseDialogAsync(saving, "CloseButton");
                await saving;
            }
        }
        log.AppendLine("PASS: an unrequested provider cancellation shows the actual save error and keeps both destination and draft intact.");
    }
}
