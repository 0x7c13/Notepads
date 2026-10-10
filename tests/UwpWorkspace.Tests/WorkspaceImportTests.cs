// Copyright (c) 2026, Jiaqi (0x7c13) Liu. See LICENSE.
using System;
using System.IO;
using System.Linq;
using System.Text;
using System.Text.Json;
using System.Threading.Tasks;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Contracts.Legacy;
using Notepads.Features.Sessions.Storage;
using Notepads.Presentation.Views.MainPage;
using Notepads.Presentation.Workspace;
using Windows.Storage;
using Windows.Storage.AccessCache;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsWorkspaceTests;

internal static class WorkspaceImportTests
{
    private static string Root => ApplicationData.Current.LocalFolder.Path;
    private static string FixturePath => Path.Combine(Root, "import.ids");
    private static readonly string[] Names = ["selected.txt", "target.txt", "token-only.txt"];
    private static readonly string[] DiskTexts = ["saved selected Ω\r", "saved target 世界\r", "saved token 😀\r"];
    private static readonly string[] RecoveryTexts =
        ["recovered selected Ω😀\r", "recovered target 世界 e\u0301\r", "recovered token Україна𐐷\r"];
    private const string UnrelatedName = "unrelated.txt";
    private const string UnrelatedText = "unrelated disk document العربية\r";

    internal static Guid ReadInitialSelection() => ReadIds()[0];

    internal static async Task VerifyEarlyActivationAsync(NotepadsMainPage page, StringBuilder log)
    {
        Check(!page.TestStartupComplete, "Early activation fixture was attached after MainPage startup.");
        var files = await ApplicationData.Current.LocalFolder.GetFolderAsync("ImportFiles");
        var selected = await files.GetFileAsync(Names[0]);
        Check(!await page.OpenFileAsync(selected).WaitAsync(TimeSpan.FromSeconds(20)),
            "Activation before Sets_Loaded opened disk text instead of observing requested recovery.");
        VerifyFile(page.TestCore, selected, ReadIds()[0], RecoveryTexts[0]);
        Check(!page.TestStartupComplete && ((NotepadsCore)page.TestCore).TestOwnedEditorCount == 1,
            "Early activation bypassed selected-first recovery or depended on Sets_Loaded.");
        log.AppendLine("PASS: actual MainPage activation before Sets_Loaded restores the selected dirty file exactly once.");
    }

    internal static async Task SeedAsync(Guid owner, StringBuilder log)
    {
        var ids = Names.Select(_ => Guid.NewGuid()).ToArray();
        var files = await ApplicationData.Current.LocalFolder.CreateFolderAsync(
            "ImportFiles", CreationCollisionOption.FailIfExists);
        var backups = await ApplicationData.Current.LocalFolder.CreateFolderAsync(
            SessionScopeData.GetSecondaryPrefix(owner) + "BackupFiles", CreationCollisionOption.FailIfExists);
        var legacy = new NotepadsSessionDataV1 { SelectedTextEditor = ids[0] };
        for (var index = 0; index < ids.Length; index++)
        {
            var file = await files.CreateFileAsync(Names[index], CreationCollisionOption.FailIfExists);
            await FileIO.WriteTextAsync(file, DiskTexts[index]);
            var saved = await backups.CreateFileAsync($"saved-{index}.txt", CreationCollisionOption.FailIfExists);
            var pending = await backups.CreateFileAsync($"pending-{index}.txt", CreationCollisionOption.FailIfExists);
            await FileIO.WriteTextAsync(saved, DiskTexts[index]);
            await FileIO.WriteTextAsync(pending, RecoveryTexts[index]);
            var token = SessionDocumentStore.GetFutureAccessTokenPrefix(owner) + ids[index].ToString("N");
            StorageApplicationPermissions.FutureAccessList.AddOrReplace(token, file);
            legacy.TextEditors.Add(new TextEditorSessionDataV1
            {
                Id = ids[index],
                LastSavedBackupFilePath = saved.Path,
                PendingBackupFilePath = pending.Path,
                EditingFileFutureAccessToken = token,
                EditingFileName = file.Name,
                // Force the last lookup to use broker identity rather than a saved path.
                EditingFilePath = index == 2 ? null : file.Path,
                StateMetaData = new DocumentMetadata
                {
                    LastSavedEncoding = "UTF-8",
                    LastSavedLineEnding = "CR",
                    FontZoomFactor = 1,
                    HasEditingFile = true,
                    IsModified = true,
                    FileNamePlaceholder = file.Name
                }
            });
        }
        var unrelated = await files.CreateFileAsync(UnrelatedName, CreationCollisionOption.FailIfExists);
        await FileIO.WriteTextAsync(unrelated, UnrelatedText);
        var manifest = await ApplicationData.Current.LocalFolder.CreateFileAsync(
            SessionScopeData.GetManifestFileName(owner), CreationCollisionOption.FailIfExists);
        await FileIO.WriteTextAsync(manifest,
            JsonSerializer.Serialize(legacy, SessionJsonContext.Default.NotepadsSessionDataV1));
        File.WriteAllLines(FixturePath, ids.Select(id => id.ToString("N")));
        log.AppendLine("PASS: fixture retains three dirty V1 documents, including token-only file identity.");
    }

