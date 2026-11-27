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
using Notepads.Presentation.Controls.TextEditor;
using NotepadsIntegrationTests;
using Windows.ApplicationModel.DataTransfer;
using Windows.Data.Json;
using Windows.Storage;
using static NotepadsWorkspaceTests.DocumentFeatureAssertions;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsWorkspaceTests;

internal static class BrokeredDocumentTests
{
    private const string Original = "brokered original";
    private const uint OpenReparsePoint = 0x00200000;
    private const uint BackupSemantics = 0x02000000;

    internal static async Task RunAsync(FeatureTestHost host, StringBuilder log)
    {
        var path = Path.Combine(ApplicationData.Current.LocalFolder.Path, "document-grants.json");
        var grants = JsonObject.Parse(await File.ReadAllTextAsync(path));
        foreach (var kind in new[] { "canonical", "casing", "junction", "link" })
        {
            if (grants.ContainsKey(kind + "Unavailable"))
            {
                Check(kind is not ("canonical" or "casing"), $"The required {kind} single-file grant must be available.");
                log.AppendLine($"SKIP: brokered {kind} grant: {grants.GetNamedString(kind + "Unavailable")}.");
                continue;
            }
            StorageFile file;
            try { file = await SharedStorageAccessManager.RedeemTokenForFileAsync(grants.GetNamedString(kind)); }
            catch (Exception ex) { throw new InvalidOperationException($"Could not redeem the {kind} grant (0x{ex.HResult:X8})."); }
            Check(await FileIO.ReadTextAsync(file) == Original, $"The redeemed {kind} grant must read the private fixture.");
            var target = await DocumentSaveTarget.ResolveAsync(file, default);
            Check(!string.IsNullOrEmpty(target.FinalPath), $"The brokered local {kind} file must expose a canonical location.");
            var parent = Path.GetDirectoryName(file.Path);
            int sibling = DocumentSaveTarget.TestRawPathAccess(Path.Combine(parent, "ungranted.txt"), 1, 0);
            int rawData = DocumentSaveTarget.TestRawPathAccess(file.Path, 1, 0);
            int rawParentList = DocumentSaveTarget.TestRawPathAccess(parent, 1, BackupSemantics);
            int rawParentTraverse = DocumentSaveTarget.TestRawPathAccess(parent, 0x20, OpenReparsePoint | BackupSemantics);
            int rawCanonical = DocumentSaveTarget.TestRawPathAccess(target.FinalPath, 0, OpenReparsePoint);
            log.AppendLine($"Brokered {kind} {file.Name}: Win32 file data={rawData}, parent list={rawParentList}, " +
                $"parent traverse={rawParentTraverse}, canonical entry metadata={rawCanonical}, ungranted sibling data={sibling}.");
            log.AppendLine($"Brokered {kind}: {await Task.Run(() => DocumentSaveTarget.TestDescribeGrantedTarget(file))}.");
            try
            {
                var grantedParent = await file.GetParentAsync();
                log.AppendLine($"Brokered {kind} GetParentAsync returned {(grantedParent == null ? "null" : "a folder")}.");
            }
            catch (UnauthorizedAccessException) { log.AppendLine($"Brokered {kind} GetParentAsync denied access."); }
            if (sibling == 0 || rawParentList == 0)
            {
                Check(kind is not ("canonical" or "casing"), $"The required {kind} grant must establish single-file isolation.");
                log.AppendLine($"SKIP: this {kind} fixture has direct sibling data or parent directory listing access; the environment cannot establish single-file isolation.");
                continue;
            }
            if (kind == "link")
            {
                var metadata = await file.GetBasicPropertiesAsync();
                await using var input = await file.OpenStreamForReadAsync();
                Check(input.Length == Original.Length, "The brokered leaf link must supply its nonempty target stream.");
                log.AppendLine($"Brokered link metadata length={metadata.Size}, stream length={input.Length}.");
                await ThrowsAsync<NotSupportedException>(async () =>
                {
                    using var oversized = await DocumentTextPipeline.DecodeFileAsync(file, new DocumentLoadOptions(),
                        host.Core.DocumentOwnerId, maximumInputBytes: input.Length - 1);
                }, "The input limit must use the readable stream length of the brokered link.");
            }
            // Check the platform capability on this very grant before asking the editor to save.
            bool canTransact = true;
            try { using var transaction = await file.OpenTransactedWriteAsync(); }
            catch (Exception ex) when (kind == "link" && ex.HResult == unchecked((int)0x800705B8))
            {
                canTransact = false;
                log.AppendLine("Brokered file link supports reading; WinRT refused a transacted write (0x800705B8).");
            }
            bool canRename = kind == "canonical" || kind == "junction";
            if (canRename)
            {
                var previous = file.Name;
                try { await file.RenameAsync("control-" + kind + ".txt", NameCollisionOption.FailIfExists); }
                catch (Exception ex) when (ex.HResult == unchecked((int)0x80070005))
                {
                    canRename = false;
                    log.AppendLine($"Brokered {kind}: WinRT denied renaming the single-file grant.");
                }
                if (canRename) await file.RenameAsync(previous, NameCollisionOption.FailIfExists);
            }
            Check(await host.Page.OpenFileAsync(file), $"The actual workspace editor must admit the brokered {kind} file.");
            var editor = (TextEditor)host.Core.GetSelectedReadyEditor();
            await editor.TestStopFileChecksAsync();
            Check(editor.GetText() == Original, $"The editor must decode the complete brokered {kind} file.");
            if (!canTransact)
            {
                await host.Page.TestCloseTabAsync(host.Core.GetTab(editor));
                continue;
            }
            editor.TypeText(" saved");
            string expected = editor.GetText();
            StorageFile saved;
            // The installed app can have a file grant with no normalized DOS spelling.
            if (kind == "canonical") DocumentSaveTarget.TestHideNormalizedDosPath = path => path == file.Path;
            try { saved = await editor.SaveContentToFileAndUpdateEditorStateAsync(editor.EditingFile); }
            catch (DocumentSaveException ex)
            {
                log.AppendLine($"Brokered {kind} editor save failed at {ex.Stage} (HRESULT 0x{ex.HResult:X8}, " +
                    $"expected={target.FinalPath}, {await Task.Run(() => DocumentSaveTarget.TestDescribeGrantedTarget(file))}).");
                throw;
            }
            finally { DocumentSaveTarget.TestHideNormalizedDosPath = null; }
            Check(saved != null && !editor.IsModified && await FileIO.ReadTextAsync(saved) == expected,
                $"Saving the writable brokered {kind} grant must verify all bytes and clear the modified flag.");
            var after = await DocumentSaveTarget.ResolveAsync(saved, default);
            Check(string.Equals(Path.GetDirectoryName(target.FinalPath), Path.GetDirectoryName(after.FinalPath), StringComparison.Ordinal) &&
                string.Equals(Path.GetFileName(target.FinalPath), Path.GetFileName(after.FinalPath), StringComparison.OrdinalIgnoreCase),
                $"Saving the private {kind} fixture may change only its leaf spelling.");
            log.AppendLine($"PASS: brokered {kind} verified save (prior leaf {Path.GetFileName(target.FinalPath)}, current {Path.GetFileName(after.FinalPath)}).");
            if (kind == "canonical") log.AppendLine("PASS: a brokered local save still verifies and commits without the normalized DOS spelling.");
            if (canRename)
            {
                var name = "editor-" + kind + ".txt";
                await editor.RenameAsync(name, saved);
                Check(editor.EditingFileName == name && await FileIO.ReadTextAsync(editor.EditingFile) == expected,
                    $"Editor rename must retain the bytes and binding when the brokered {kind} grant supports rename.");
                log.AppendLine($"PASS: brokered {kind} editor rename retained the saved bytes.");
            }
            await host.Page.TestCloseTabAsync(host.Core.GetTab(editor));
            if (kind == "casing") await VerifyPersistedSpellingAsync(saved, log);
        }
    }

