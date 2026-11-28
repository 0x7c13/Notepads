// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Documents.IO;
using Notepads.Features.Sessions;
using Notepads.Presentation.Controls.Dialog;
using Notepads.Presentation.Controls.TextEditor;
using NotepadsIntegrationTests;
using Windows.Storage;
using static NotepadsWorkspaceTests.DocumentFeatureAssertions;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsWorkspaceTests;

internal static class RetargetedSaveTests
{
    private enum SaveWindow { Capture, Picker, BetweenSaves }

    internal static async Task RunAsync(FeatureTestHost host, StringBuilder log)
    {
        var streamed = await StorageFile.CreateStreamedFileAsync("provider-document.txt", request => request.Dispose(), null);
        var confirmation = new FileSaveErrorDialog(streamed, "Saved {0}", committed: true);
        Check(string.IsNullOrEmpty(streamed.Path) && (string)confirmation.Content == "Saved " + streamed.Name,
            "A pathless provider's save confirmation must name the document.");
        ProtectLocalLinkToUnreachableShare(log);
        await RenameFileLinkAsync(host, log);
        foreach (var window in Enum.GetValues<SaveWindow>())
            await RetargetAsync(host, log, window);
    }

    private static async Task RenameFileLinkAsync(FeatureTestHost host, StringBuilder log)
    {
        var root = ApplicationData.Current.LocalFolder.Path;
        if (File.Exists(Path.Combine(root, "document-unc-symlink-unavailable.txt"))) return;
        var alias = await StorageFile.GetFileFromPathAsync(Path.Combine(root, "document-alias", "leaf-link.txt"));
        var destination = Path.Combine(root, "document-targets", "leaf-source.txt");
        var original = await File.ReadAllBytesAsync(destination);
        await ThrowsAsync<NotSupportedException>(async () =>
        {
            using var rejected = await DocumentTextPipeline.DecodeFileAsync(alias, new DocumentLoadOptions(),
                host.Core.DocumentOwnerId, maximumInputBytes: original.Length - 1);
        }, "Admission must check the symlink target's actual length.");
        Check(await host.Page.OpenFileAsync(alias).WaitAsync(TimeSpan.FromSeconds(15)), "Open a file symlink through a directory junction.");
        var editor = (TextEditor)host.Core.GetSelectedReadyEditor();
        await editor.TestStopFileChecksAsync();
        editor.TypeText("draft");
        var draft = editor.GetText();
        await editor.RenameAsync("leaf-renamed.txt", alias);
        Check(editor.EditingFileName == "leaf-renamed.txt" && editor.IsModified && editor.GetText() == draft &&
            !File.Exists(Path.Combine(root, "document-targets", "leaf-link.txt")) &&
            new FileInfo(editor.EditingFile.Path).LinkTarget == destination,
            "Renaming the file link must move that directory entry and preserve the editor draft.");
        await using (var stream = await editor.EditingFile.OpenStreamForReadAsync())
            Check(stream.Length == original.Length, "The renamed grant must still read the original target.");
        try
        {
            await editor.SaveContentToFileAndUpdateEditorStateAsync(editor.EditingFile);
            throw new InvalidOperationException("Windows accepted a transacted write on a local file symlink.");
        }
        catch (DocumentSaveException ex) when (ex.HResult == unchecked((int)0x800705B8)) { }
        Check(editor.IsModified && editor.GetText() == draft &&
            (await File.ReadAllBytesAsync(destination)).SequenceEqual(original),
            "An unsupported transacted write must leave the target and draft intact.");
        log.AppendLine("PASS: file symlink rename moves only the link; unsupported in-place save preserves the draft and target.");
    }