    internal static async Task RunAsync(NotepadsMainPage page, RestorePreparationGate gate,
        string phase, StringBuilder log)
    {
        await UntilAsync(() => page.TestStartupComplete, "Imported selected-tab startup did not finish.");
        page.TestController.StopSessionBackup();
        if (phase.EndsWith("-restart", StringComparison.Ordinal))
        {
            await VerifyRestartAsync(page, log);
            return;
        }
        if (phase == "--workspace-import-failed")
        {
            await VerifyFailedImportAsync(page, gate, log);
            return;
        }

        var core = page.TestCore;
        var ids = ReadIds();
        var files = await ApplicationData.Current.LocalFolder.GetFolderAsync("ImportFiles");
        var target = await files.GetFileAsync(Names[1]);
        var tokenOnly = await files.GetFileAsync(Names[2]);
        var unrelated = await files.GetFileAsync(UnrelatedName);
        await UntilAsync(() => gate.Admitted.Contains(ids[1]), "The target import did not reach preparation.");
        Check(core.GetTabs().Select(tab => tab.Id).SequenceEqual(new[] { ids[0] }) &&
            ((NotepadsCore)core).TestOwnedEditorCount == 1,
            "Startup must expose only the selected imported native editor while target preparation is blocked.");

        var firstOpen = page.OpenFileAsync(target);
        var secondOpen = page.OpenFileAsync(target);
        var lookup = page.TestController.ResolveFileAsync(target);
        // Resolution waits for recovery; the synchronous ready lookup stays pure.
        Check(core.FindReadyTab(target) == null, "Ready lookup materialized a hidden import.");
        Check(!lookup.IsCompleted, "File lookup bypassed the hidden dirty import while its preparation was blocked.");
        Check(await page.OpenFileAsync(unrelated).WaitAsync(TimeSpan.FromSeconds(10)),
            "An unrelated disk file was blocked by hidden import restoration.");
        Check(!firstOpen.IsCompleted && !secondOpen.IsCompleted && !lookup.IsCompleted &&
            core.GetTabs().Length == 2 && ((NotepadsCore)core).TestOwnedEditorCount == 2,
            "Concurrent target opens materialized disk content before the retained import was adopted.");
        log.AppendLine("PASS: hidden import lookup waits; concurrent opens coalesce and unrelated files open.");

        gate.Release(ids[1]);
        await Task.WhenAll(firstOpen, secondOpen, lookup).WaitAsync(TimeSpan.FromSeconds(20));
        var restored = await lookup;
        Check(restored?.Id == ids[1] && !await firstOpen && !await secondOpen,
            "Opening an importing file created a disk editor instead of selecting its recovered tab.");
        Check(ReferenceEquals(core.GetSelectedTab(), restored), "The completed import open selected the wrong tab.");
        VerifyFile(core, target, ids[1], RecoveryTexts[1]);
        await UntilAsync(() => gate.Admitted.Contains(ids[2]), "The token-only import did not reach preparation.");
        Check(core.GetTabs().Length == 3 && ((NotepadsCore)core).TestOwnedEditorCount == 3,
            "Opening the matching import waited for or materialized the later blocked import.");

        var tokenOpen = page.OpenFileAsync(tokenOnly);
        var tokenLookup = page.TestController.ResolveFileAsync(tokenOnly);
        Check(ReferenceEquals(await page.TestController.ResolveFileAsync(unrelated).WaitAsync(TimeSpan.FromSeconds(10)),
                core.GetTabs().Single(tab => tab.ReadyEditor?.EditingFile?.IsEqual(unrelated) == true)),
            "An unrelated ready file lookup waited for a token-only import.");
        Check(!tokenOpen.IsCompleted && !tokenLookup.IsCompleted,
            "A blank saved path allowed opening a second editor for a file retained by broker identity.");
        gate.Release(ids[2]);
        await Task.WhenAll(tokenOpen, tokenLookup).WaitAsync(TimeSpan.FromSeconds(20));
        Check((await tokenLookup)?.Id == ids[2] && !await tokenOpen,
            "Token-only file lookup did not select the retained imported document.");
        VerifyFile(core, tokenOnly, ids[2], RecoveryTexts[2]);
        Check(ReferenceEquals(core.GetSelectedTab(), await tokenLookup), "Token-only open did not select its import.");
        await page.TestController.WaitForRestorationCheckpointAsync().WaitAsync(TimeSpan.FromSeconds(20));
        Check(await page.TestController.SaveSessionAsync(), "Could not checkpoint imported documents after file opens.");
        Check(await FileIO.ReadTextAsync(target) == DiskTexts[1] &&
            await FileIO.ReadTextAsync(tokenOnly) == DiskTexts[2],
            "Import deduplication overwrote either original disk file with recovered edits.");
        log.AppendLine("PASS: path and token-only opens select their recovery without waiting for later imports.");
    }