    private static async Task VerifyPersistedSpellingAsync(StorageFile file, StringBuilder log)
    {
        var initial = await DocumentSaveTarget.ReadVersionAsync(file);
        var leaf = Path.GetFileName(initial.TargetPath);
        var newName = leaf == leaf.ToUpperInvariant() ? leaf.ToLowerInvariant() : leaf.ToUpperInvariant();
        try { await file.RenameAsync("temporary-case-entry.txt", NameCollisionOption.FailIfExists); }
        catch (UnauthorizedAccessException)
        {
            log.AppendLine("SKIP: WinRT denied changing the brokered file's case on disk after saving.");
            return;
        }
        await file.RenameAsync(newName, NameCollisionOption.FailIfExists);
        var renamed = await DocumentSaveTarget.ReadVersionAsync(file);
        Check(renamed.TargetPath != initial.TargetPath && string.Equals(renamed.TargetPath, initial.TargetPath, StringComparison.OrdinalIgnoreCase) &&
            renamed.Sha256 == initial.Sha256 && renamed.ModifiedTime == initial.ModifiedTime,
            "The brokered rename must change only the spelling of the saved version.");
        // A persisted revision carries no live receipt. Require evidence from opening
        // an entry by name before associating its old spelling with the current grant.
        bool canProve = DocumentSaveTarget.TestRawEntrySpelling(initial.TargetPath) == renamed.TargetPath ||
            DocumentSaveTarget.TestRawEntrySpelling(renamed.TargetPath) == initial.TargetPath;
        if (!canProve)
        {
            try
            {
                await DocumentFileWriter.WriteAsync(file, _ => throw new InvalidOperationException("An unproven spelling prepared a write."),
                    expectedVersion: initial);
                throw new InvalidOperationException("An unproven persisted spelling was accepted.");
            }
            catch (DocumentSaveException ex) when (ex.Stage == DocumentSaveStage.Conflict) { }
            var retained = await DocumentSaveTarget.ReadVersionAsync(file);
            Check(retained == renamed, "Declining the opaque persisted spelling must retain the entire disk version.");
            log.AppendLine("PASS: both raw spellings lacked independent proof; the persisted version returned Conflict and retained its bytes.");
            return;
        }
        var bytes = "recovered version"u8.ToArray();
        var saved = await DocumentFileWriter.WriteAsync(file, stream => stream.WriteAsync(bytes, 0, bytes.Length), expectedVersion: initial);
        Check(saved.Version.IsVerified, "A prior verified version must authorize a write after a brokered leaf case change.");
        await BytesAsync(saved.File, bytes, "Persisted brokered case variant must retain the saved bytes.");
        log.AppendLine("PASS: a brokered file with a real leaf-case change accepted its prior disk version.");
    }
}