    private static void ProtectLocalLinkToUnreachableShare(StringBuilder log)
    {
        var root = ApplicationData.Current.LocalFolder.Path;
        using (DocumentSaveTarget.TestPinNamespace($@"\\localhost\Notepads-Unavailable-{Guid.NewGuid():N}\documents\file.txt"))
            log.AppendLine("PASS: direct UNC namespace protection does not require access to the remote share.");
        var unavailable = Path.Combine(root, "document-unc-symlink-unavailable.txt");
        if (File.Exists(unavailable))
        {
            log.AppendLine("SKIP: the test host cannot create directory symlinks. " + File.ReadAllText(unavailable));
            return;
        }
        var link = Path.Combine(root, "document-unc-link");
        var parked = Path.Combine(root, "document-unc-parked");
        Check(new DirectoryInfo(link).LinkTarget?.StartsWith(@"\\localhost\Notepads-Unavailable-", StringComparison.OrdinalIgnoreCase) == true,
            "The fixture must use a real local symlink that leads to an unavailable UNC share.");
        bool blocked = false;
        using (DocumentSaveTarget.TestPinNamespace(Path.Combine(link, "file.txt")))
        {
            try { Directory.Move(link, parked); }
            catch (Exception ex) when (ex.HResult is unchecked((int)0x80070020) or unchecked((int)0x80070005))
            {
                blocked = true;
            }
            if (!blocked) Directory.Move(parked, link);
            Check(blocked, "The local symlink must remain pinned when the rest of the path is UNC.");
        }
        Directory.Move(link, parked);
        Directory.Move(parked, link);
        log.AppendLine("PASS: a local symlink into UNC remains protected until release without requiring access to the remote share.");
    }

    private static async Task RetargetAsync(FeatureTestHost host, StringBuilder log, SaveWindow window)
    {
        var root = ApplicationData.Current.LocalFolder.Path;
        var selected = Path.Combine(root, "document-retarget");
        var standby = Path.Combine(root, "document-standby");
        var parked = Path.Combine(root, "document-parked");
        var name = $"retarget-{Guid.NewGuid():N}.txt";
        var originalFolder = await StorageFolder.GetFolderFromPathAsync(Path.Combine(root, "document-targets"));
        var otherFolder = await StorageFolder.GetFolderFromPathAsync(Path.Combine(root, "document-other-targets"));
        var original = await originalFolder.CreateFileAsync(name);
        var other = await otherFolder.CreateFileAsync(name);
        var bytes = "same bytes"u8.ToArray();
        await FileIO.WriteBytesAsync(original, bytes);
        await FileIO.WriteBytesAsync(other, bytes);
        File.SetLastWriteTimeUtc(other.Path, File.GetLastWriteTimeUtc(original.Path));
        var alias = await StorageFile.GetFileFromPathAsync(Path.Combine(selected, name));

        Check(await host.Page.OpenFileAsync(other), "Open the other physical file in its own tab.");
        var peer = (TextEditor)host.Core.GetSelectedReadyEditor();
        await peer.TestStopFileChecksAsync();
        peer.TypeText("peer draft");
        var peerDraft = peer.GetText();
        TextEditor editor;
        if (window == SaveWindow.Picker) editor = await OpenAsync(host, name, bytes);
        else
        {
            Check(await host.Page.OpenFileAsync(alias), "Open the target through the directory junction.");
            editor = (TextEditor)host.Core.GetSelectedReadyEditor();
            await editor.TestStopFileChecksAsync();
        }
        if (window == SaveWindow.BetweenSaves)
        {
            await SaveWithPinnedJunctionAsync(host, editor, selected, parked);
            await BytesAsync(other, bytes, "The first save through the pinned junction must leave the peer file untouched");
            bytes = editor.GetEncoding().GetBytes(editor.GetText());
            await BytesAsync(original, bytes, "The first save must reach the authorized directory entry");
            await FileIO.WriteBytesAsync(other, bytes);
            File.SetLastWriteTimeUtc(other.Path, File.GetLastWriteTimeUtc(original.Path));
        }
        editor.TypeText("saving draft");
        var draft = editor.GetText();
        var savedSnapshot = editor.LastSavedSnapshot;
        bool parkedOriginal = false, movedStandby = false;
        void Redirect()
        {
            Directory.Move(selected, parked);
            parkedOriginal = true;
            Directory.Move(standby, selected);
            movedStandby = true;
        }

        try
        {
            if (window == SaveWindow.Picker)
            {
                TestFileSavePicker.Reset();
                TestFileSavePicker.Results.Enqueue(alias);
                host.Page.TestPickedVersionCaptured = _ => Redirect();
            }
            else if (window == SaveWindow.Capture) editor.TestSaveReaderCaptured = () => { Redirect(); return Task.CompletedTask; };
            else await CheckRecoveredTargetAsync(host, editor, original, alias, Redirect);
            var save = host.Page.TestSaveDocumentAsync(editor, saveAs: window == SaveWindow.Picker);
            await UntilAsync(() => save.IsCompleted || DialogManager.ActiveDialog is FileSaveErrorDialog, "Save must report the changed target.");
            Check(!save.IsCompleted && DialogManager.ActiveDialog is FileSaveErrorDialog,
                "A redirected save must show its conflict dialog rather than silently returning false.");
            var conflict = Windows.ApplicationModel.Resources.ResourceLoader.GetForCurrentView().GetString("FileSaveErrorDialog_VersionConflict");
            await ChooseDialogAsync(save, "CloseButton", conflict);
            var succeeded = await save;
            Check(movedStandby, "The junction must have switched after the original target was observed.");
            await BytesAsync(original, bytes, "Preserve the file reached by the first observation");
            await BytesAsync(other, bytes, "Preserve the other tab's identically fingerprinted file at the redirected path");
            Check(!succeeded && editor.IsModified && editor.GetText() == draft && ReferenceEquals(editor.LastSavedSnapshot, savedSnapshot) &&
                peer.IsModified && peer.GetText() == peerDraft, "A redirected save must preserve both drafts and the saved baseline.");
        }
        finally
        {
            host.Page.TestPickedVersionCaptured = null;
            editor.TestSaveReaderCaptured = null;
            if (movedStandby) Directory.Move(selected, standby);
            if (parkedOriginal) Directory.Move(parked, selected);
        }
        log.AppendLine($"PASS: {window} rejects a redirected directory junction with identical bytes and timestamp.");
    }

