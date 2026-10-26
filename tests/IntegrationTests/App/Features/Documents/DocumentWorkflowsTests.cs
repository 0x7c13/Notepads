// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents.Contracts;
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
        await using var host = await FeatureTestHost.CreateAsync();
        await EncodingMenuTests.RunAsync(host, log);
        await EncodingAndLineEndingsAsync(host, log);
        await FailedAndCanceledSaveAsync(host, log);
        await SaveAsAndSaveAllAsync(host, log);
        await ReloadRevertRenameAndShareAsync(host, log);
        await ExternalFileSignalsAsync(host, log);
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
