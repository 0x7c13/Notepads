// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Linq;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Composition;
using Notepads.Controls;
using Notepads.Features.Preferences;
using Notepads.Features.Sessions;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Recovery;
using Notepads.Features.Sessions.Storage;
using Notepads.Infrastructure.Settings;
using Notepads.Presentation.Controls.TextEditor;
using Notepads.Presentation.Views.MainPage;
using Notepads.Presentation.Workspace;
using Windows.Storage;
using Windows.Storage.AccessCache;
using Windows.UI.ViewManagement;
using Windows.UI.Xaml;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsWorkspaceTests;

internal static class WorkspaceLifecycleTests
{
    private const int InitialViewportLine = 80;
    private static readonly string LogText = ".LOG\r" + string.Concat(Enumerable.Range(0, 160)
        .Select(line => $"previous log entry {line}\r"));
    private static string Root => ApplicationData.Current.LocalFolder.Path;
    private static string FixturePath => Path.Combine(Root, "fixture.ids");
    private static readonly string[] Names = ["selected.txt", "discard.txt", "cancel.txt", "save.txt", "missing-grant.txt", "log.txt"];

    internal static Guid ReadInitialSelection() => File.Exists(FixturePath) ? Guid.ParseExact(File.ReadAllLines(FixturePath)[0], "N") : Guid.Empty;

    internal static async Task RunAsync(NotepadsMainPage page, SessionService service, SessionRegistry registry,
        RestorePreparationGate gate, string phase, Guid owner, StringBuilder log)
    {
        await UntilAsync(() => page.TestStartupComplete, "MainPage did not finish selected-tab startup.");
        page.TestController.StopSessionBackup();
        if (phase.EndsWith("-seed", StringComparison.Ordinal)) await SeedAsync(page, owner, log, cleanOnly: phase == "--workspace-clean-seed");
        else if (phase == "--workspace-clean-close") await CloseCleanUnloadedAsync(page, gate, owner, log);
        else if (phase == "--workspace-window-close") await CloseWhileLoadingAsync(page, gate, owner, log);
        else if (phase.EndsWith("-restart", StringComparison.Ordinal)) await VerifyRestartAsync(page, owner, log);
        else await VerifyLifecycleAsync(page, registry, gate, owner, log);
    }