    private static async Task SaveWithPinnedJunctionAsync(FeatureTestHost host, TextEditor editor, string selected, string parked)
    {
        editor.TypeText("first revision");
        bool blocked = false;
        DocumentSaveTarget.TestOriginalVersionRead = (_, _) =>
        {
            try { Directory.Move(selected, parked); }
            catch (Exception ex) when (ex.HResult is unchecked((int)0x80070020) or unchecked((int)0x80070005))
            {
                blocked = true;
            }
            if (!blocked) Directory.Move(parked, selected);
            return Task.CompletedTask;
        };
        try
        {
            Check(await host.Page.TestSaveDocumentAsync(editor).WaitAsync(TimeSpan.FromSeconds(15)) && !editor.IsModified && blocked,
                "The OS must refuse junction replacement while the open transaction verifies its original bytes.");
        }
        finally { DocumentSaveTarget.TestOriginalVersionRead = null; }
    }

    private static async Task CheckRecoveredTargetAsync(FeatureTestHost host, TextEditor editor, StorageFile original, StorageFile alias, Action redirect)
    {
        var expected = (await DocumentSaveTarget.ResolveAsync(original, default)).FinalPath;
        var controller = host.Page.TestController;
        var backupEnabled = controller.IsBackupEnabled;
        controller.IsBackupEnabled = true;
        try
        {
            Check(await controller.SaveSessionAsync().WaitAsync(TimeSpan.FromSeconds(15)), "Publish the edited tab before redirecting its grant.");
            using var plan = await host.Page.TestDiscoverRecoveryAsync();
            var source = plan.Sources.Single(item => item.Id == editor.Id);
            Check(expected != null && source.Metadata.SavedFileTargetPath == expected,
                "The persisted checkpoint must name the physical target of the first successful save.");
            redirect();
            using var prepared = await RecoveryDocumentPreparation.PrepareAsync(source, host.Core.DocumentOwnerId, new DocumentLoadOptions(), default);
            Check(prepared.Checkpoint != null && prepared.EditingFile == null && prepared.FileAccessBinding == null &&
                prepared.SavedSnapshot.FileVersion.TargetPath == expected,
                "Restart must recover the original target evidence with the dirty document.");
            // Give the recovered editor the same alias spelling that the user originally opened.
            var reopened = await StorageFile.GetFileFromPathAsync(alias.Path);
            var restored = (TextEditor)host.Core.CreateTextEditor(Guid.NewGuid(), reopened, "Recovered alias");
            try
            {
                await restored.RestoreV2Async(prepared.SavedSnapshot, prepared.RecoveryBaseline, prepared.Journal, prepared.Checkpoint,
                    reopened, prepared.Metadata, prepared.TextDirty);
                try
                {
                    await restored.SaveContentToFileAndUpdateEditorStateAsync(reopened);
                    throw new InvalidOperationException("Recovery silently saved the draft through a redirected alias.");
                }
                catch (DocumentSaveException ex) when (ex.Stage == DocumentSaveStage.Conflict) { }
                Check(restored.IsModified && restored.GetText() == editor.GetText(), "The restored draft remains available after the conflict.");
            }
            finally { await host.Core.DiscardUnattachedEditorAsync(restored); }
        }
        finally { controller.IsBackupEnabled = backupEnabled; }
    }
}