    private static async Task VerifyRestartAsync(NotepadsMainPage page, StringBuilder log)
    {
        await page.TestController.WaitForRestorationAsync().WaitAsync(TimeSpan.FromSeconds(20));
        var core = page.TestCore;
        var ids = ReadIds();
        var files = await ApplicationData.Current.LocalFolder.GetFolderAsync("ImportFiles");
        Check(core.GetTabs().Length == ids.Length + 1 && core.GetSelectedTab()?.Id == ids[2],
            "Restart duplicated an imported document or lost the last explicit file-open selection.");
        for (var index = 0; index < ids.Length; index++)
            VerifyFile(core, await files.GetFileAsync(Names[index]), ids[index], RecoveryTexts[index]);
        var unrelated = await files.GetFileAsync(UnrelatedName);
        var unrelatedTab = core.GetTabs().Single(tab => tab.ReadyEditor?.EditingFile?.IsEqual(unrelated) == true);
        Check(unrelatedTab.ReadyEditor.GetText() == UnrelatedText && !unrelatedTab.IsModified,
            "Restart changed the unrelated disk document opened during import.");
        log.AppendLine("PASS: fresh restart preserves recovered edits, unique file identities and selected tab.");
    }

    private static async Task VerifyFailedImportAsync(NotepadsMainPage page, RestorePreparationGate gate,
        StringBuilder log)
    {
        var core = page.TestCore;
        var ids = ReadIds();
        var files = await ApplicationData.Current.LocalFolder.GetFolderAsync("ImportFiles");
        var target = await files.GetFileAsync(Names[1]);
        await UntilAsync(() => gate.Admitted.Contains(ids[1]), "The failing import did not reach preparation.");
        var owner = Guid.ParseExact(File.ReadAllText(Path.Combine(Root, "workspace-owner.txt")), "N");
        var backups = await ApplicationData.Current.LocalFolder.GetFolderAsync(
            SessionScopeData.GetSecondaryPrefix(owner) + "BackupFiles");
        var pending = await backups.GetFileAsync("pending-1.txt");
        await pending.DeleteAsync();

        var firstOpen = page.OpenFileAsync(target);
        var secondOpen = page.OpenFileAsync(target);
        var lookup = page.TestController.ResolveFileAsync(target);
        Check(!lookup.IsCompleted, "File lookup bypassed the hidden import before its preparation failed.");
        var failure = ObserveFailureAsync(lookup);
        gate.Release(ids[1]);
        await ChooseDialogAsync(firstOpen, "PrimaryButton", target.Path);
        Check(!await firstOpen && !await secondOpen.WaitAsync(TimeSpan.FromSeconds(10)),
            "Concurrent failed-import opens did not finish through the shared error-dialog path.");
        Check(await failure.WaitAsync(TimeSpan.FromSeconds(10)) is { } error && error is not OperationCanceledException,
            "Failed import lookup silently allowed a disk editor instead of reporting the unavailable recovery.");
        Check(!core.GetTabs().Any(tab => tab.ReadyEditor?.EditingFile?.IsEqual(target) == true) &&
            ((NotepadsCore)core).TestOwnedEditorCount == 1,
            "A failed dirty import was replaced by a newly opened disk editor.");
        var unrelated = await files.GetFileAsync(UnrelatedName);
        Check(await page.OpenFileAsync(unrelated).WaitAsync(TimeSpan.FromSeconds(10)),
            "A failed import prevented opening an unrelated disk file.");
        gate.Release(ids[2]);
        await page.TestController.WaitForRestorationAsync().WaitAsync(TimeSpan.FromSeconds(20));
        VerifyFile(core, await files.GetFileAsync(Names[2]), ids[2], RecoveryTexts[2]);
        Check(await FileIO.ReadTextAsync(target) == DiskTexts[1], "Failed import handling changed the disk file.");
        log.AppendLine("PASS: failed opens share one dialog and preserve disk content and unrelated restoration.");
    }

    private static async Task<Exception> ObserveFailureAsync(Task task)
    {
        try { await task; return null; }
        catch (Exception error) { return error; }
    }

    private static void VerifyFile(INotepadsCore core, StorageFile file, Guid id, string text)
    {
        var matches = core.GetTabs().Where(tab => tab.ReadyEditor?.EditingFile?.IsEqual(file) == true).ToArray();
        Check(matches.Length == 1 && matches[0].Id == id && matches[0].ReadyEditor.GetText() == text &&
            matches[0].IsModified, "File identity was duplicated or dirty recovery text was replaced: " + file.Name);
    }

    private static Guid[] ReadIds() => File.ReadAllLines(FixturePath).Select(id => Guid.ParseExact(id, "N")).ToArray();
}