    private static async Task SeedAsync(NotepadsMainPage page, Guid owner, StringBuilder log, bool cleanOnly)
    {
        var core = page.TestCore;
        var original = core.GetTabs().Single();
        var folder = await ApplicationData.Current.LocalFolder.CreateFolderAsync("Files", CreationCollisionOption.FailIfExists);
        var names = cleanOnly ? new[] { Names[0], Names[4] } : Names;
        var tabs = new WorkspaceTab[names.Length];
        for (var index = 0; index < names.Length; index++)
        {
            var file = await folder.CreateFileAsync(names[index], CreationCollisionOption.FailIfExists);
            var text = index == 5 ? LogText : string.Concat(Enumerable.Range(0, index == 0 ? 160 : 4)
                .Select(line => $"{names[index]} line {line}: 你好 😀 Кириллица e\u0301 {new string('x', 100)}\r"));
            await FileIO.WriteTextAsync(file, text);
            var editor = await core.CreateTextEditorAsync(Guid.NewGuid(), file);
            if (!cleanOnly && (index is 0 or 1 or 2 or 3)) editor.TypeText("unsaved 世界😀\r");
            var metadata = editor.GetTextEditorStateMetaData();
            metadata.WrapWord = false;
            metadata.SelectionStartPosition = 23;
            metadata.SelectionEndPosition = 29;
            metadata.ScrollViewerHorizontalOffset = index == 0 ? 30 : 0;
            metadata.FirstVisibleDocumentLine = index == 0 ? InitialViewportLine : index == 5 ? 40 : 0;
            metadata.FirstVisibleSubLine = 0;
            metadata.FontZoomFactor = 1.2;
            editor.ResetEditorState(metadata);
            tabs[index] = new WorkspaceTab(editor);
            // Seed background rows without visual Loaded. Their first visual lifetime occurs during restore.
            core.OpenTab(tabs[index], select: false);
        }
        await page.TestCloseTabAsync(original);
        core.SwitchTo(tabs[0]);
        var selectedCore = await WaitForViewportAsync(tabs[0].ReadyEditor);
        Check(selectedCore.FirstVisibleRow == InitialViewportLine,
            "Seed viewport must actually show the requested nonzero line before saving.");
        core.SetTabScrollViewerHorizontalOffset(17);
        Check(await page.TestController.SaveSessionAsync(), "Could not publish real-editor seed state.");
        File.WriteAllLines(FixturePath, tabs.Select(tab => tab.Id.ToString("N")));
        WriteExpected(tabs, tabs[0].Id);
        var checkpoint = await ReadCommittedAsync(owner);
        var missing = checkpoint.TextEditors.Single(editor => editor.Id == tabs[cleanOnly ? 1 : 4].Id);
        Check(!missing.StateMetaData.IsModified && !string.IsNullOrEmpty(missing.EditingFileFutureAccessToken),
            "The missing-grant regression must start from clean persisted metadata with an actual file grant.");
        StorageApplicationPermissions.FutureAccessList.Remove(missing.EditingFileFutureAccessToken);
        Check(!StorageApplicationPermissions.FutureAccessList.ContainsItem(missing.EditingFileFutureAccessToken), "Could not revoke the owned fixture grant.");
        var selected = checkpoint.TextEditors.Single(editor => editor.Id == tabs[0].Id).StateMetaData;
        Check(selected.FirstVisibleDocumentLine == InitialViewportLine && selected.FirstVisibleSubLine == 0 &&
            selected.ScrollViewerVerticalOffset > 0,
            "The committed seed must retain the nonzero document anchor and actual vertical offset.");
        File.WriteAllText(Path.Combine(Root, "selected-viewport.txt"),
            string.Join("\n", selected.SelectionStartPosition, selected.SelectionEndPosition,
                selected.ScrollViewerHorizontalOffset.ToString("R", System.Globalization.CultureInfo.InvariantCulture),
                selected.ScrollViewerVerticalOffset.ToString("R", System.Globalization.CultureInfo.InvariantCulture),
                selected.FirstVisibleDocumentLine, selected.FirstVisibleSubLine));
        log.AppendLine($"PASS: seed viewport shows and persists document line {InitialViewportLine} with its saved selection.");
        log.AppendLine($"PASS: {tabs.Length} actual editors seed ordered Unicode snapshots; one clean file grant is revoked within the private scope.");
    }

    private static async Task CloseCleanUnloadedAsync(NotepadsMainPage page, RestorePreparationGate gate, Guid owner, StringBuilder log)
    {
        var tabs = page.TestCore.GetTabs();
        await UntilAsync(() => gate.Admitted.Count >= 2, "The clean missing-grant row never reached its preparation gate.");
        Check(tabs.Length == 2 && tabs.All(tab => !tab.IsModified) && tabs[0].ReadyEditor != null && tabs[1].ReadyEditor == null,
            "The clean-only close regression requires no dirty flags and one unloaded document.");
        var source = tabs[1].RecoverySource;
        // Emulates the exact preference-read path used when another window disables snapshots.
        // No preference-change event/reset materializes the pending fixture before this close request.
        ApplicationSettingsStore.Write(SettingsKey.EditorEnableSessionBackupAndRestoreBool, false);
        var close = page.TestCloseWindowAsync();
        await ChooseDialogAsync(close, "CloseButton");
        Check(!await close && page.IsEnabled && page.TestCore.ContainsTab(tabs[1]) && ReferenceEquals(source, tabs[1].RecoverySource),
            "Snapshot-off window close silently discarded clean pending metadata or failed to resume after Cancel.");
        Check((await ReadCommittedAsync(owner)).TextEditors.Count == 2, "Canceled snapshot-off close published a missing-grant Closed decision.");
        Check(tabs[1].ReadyEditor == null && ((NotepadsCore)page.TestCore).TestOwnedEditorCount == 1,
            "Cancel unexpectedly materialized the blocked clean row.");
        ApplicationSettingsStore.Write(SettingsKey.EditorEnableSessionBackupAndRestoreBool, true);
        Check(await page.TestCloseWindowAsync().WaitAsync(TimeSpan.FromSeconds(10)), "Snapshot-backed close could not retain the clean pending row.");
        log.AppendLine("PASS: actual snapshot-off window close prompts with every stored dirty flag false; Cancel preserves clean missing-grant content and re-enables the page before snapshot-backed terminal close.");
    }

