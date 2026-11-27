// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Microsoft.Win32.SafeHandles;
using Notepads.Features.Documents.IO;
using Notepads.Features.Documents.Text;
using Notepads.Presentation.Controls.Dialog;
using Notepads.Presentation.Controls.TextEditor;
using NotepadsIntegrationTests;
using Windows.Storage;
using static NotepadsWorkspaceTests.DocumentFeatureAssertions;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsWorkspaceTests;

internal static partial class ParentDirectoryCasingTests
{
    internal static async Task RunAsync(FeatureTestHost host, StringBuilder log)
    {
        await SaveAfterParentRenameAsync(host, log, nested: false);
        await SaveAfterParentRenameAsync(host, log, nested: true);
        await OriginLostDuringAliasLookupAsync(host, log);
        await RejectParentJunctionAsync(host, log);
    }

    private static async Task<TextEditor> OpenFileAsync(FeatureTestHost host, StorageFile file)
    {
        Check(await host.Page.OpenFileAsync(file), "Open the parent-casing fixture in its actual workspace editor.");
        var editor = (TextEditor)host.Core.GetSelectedReadyEditor();
        await UntilAsync(() => editor.TestNativeCore.IsLoaded && host.Page.IsEnabled,
            "The parent-casing fixture must be interactive before editing.");
        await editor.TestStopFileChecksAsync();
        return editor;
    }