    private static async Task VerifyLifecycleAsync(NotepadsMainPage page, SessionRegistry registry,
        RestorePreparationGate gate, Guid owner, StringBuilder log)
    {
        var core = page.TestCore;
        var ids = File.ReadAllLines(FixturePath).Select(id => Guid.ParseExact(id, "N")).ToArray();
        var tabs = core.GetTabs();
        Check(tabs.Select(tab => tab.Id).SequenceEqual(ids), "MainPage changed the saved roster or inserted an Untitled tab.");
        Check(ReferenceEquals(core.GetSelectedTab(), tabs[0]) && tabs[0].ReadyEditor != null,
            "The saved selected tab did not become usable first.");
        await UntilAsync(() => gate.Admitted.Count >= 2, "The first background document was never admitted.");
        Check(gate.Admitted[0] == ids[0] && gate.Admitted[1] == ids[1], "Restoration did not load the selected document before background work.");
        Check(tabs.Skip(1).All(tab => tab.ReadyEditor == null) && ((NotepadsCore)core).TestOwnedEditorCount == 1,
            "A pending/loading tab constructed a native editor before its preparation gate was released.");
        Check(tabs.Skip(1).All(tab => page.TestContainer(tab).Header?.ToString() == tab.Title), "Pending tab titles were not restored.");
        Check(page.TestContainer(tabs[1]).Icon.Visibility == Visibility.Visible &&
            page.TestContainer(tabs[4]).Icon.Visibility == Visibility.Collapsed, "Pending modified indicators do not reflect retained metadata.");
        var viewport = File.ReadAllLines(Path.Combine(Root, "selected-viewport.txt"));
        var selectedCore = await WaitForViewportAsync(tabs[0].ReadyEditor);
        var current = tabs[0].ReadyEditor.GetTextEditorStateMetaData();
        Check(current.SelectionStartPosition == int.Parse(viewport[0]) && current.SelectionEndPosition == int.Parse(viewport[1]),
            "The actual selected editor lost its saved caret/selection during first visual Loaded.");
        var expectedY = double.Parse(viewport[3], System.Globalization.CultureInfo.InvariantCulture);
        var expectedX = double.Parse(viewport[2], System.Globalization.CultureInfo.InvariantCulture);
        var expectedLine = int.Parse(viewport[4]);
        var expectedSubLine = int.Parse(viewport[5]);
        Check(expectedLine == InitialViewportLine && expectedSubLine == 0 && expectedY > 0,
            "The cross-process viewport fixture must describe a nonzero document anchor.");
        Check(selectedCore.FirstVisibleRow == expectedLine && current.FirstVisibleDocumentLine == expectedLine &&
            current.FirstVisibleSubLine == expectedSubLine,
            "A fresh process must restore the actual native viewport, not only retain pending metadata.");
        Check(Math.Abs(current.ScrollViewerVerticalOffset - expectedY) <= selectedCore.GetSingleLineHeight(),
            "The first visual Loaded discarded the saved viewport.");
        Check(Math.Abs(current.ScrollViewerHorizontalOffset - expectedX) <= 2,
            "The first visual Loaded discarded the saved horizontal viewport.");
        Check(page.TestStatusModified && ApplicationView.GetForCurrentView().Title.Contains(Names[0], StringComparison.Ordinal),
            "Selected status/title did not reflect the restored editor.");
        log.AppendLine($"PASS: a fresh MainPage process restores document line {expectedLine}, horizontal offset and selection.");
        log.AppendLine("PASS: selected-first production MainPage paints every header with exactly one native editor, correct title/modified state and restored caret/viewport.");

        var duplicateFile = await ApplicationData.Current.LocalFolder.CreateFileAsync("concurrent-open.txt", CreationCollisionOption.FailIfExists);
        await FileIO.WriteTextAsync(duplicateFile, "concurrent open Ω😀");
        await Task.WhenAll(page.OpenFileAsync(duplicateFile), page.OpenFileAsync(duplicateFile)).WaitAsync(TimeSpan.FromSeconds(15));
        var opened = core.GetTabs().Where(tab => tab.ReadyEditor?.EditingFile?.IsEqual(duplicateFile) == true).ToArray();
        Check(opened.Length == 1, "Concurrent opens across pending file resolution created duplicate editors.");
        await page.TestCloseTabAsync(opened[0]);
        core.SwitchTo(tabs[0]);
        Check(core.GetTabs().SequenceEqual(tabs), "Concurrent-open cleanup changed the restored roster.");
        log.AppendLine("PASS: concurrent production OpenFileAsync calls retain a single document while unrelated pending file grants require asynchronous lookup.");

        var retainedSource = tabs[1].RecoverySource;
        tabs[0].ReadyEditor.TypeText("suspension edit Ω😀\r");
        var edited = tabs[0].ReadyEditor.GetText();
        using (var cancellation = new CancellationTokenSource(TimeSpan.FromSeconds(8)))
            await registry.SaveBeforeSuspensionAsync(cancellation.Token).WaitAsync(TimeSpan.FromSeconds(10));
        var suspended = await ReadCommittedAsync(owner);
        Check(suspended.TextEditors.Select(editor => editor.Id).SequenceEqual(ids), "Suspension capture omitted blocked pending documents.");
        Check(ReferenceEquals(retainedSource, tabs[1].RecoverySource) && tabs[1].ReadyEditor == null,
            "Suspension capture replaced the pending source identity or waited for payload materialization.");
        Check(tabs[0].ReadyEditor.GetText() == edited, "The suspension acknowledgement changed native text.");
        log.AppendLine("PASS: real SessionRegistry suspension capture completes with background preparation blocked and retains the exact pending source and roster.");

        core.SwitchTo(tabs[2]);
        var stableHost = page.TestContentHost(tabs[2]);
        page.TestRefreshMenu();
        Check(!page.TestSaveAsEnabled && !page.TestFindEnabled, "Pending content commands remained enabled.");
        Check(page.TestStatusPath == tabs[2].EditingFilePath && ApplicationView.GetForCurrentView().Title.Contains(Names[2], StringComparison.Ordinal),
            "Selecting a pending tab left another document's path or title visible.");
        Check(!await page.TestSaveSelectedAsAsync().WaitAsync(TimeSpan.FromSeconds(2)), "Direct Save As entered the picker for a pending selection.");
        var cancelClose = page.TestCloseTabAsync(tabs[2]);
        await ChooseDialogAsync(cancelClose, "CloseButton", Names[2]);
        Check(core.ContainsTab(tabs[2]) && tabs[2].ReadyEditor == null && page.IsEnabled,
            "Cancel did not preserve and re-enable an unloaded dirty tab.");
        var cleanSource = tabs[4].RecoverySource;
        Check(!tabs[4].IsModified, "The missing-grant test no longer has clean pending metadata.");
        await ChooseDialogAsync(page.TestCloseTabAsync(tabs[4]), "CloseButton", Names[4]);
        Check(core.ContainsTab(tabs[4]) && ReferenceEquals(cleanSource, tabs[4].RecoverySource),
            "Clean metadata silently discarded the missing-grant recovery source.");
        var afterCancel = await ReadCommittedAsync(owner);
        Check(afterCancel.TextEditors.Any(editor => editor.Id == ids[4]), "Cancel published Closed for the clean missing-grant tab.");
        log.AppendLine("PASS: actual pending Save As menu/direct guard and real unloaded dirty/clean-missing-grant Cancel dialogs preserve recovery.");

        await ChooseDialogAsync(page.TestCloseTabAsync(tabs[1]), "SecondaryButton", Names[1]);
        Check(!core.ContainsTab(tabs[1]) && tabs[1].State == WorkspaceTabState.Closed && tabs[1].ReadyEditor == null,
            "Discard did not durably close the exact loading entry without an editor.");
        // Selection made by the confirmation dialog must not override an explicit subsequent selection.
        core.SwitchTo(tabs[2]);
        gate.Release(ids[4]);
        gate.Release(ids[2]);
        var selectedLoad = page.TestController.EnsureLoadedAsync(tabs[2]);
        var sameLoad = page.TestController.EnsureLoadedAsync(tabs[2]);
        var selectedEditor = await selectedLoad.WaitAsync(TimeSpan.FromSeconds(15));
        Check(ReferenceEquals(selectedEditor, await sameLoad) && ReferenceEquals(core.GetSelectedTab(), tabs[2]),
            "Selected pending restoration duplicated the editor or changed selection.");
        await UntilAsync(() => ((FrameworkElement)selectedEditor).ActualWidth > 0, "The selected pending editor never attached visibly.");
        Check(ReferenceEquals(stableHost, page.TestContentHost(tabs[2])) && ReferenceEquals(stableHost.Content, selectedEditor),
            "Materialization replaced the outer Sets content or left the selected host stale.");
        page.TestRefreshMenu();
        Check(page.TestSaveAsEnabled && page.TestFindEnabled && page.TestStatusPath.Contains(Names[2], StringComparison.Ordinal) &&
            !string.IsNullOrEmpty(page.TestStatusPosition), "Pending-to-ready attachment did not refresh actual status/menu controls.");
        log.AppendLine("PASS: loading Discard creates no editor; repeated selected demands share one editor and update the stable visible host/menu/status without reselection.");

        // Save goes through RequestTabCloseAsync -> demand load -> real SaveAsync -> durable Closed.
        var savedText = File.ReadAllText(Path.Combine(Root, "expected", ids[3].ToString("N") + ".txt"));
        Check(tabs[3].ReadyEditor == null, "The save-before-close fixture materialized before its close command.");
        await ChooseDialogAsync(page.TestCloseTabAsync(tabs[3]), "PrimaryButton", Names[3], () => gate.Release(ids[3]));
        Check(!core.ContainsTab(tabs[3]), "Save-before-close did not remove the loaded tab.");
        var savedFile = await (await ApplicationData.Current.LocalFolder.GetFolderAsync("Files")).GetFileAsync(Names[3]);
        var savedBytes = File.ReadAllBytes(savedFile.Path);
        var written = Encoding.UTF8.GetString(savedBytes).TrimStart('\uFEFF').Replace("\r\n", "\r").Replace("\n", "\r");
        Check(written == savedText, "Save-before-close lost Unicode text or saved a blank pending editor.");
        log.AppendLine("PASS: the actual unloaded Save dialog demand-loads, writes the complete Unicode document, and then closes durably.");

        // Closing the selected Save tab legitimately selects a neighbor. Establish
        // the user's choice before verifying that background completion preserves it.
        core.SwitchTo(tabs[2]);
        gate.HoldPromotion(ids[5]);
        gate.ReleaseAll();
        await gate.PromotionReached.WaitAsync(TimeSpan.FromSeconds(20));
        Check(ReferenceEquals(core.GetSelectedTab(), tabs[2]), "Background completion stole tab selection.");
        var logBefore = gate.PromotionCandidate.GetText();
        Check(logBefore == LogText, "Off-tree background restoration fired .LOG before its first visual Loaded.");
        var attachedWhileDisabled = false;
        void OnAttached(object sender, WorkspaceTab tab)
        {
            if (ReferenceEquals(tab, tabs[5]) && !page.IsEnabled) attachedWhileDisabled = true;
        }
        core.EditorAttached += OnAttached;
        core.SwitchTo(tabs[5]);
        ApplicationSettingsStore.Write(SettingsKey.EditorEnableSessionBackupAndRestoreBool, false);
        var canceledWindowClose = page.TestCloseWindowAsync();
        await ChooseDialogAsync(canceledWindowClose, "CloseButton", whileDialogOpen: async () =>
        {
            Check(!page.IsEnabled && tabs[5].ReadyEditor == null, "Window close did not pause the prepared .LOG candidate.");
            gate.ReleasePromotion();
            await UntilAsync(() => gate.PromotionGateReturned, "The valid native .LOG candidate did not leave its test gate.");
            await Task.Yield();
            Check(tabs[5].ReadyEditor == null && gate.PromotionCandidate.GetText() == logBefore,
                "A completed native candidate was promoted or fired .LOG while the close dialog disabled the page.");
        });
        Check(!await canceledWindowClose && page.IsEnabled && !attachedWhileDisabled,
            "Canceled close did not enable the page before resuming native editor presentation.");
        await page.TestController.WaitForRestorationAsync().WaitAsync(TimeSpan.FromSeconds(20));
        core.EditorAttached -= OnAttached;
        Check(!attachedWhileDisabled && core.GetTabs().All(tab => tab.ReadyEditor != null),
            "Canceled-close restoration promoted while disabled or failed to finish.");
        Check(tabs[4].ReadyEditor.EditingFile == null && tabs[4].ReadyEditor.IsModified,
            "A revoked clean-file grant did not restore recoverable content requiring Save As.");
        await UntilAsync(() => tabs[5].ReadyEditor.GetText().Length > logBefore.Length, ".LOG did not append on the actual first visual Loaded.");
        var logCore = await WaitForViewportAsync(tabs[5].ReadyEditor);
        var logNative = logCore.TestFeatureNative;
        var caretY = logNative.PointYFromPosition(logNative.CurrentPos);
        Check(logNative.CurrentPos == logNative.Length && logNative.Anchor == logNative.Length &&
            caretY >= 0 && caretY < logNative.LinesOnScreen * logNative.TextHeight(0),
            "Long .LOG must show its appended EOF caret after the actual first visual Loaded.");
        var logAfter = tabs[5].ReadyEditor.GetText();
        core.SwitchTo(tabs[0]);
        await UntilAsync(() => !logCore.IsLoaded, ".LOG editor did not unload on tab switch.");
        core.SwitchTo(tabs[5]);
        await UntilAsync(() => logCore.IsLoaded, ".LOG editor did not reload on tab reselection.");
        Check(tabs[5].ReadyEditor.GetText() == logAfter, ".LOG appended twice after tab reselection.");
        core.SwitchTo(tabs[0]);
        ApplicationSettingsStore.Write(SettingsKey.EditorEnableSessionBackupAndRestoreBool, true);
        ApplicationPreferences.RefreshSessionSnapshotSetting();
        page.TestController.IsBackupEnabled = true;
        // Arrange the UWP reorder boundary; the next process independently verifies
        // persistence of this explicit order, selection and each document's bytes.
        var sets = (SetsView)page.FindName("Sets");
        var reordered = page.TestContainer(tabs[5]);
        sets.Items.Remove(reordered);
        sets.Items.Insert(0, reordered);
        var expectedOrder = new[] { tabs[5], tabs[0], tabs[2], tabs[4] };
        Check(core.GetTabs().SequenceEqual(expectedOrder) && ReferenceEquals(core.GetSelectedTab(), tabs[0]),
            "Collection-boundary reorder preserves the selected document before its durable checkpoint.");
        Check(await page.TestController.SaveSessionAsync(), "Final UI lifecycle checkpoint failed.");
        WriteExpected(expectedOrder, tabs[0].Id);
        Check((await ReadCommittedAsync(owner)).TextEditors.Select(editor => editor.Id).SequenceEqual(expectedOrder.Select(tab => tab.Id)),
            "The final checkpoint lost the reordered roster or retained a closed document.");
        log.AppendLine("PASS: actual window-close Cancel parks a completed native .LOG candidate until the page is enabled; first visual Loaded runs once and reselection does not append again.");
    }