    private static async Task SaveAfterParentRenameAsync(FeatureTestHost host, StringBuilder log, bool nested)
    {
        var fixture = await host.Folder.CreateFolderAsync("parent-case-" + Guid.NewGuid().ToString("N"));
        var parent = await fixture.CreateFolderAsync("Parent");
        var content = nested ? await parent.CreateFolderAsync("Child") : parent;
        var file = await content.CreateFileAsync("Note.txt");
        await FileIO.WriteBytesAsync(file, "base"u8.ToArray());
        var editor = await OpenFileAsync(host, file);
        var historical = editor.LastSavedSnapshot.FileVersion.TargetPath;
        Check(historical?.StartsWith(@"\\?\Volume{", StringComparison.Ordinal) == true,
            "The parent-casing fixture must retain a canonical local historical path.");
        editor.TypeText("saved Ω\r");
        var expected = editor.GetEncoding().GetBytes(
            LineEndingUtility.ApplyLineEnding(editor.GetText(), editor.GetLineEnding()));
        var oldParent = parent.Path;
        var newParent = Path.Combine(fixture.Path, "PARENT");
        var parked = Path.Combine(fixture.Path, "parent-parked");
        Directory.Move(oldParent, parked);
        Directory.Move(parked, newParent);
        if (nested)
        {
            var child = Path.Combine(newParent, "Child");
            var parkedChild = Path.Combine(newParent, "child-parked");
            Directory.Move(child, parkedChild);
            Directory.Move(parkedChild, Path.Combine(newParent, "CHILD"));
            // Refresh the grant's leaf while leaving the saved version unchanged.
            await file.RenameAsync("leaf-case-parked.txt");
            await file.RenameAsync("NOTE.TXT");
            var beforeSave = await DocumentSaveTarget.ResolveAsync(file, default);
            Check(beforeSave.FinalPath?.EndsWith(@"\PARENT\CHILD\NOTE.TXT", StringComparison.Ordinal) == true,
                "The combined fixture must actually change both parent and leaf spelling before Save.");
        }
        var dispatcher = Windows.UI.Xaml.Window.Current.Dispatcher;
        var attempts = 0;
        var protectedMoves = 0;
        DocumentSaveTarget.TestNamedMetadataUnavailable = path =>
        {
            if (!string.Equals(path, historical, StringComparison.Ordinal)) return false;
            Check(!dispatcher.HasThreadAccess, "Parent-case proof metadata queries must run off the editor thread.");
            attempts++;
            bool moved = false;
            try
            {
                Directory.Move(newParent, parked);
                moved = true;
            }
            catch (Exception error) when (error.HResult is unchecked((int)0x80070020) or unchecked((int)0x80070005))
            {
                protectedMoves++;
            }
            finally { if (moved) Directory.Move(parked, newParent); }
            Check(!moved, "The parent must remain pinned while its historical leaf spelling is proved.");
            return false;
        };
        Task<bool> saving = null;
        try
        {
            saving = host.Page.TestSaveDocumentAsync(editor);
            await UntilAsync(() => saving.IsCompleted || ActiveVisibleDialog() is FileSaveErrorDialog,
                "A parent-only case change must finish ordinary Save without an error dialog.");
            if (!saving.IsCompleted)
            {
                await ChooseDialogAsync(saving, "CloseButton");
                throw new InvalidOperationException("Ordinary Save rejected a genuine parent-directory case change.");
            }
            Check(await saving, "A genuine parent-directory case change must permit ordinary Save.");
            Check(attempts > 0 && protectedMoves == attempts,
                "The actual case proof must run with its ancestor protected for every historical-leaf observation.");
            var current = await DocumentSaveTarget.ResolveAsync(editor.EditingFile, default);
            Check(current.FinalPath != null && editor.LastSavedSnapshot.FileVersion.IsVerified &&
                editor.LastSavedSnapshot.FileVersion.TargetPath == current.FinalPath &&
                current.FinalPath.Contains(@"\PARENT\", StringComparison.Ordinal) &&
                (!nested || current.FinalPath.Contains(@"\PARENT\CHILD\", StringComparison.Ordinal)) &&
                !editor.IsModified && !editor.GetTextEditorStateMetaData().HasUnconfirmedFileSave,
                "The verified saved version must publish the new parent spelling and leave the actual tab clean.");
            await BytesAsync(editor.EditingFile, expected, "Saving after parent-case changes preserves exact encoded bytes");
            await VerifySharedCaseLeaseAsync(editor.EditingFile, historical, current);
            var alias = await StorageFile.GetFileFromPathAsync(editor.EditingFile.Path.ToUpperInvariant());
            int tabCount = host.Core.GetTabCount();
            Check(!await host.Page.OpenFileAsync(alias) && host.Core.GetTabCount() == tabCount &&
                ReferenceEquals(host.Core.GetSelectedReadyEditor(), editor),
                "An ordinary case-only file alias must still select the existing tab instead of creating a duplicate.");
        }
        finally
        {
            DocumentSaveTarget.TestNamedMetadataUnavailable = null;
            if (saving != null)
            {
                if (!saving.IsCompleted && ActiveVisibleDialog() is FileSaveErrorDialog)
                    await ChooseDialogAsync(saving, "CloseButton");
                await saving;
            }
        }
        // Proof handles are temporary: normal directory rename is available again.
        await editor.TestStopFileChecksAsync();
        Directory.Move(newParent, parked);
        Directory.Move(parked, newParent);
        log.AppendLine($"PASS: actual ordinary Save accepts parent casing (nested/leaf={nested}), proves it off UI while pinning ancestors, releases handles, and publishes exact bytes and the new saved location.");
    }

    private static async Task VerifySharedCaseLeaseAsync(StorageFile file, string historical,
        DocumentSaveTarget.Identity current)
    {
        using (await DocumentSaveTarget.AcquireAsync(new DocumentSaveTarget.Identity(file, historical), default))
        using (var cancellation = new CancellationTokenSource())
        {
            var waiting = DocumentSaveTarget.AcquireAsync(current, cancellation.Token);
            var canceled = false;
            try
            {
                await Task.WhenAny(waiting, Task.Delay(80));
                Check(!waiting.IsCompleted, "Historical and current parent spellings must contend for the same save lease.");
            }
            finally
            {
                cancellation.Cancel();
                try { using var unexpected = await waiting.WaitAsync(TimeSpan.FromSeconds(5)); }
                catch (OperationCanceledException) when (cancellation.IsCancellationRequested) { canceled = true; }
            }
            Check(canceled, "The contending case-variant lease must honor cancellation while the first lease remains held.");
        }
        using var reacquired = await DocumentSaveTarget.AcquireAsync(current, default);
        Check(reacquired != null, "The current spelling must acquire normally after the historical lease is released.");
    }

    private static async Task OriginLostDuringAliasLookupAsync(FeatureTestHost host, StringBuilder log)
    {
        var suffix = Guid.NewGuid().ToString("N");
        var target = await OpenAsync(host, "case-lookup-target-" + suffix + ".txt", "target"u8.ToArray());
        var origin = await OpenAsync(host, "case-lookup-origin-" + suffix + ".txt", "origin"u8.ToArray());
        var selected = await OpenAsync(host, "case-lookup-selection-" + suffix + ".txt", "selection"u8.ToArray());
        var alias = await StorageFile.GetFileFromPathAsync(target.EditingFile.Path.ToUpperInvariant());
        Check(!string.Equals(alias.Path, target.EditingFile.Path, StringComparison.Ordinal) &&
            string.Equals(alias.Path, target.EditingFile.Path, StringComparison.OrdinalIgnoreCase),
            "The ready-lookup fixture must require filesystem proof of a case-variant file hint.");
        var aliasMetadataPath = @"\\?\" + alias.Path;
        var entered = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var release = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var dispatcher = Windows.UI.Xaml.Window.Current.Dispatcher;
        var before = host.Core.GetTabs();
        var presentations = 0;
        Task<bool> opening = null;
        host.Core.SwitchTo(host.Core.GetTab(origin));
        DocumentSaveTarget.TestNamedMetadataUnavailable = path =>
        {
            if (!string.Equals(path, aliasMetadataPath, StringComparison.Ordinal)) return false;
            Check(!dispatcher.HasThreadAccess, "The held ready-file case query must run off the editor thread.");
            entered.TrySetResult();
            release.Task.GetAwaiter().GetResult();
            return false;
        };
        try
        {
            opening = host.Core.OpenFileAsync(alias, _ =>
            {
                presentations++;
                return Task.CompletedTask;
            }, mayPublish: () => ReferenceEquals(host.Core.GetSelectedReadyEditor(), origin));
            await entered.Task.WaitAsync(TimeSpan.FromSeconds(10));
            Check(!opening.IsCompleted, "The ready-file match must still be waiting for its actual filesystem query.");
            host.Core.SwitchTo(host.Core.GetTab(selected));
            release.TrySetResult();
            Check(!await opening.WaitAsync(TimeSpan.FromSeconds(15)) &&
                ReferenceEquals(host.Core.GetSelectedReadyEditor(), selected) &&
                host.Core.GetTabs().SequenceEqual(before) && presentations == 0,
                "An alias open whose origin changed during ready lookup must not switch tabs, create a tab or present a late result.");
        }
        finally
        {
            release.TrySetResult();
            DocumentSaveTarget.TestNamedMetadataUnavailable = null;
            if (opening != null) await opening.WaitAsync(TimeSpan.FromSeconds(15));
        }
        log.AppendLine("PASS: a case-variant ready-file lookup yields to a changed originating tab without late selection, tab creation or result presentation.");
    }

    private static async Task RejectParentJunctionAsync(FeatureTestHost host, StringBuilder log)
    {
        var root = Path.Combine(ApplicationData.Current.LocalFolder.Path, "document-parent-case");
        var lowerPath = Path.Combine(root, "parent");
        var upperPath = Path.Combine(root, "PARENT");
        var redirect = Path.Combine(root, "redirect");
        var parked = Path.Combine(root, "original-parked");
        // The desktop supplies the junction. Enable case sensitivity on an empty
        // directory, then restore that fixture before opening either document.
        var setup = Path.Combine(ApplicationData.Current.LocalFolder.Path, "document-parent-case-setup");
        Directory.CreateDirectory(setup);
        bool upperStaged = false, linkStaged = false;
        try
        {
            Directory.Move(upperPath, Path.Combine(setup, "PARENT"));
            upperStaged = true;
            Directory.Move(redirect, Path.Combine(setup, "redirect"));
            linkStaged = true;
            using var handle = OpenCaseDirectory(root, 0x100 /* FILE_WRITE_ATTRIBUTES */,
                FileShare.ReadWrite | FileShare.Delete, IntPtr.Zero, FileMode.Open, 0x02000000 /* BACKUP_SEMANTICS */, IntPtr.Zero);
            uint enabled = 1;
            Check(!handle.IsInvalid && SetCaseInformation(handle, 23 /* FileCaseSensitiveInfo */, in enabled, sizeof(uint)),
                "The empty parent-redirect fixture must enable real directory case sensitivity (Win32 " + Marshal.GetLastPInvokeError() + ").");
        }
        finally
        {
            try { if (upperStaged) Directory.Move(Path.Combine(setup, "PARENT"), upperPath); }
            finally { if (linkStaged) Directory.Move(Path.Combine(setup, "redirect"), redirect); }
        }
        Directory.CreateDirectory(lowerPath);
        const string name = "parent-collision.txt";
        var originalPath = Path.Combine(lowerPath, name);
        var otherPath = Path.Combine(upperPath, name);
        var bytes = "same bytes"u8.ToArray();
        File.WriteAllBytes(originalPath, bytes);
        File.WriteAllBytes(otherPath, bytes);
        File.SetLastWriteTimeUtc(otherPath, File.GetLastWriteTimeUtc(originalPath));
        var original = await StorageFile.GetFileFromPathAsync(originalPath);
        var other = await StorageFile.GetFileFromPathAsync(otherPath);
        var openingOriginal = host.Page.OpenFileAsync(original);
        var openingOther = host.Page.OpenFileAsync(other);
        var opened = await Task.WhenAll(openingOriginal, openingOther).WaitAsync(TimeSpan.FromSeconds(15));
        Check(opened.All(created => created),
            "Concurrent opens of distinct case-sensitive parent entries must create two tabs rather than coalesce provider hints.");
        var editor = host.Core.GetTabs().Select(tab => tab.ReadyEditor).OfType<TextEditor>()
            .Single(ready => ReferenceEquals(ready.EditingFile, original));
        var peer = host.Core.GetTabs().Select(tab => tab.ReadyEditor).OfType<TextEditor>()
            .Single(ready => ReferenceEquals(ready.EditingFile, other));
        host.Core.SwitchTo(host.Core.GetTab(editor));
        await UntilAsync(() => editor.TestNativeCore.IsLoaded && host.Page.IsEnabled,
            "The first case-sensitive document must be interactive before editing.");
        await editor.TestStopFileChecksAsync();
        editor.TypeText("original draft");
        var draft = editor.GetText();
        var snapshot = editor.LastSavedSnapshot;
        Check(!ReferenceEquals(editor, peer), "Different case-sensitive parent directories must open as two actual tabs.");
        host.Core.SwitchTo(host.Core.GetTab(peer));
        await UntilAsync(() => peer.TestNativeCore.IsLoaded && host.Page.IsEnabled,
            "The second case-sensitive document must be interactive before editing.");
        await peer.TestStopFileChecksAsync();
        peer.TypeText("peer draft");
        var peerDraft = peer.GetText();
        var otherVersion = peer.LastSavedSnapshot.FileVersion;
        Check(!string.Equals(snapshot.FileVersion.TargetPath, otherVersion.TargetPath, StringComparison.Ordinal) &&
            string.Equals(snapshot.FileVersion.TargetPath, otherVersion.TargetPath, StringComparison.OrdinalIgnoreCase) &&
            snapshot.FileVersion.Sha256 == otherVersion.Sha256 && snapshot.FileVersion.ModifiedTime == otherVersion.ModifiedTime,
            "The distinct parent entries must differ only by case while their bytes and timestamps match.");
        Check(!await DocumentSaveTarget.SameLocationAsync(snapshot.FileVersion.TargetPath, otherVersion.TargetPath),
            "Unrelated case-sensitive parents must not prove the same location even when contents match.");
        var unknownOriginal = new DocumentSaveTarget.Identity(original, null);
        var knownPeer = await DocumentSaveTarget.ResolveAsync(other, default);
        Check(await unknownOriginal.MatchesAsync(knownPeer) == null &&
            await knownPeer.MatchesAsync(unknownOriginal) == null,
            "Missing canonical metadata must keep a case-ambiguous provider hint indeterminate in both directions.");
        bool parkedOriginal = false, movedLink = false;
        Task<bool> saving = null;
        try
        {
            Directory.Move(lowerPath, parked);
            parkedOriginal = true;
            Directory.Move(redirect, lowerPath);
            movedLink = true;
            saving = host.Page.TestSaveDocumentAsync(editor);
            await UntilAsync(() => saving.IsCompleted || ActiveVisibleDialog() is FileSaveErrorDialog,
                "A parent junction to the case-variant peer must report a conflict.");
            Check(!saving.IsCompleted && ActiveVisibleDialog() is FileSaveErrorDialog,
                "A redirected parent must show the actual conflict dialog rather than save or silently return.");
            var conflict = Windows.ApplicationModel.Resources.ResourceLoader.GetForCurrentView().GetString("FileSaveErrorDialog_VersionConflict");
            await ChooseDialogAsync(saving, "CloseButton", conflict);
            Check(!await saving && editor.IsModified && editor.GetText() == draft &&
                ReferenceEquals(snapshot, editor.LastSavedSnapshot) && peer.IsModified && peer.GetText() == peerDraft,
                "Rejecting the case-variant parent junction must retain both drafts and the original saved version.");
            await BytesAsync(await StorageFile.GetFileFromPathAsync(Path.Combine(parked, name)), bytes,
                "The parked original file remains unchanged after the redirected save");
            await BytesAsync(other, bytes, "The identically fingerprinted peer file remains unchanged after the redirected save");
        }
        finally
        {
            try
            {
                if (saving != null)
                {
                    if (!saving.IsCompleted && ActiveVisibleDialog() is FileSaveErrorDialog)
                        await ChooseDialogAsync(saving, "CloseButton");
                    await saving;
                }
            }
            finally
            {
                if (movedLink) Directory.Move(lowerPath, redirect);
                if (parkedOriginal) Directory.Move(parked, lowerPath);
            }
        }
        log.AppendLine("PASS: an actual two-tab save rejects a parent junction to a distinct case-sensitive parent with identical bytes and timestamp, preserving both files, drafts and saved-version authority.");
        await VerifyCheckpointGrantAfterSaveAsAsync(host, editor, other, log);
    }

    private static async Task VerifyCheckpointGrantAfterSaveAsAsync(FeatureTestHost host, TextEditor editor,
        StorageFile destination, StringBuilder log)
    {
        var controller = host.Page.TestController;
        var backupEnabled = controller.IsBackupEnabled;
        controller.IsBackupEnabled = true;
        Task<bool> saving = null;
        try
        {
            Check(await controller.SaveSessionAsync().WaitAsync(TimeSpan.FromSeconds(15)),
                "The original case-sensitive binding must have a real committed session checkpoint.");
            var originalTarget = (await DocumentSaveTarget.ResolveAsync(editor.EditingFile, default)).FinalPath;
            var oldToken = await AssertCheckpointGrantAsync(host, editor, originalTarget);
            host.Core.SwitchTo(host.Core.GetTab(editor));
            await UntilAsync(() => editor.TestNativeCore.IsLoaded && host.Page.IsEnabled,
                "The source tab must be interactive for the explicit case-sensitive Save As.");
            var expected = editor.GetEncoding().GetBytes(
                LineEndingUtility.ApplyLineEnding(editor.GetText(), editor.GetLineEnding()));
            TestFileSavePicker.Reset();
            TestFileSavePicker.Results.Enqueue(destination);
            saving = host.Page.TestSaveDocumentAsync(editor, saveAs: true);
            await UntilAsync(() => saving.IsCompleted || ActiveVisibleDialog() is FileSaveErrorDialog,
                "An explicit Save As to the distinct case-sensitive peer must finish without an error dialog.");
            if (!saving.IsCompleted)
            {
                await ChooseDialogAsync(saving, "CloseButton");
                throw new InvalidOperationException("The explicit case-sensitive Save As unexpectedly failed.");
            }
            Check(await saving && !editor.IsModified, "The explicit picked save must establish its new verified binding.");
            var newTarget = (await DocumentSaveTarget.ResolveAsync(editor.EditingFile, default)).FinalPath;
            Check(newTarget != originalTarget && string.Equals(newTarget, originalTarget, StringComparison.OrdinalIgnoreCase),
                "The picked save must move the binding between distinct parents whose names differ only by case.");
            await BytesAsync(destination, expected, "The explicit case-sensitive Save As writes the selected destination");
            Check(await controller.SaveSessionAsync().WaitAsync(TimeSpan.FromSeconds(15)),
                "The changed case-sensitive binding must publish a new real checkpoint.");
            var newToken = await AssertCheckpointGrantAsync(host, editor, newTarget);
            Check(newToken != oldToken, "A checkpoint must not reuse the old parent's grant through the shell's case-folding IsEqual hint.");
        }
        finally
        {
            TestFileSavePicker.Reset();
            try
            {
                if (saving != null)
                {
                    if (!saving.IsCompleted && ActiveVisibleDialog() is FileSaveErrorDialog)
                        await ChooseDialogAsync(saving, "CloseButton");
                    await saving;
                }
            }
            finally { controller.IsBackupEnabled = backupEnabled; }
        }
        await editor.TestStopFileChecksAsync();
        log.AppendLine("PASS: real session checkpoints before and after case-sensitive Save As retain a grant resolving to the selected parent rather than reusing the original parent's token.");
    }

    private static async Task<string> AssertCheckpointGrantAsync(FeatureTestHost host, TextEditor editor, string expectedTarget)
    {
        using var plan = await host.Page.TestDiscoverRecoveryAsync();
        var source = plan.Sources.Single(item => item.Id == editor.Id);
        var descriptor = source.GetDescriptor(-1);
        Check(expectedTarget != null && descriptor?.EditingFileFutureAccessToken != null &&
            descriptor.StateMetaData.SavedFileTargetPath == expectedTarget,
            "The durable descriptor must retain its actual saved target and a file-access token.");
        var granted = await source.ResolveEditingFileAsync().WaitAsync(TimeSpan.FromSeconds(10));
        var resolved = await DocumentSaveTarget.ResolveAsync(granted, default);
        Check(resolved?.FinalPath == expectedTarget,
            "The durable source's actual future-access grant must resolve to its selected canonical parent.");
        return descriptor.EditingFileFutureAccessToken;
    }

    [LibraryImport("api-ms-win-core-file-fromapp-l1-1-0.dll", EntryPoint = "CreateFileFromAppW",
        StringMarshalling = StringMarshalling.Utf16, SetLastError = true)]
    private static partial SafeFileHandle OpenCaseDirectory(string path, uint access, FileShare share,
        IntPtr security, FileMode disposition, uint flags, IntPtr template);

    [LibraryImport("api-ms-win-core-file-l1-2-0.dll", EntryPoint = "SetFileInformationByHandle", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool SetCaseInformation(SafeFileHandle handle, int infoClass, in uint flags, uint size);
}