    private static async Task CloseWhileLoadingAsync(NotepadsMainPage page, RestorePreparationGate gate, Guid owner, StringBuilder log)
    {
        var tabs = page.TestCore.GetTabs();
        var selected = page.TestCore.GetSelectedTab();
        await UntilAsync(() => gate.Admitted.Count >= 2, "Window-close phase did not reach blocked background preparation.");
        Check(tabs.Count(tab => tab.ReadyEditor != null) == 1, "Window-close fixture needs only the selected editor materialized.");
        Check(await page.TestCloseWindowAsync().WaitAsync(TimeSpan.FromSeconds(10)), "Actual window close waited for or rejected a blocked background read.");
        var committed = await ReadCommittedAsync(owner);
        Check(committed.TextEditors.Select(editor => editor.Id).SequenceEqual(tabs.Select(tab => tab.Id)),
            "Terminal close omitted or reordered pending documents.");
        Check(tabs.Where(tab => !ReferenceEquals(tab, selected)).All(tab => tab.ReadyEditor == null),
            "Terminal close promoted a background editor after its final capture.");
        log.AppendLine("PASS: real MainPage window close checkpoints the ready editor and retained pending roster without waiting for background preparation or visual promotion.");
    }

    private static async Task VerifyRestartAsync(NotepadsMainPage page, Guid owner, StringBuilder log)
    {
        await page.TestController.WaitForRestorationAsync().WaitAsync(TimeSpan.FromSeconds(25));
        var expectedIds = File.ReadAllLines(Path.Combine(Root, "expected-order.txt")).Select(Guid.Parse).ToArray();
        var selected = Guid.Parse(File.ReadAllText(Path.Combine(Root, "expected-selected.txt")));
        var tabs = page.TestCore.GetTabs();
        Check(tabs.Select(tab => tab.Id).SequenceEqual(expectedIds), "A fresh process lost order or resurrected a discarded/saved-closed tab.");
        Check(page.TestCore.GetSelectedTab().Id == selected, "A fresh process lost saved selection.");
        foreach (var tab in tabs)
        {
            Check(tab.ReadyEditor != null, "A retained document did not materialize after a fresh launch.");
            var expected = File.ReadAllBytes(Path.Combine(Root, "expected", tab.Id.ToString("N") + ".txt"));
            var actual = Encoding.UTF8.GetBytes(tab.ReadyEditor.GetText());
            Check(CryptographicOperations.FixedTimeEquals(SHA256.HashData(actual), SHA256.HashData(expected)),
                "Fresh-process exact Unicode bytes changed for " + tab.Title);
            log.AppendLine("PASS: fresh-process SHA-256 " + tab.Id.ToString("N") + " " + Convert.ToHexString(SHA256.HashData(actual)));
        }
        Check(await page.TestCloseWindowAsync().WaitAsync(TimeSpan.FromSeconds(10)), "Fresh-process window close failed.");
        Check((await ReadCommittedAsync(owner)).TextEditors.Select(editor => editor.Id).SequenceEqual(expectedIds),
            "The actual final close changed the verified roster.");
        log.AppendLine("PASS: fresh process restores exact Unicode hashes/order/selection with no discarded-tab resurrection, then the actual MainPage close succeeds.");
    }

    private static void WriteExpected(WorkspaceTab[] tabs, Guid selected)
    {
        var folder = Path.Combine(Root, "expected");
        Directory.CreateDirectory(folder);
        foreach (var tab in tabs) File.WriteAllText(Path.Combine(folder, tab.Id.ToString("N") + ".txt"), tab.ReadyEditor.GetText(), new UTF8Encoding(false, true));
        File.WriteAllLines(Path.Combine(Root, "expected-order.txt"), tabs.Select(tab => tab.Id.ToString("N")));
        File.WriteAllText(Path.Combine(Root, "expected-selected.txt"), selected.ToString("N"));
    }

    private static async Task<TextEditorCore> WaitForViewportAsync(ITextEditor editor)
    {
        var core = ((TextEditor)editor).TestNativeCore;
        await UntilAsync(() => core.IsLoaded && core.ActualWidth > 0, "Selected editor did not become visible.");
        var view = Descendants(core).OfType<WinUIEditor.EditorBaseControl>().Single();
        await UntilAsync(() => !view.IsViewportRestorePending, "Native viewport restoration did not complete.");
        return core;
    }

    private static Task<NotepadsSessionDataV2> ReadCommittedAsync(Guid owner) => Task.Run(async () =>
    {
        using (await SessionRecoveryTransaction.EnterAsync(CancellationToken.None))
        {
            var scope = await SessionRecoveryCatalog.ReadScopeAsync(owner);
            return scope.SelectedCheckpoint?.Record.Session ?? throw new InvalidOperationException("No committed checkpoint exists.");
        }
    });
}
