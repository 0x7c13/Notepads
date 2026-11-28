// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Notepads.Composition;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Contracts.Legacy;
using Notepads.Features.Sessions.Recovery;
using Notepads.Features.Sessions.Storage;
using Notepads.Presentation.Workspace;
using Windows.Storage;

namespace NotepadsEditorTests;

internal static partial class SessionControllerTests
{
    private static async Task VerifyImportedWorkspaceAsync(StringBuilder log)
    {
        var owner = Guid.NewGuid();
        var registry = new SessionRegistry();
        var ids = new[] { Guid.NewGuid(), Guid.NewGuid(), Guid.NewGuid() };
        var texts = new[] { "first العربية😀", "selected 日本語Ω", "last Україна𐐷" };
        const double scrollOffset = 23;
        try
        {
            var folder = await ApplicationData.Current.LocalFolder.CreateFolderAsync(
                SessionScopeData.GetSecondaryPrefix(owner) + "BackupFiles", CreationCollisionOption.OpenIfExists);
            var legacy = new NotepadsSessionDataV1
            {
                SelectedTextEditor = ids[1],
                TabScrollViewerHorizontalOffset = scrollOffset
            };
            for (var index = 0; index < ids.Length; index++)
            {
                var backup = await folder.CreateFileAsync($"import-{index}.txt", CreationCollisionOption.FailIfExists);
                await FileIO.WriteTextAsync(backup, texts[index]);
                legacy.TextEditors.Add(new TextEditorSessionDataV1
                {
                    Id = ids[index],
                    LastSavedBackupFilePath = backup.Path,
                    StateMetaData = new DocumentMetadata
                    {
                        LastSavedEncoding = "UTF-8",
                        LastSavedLineEnding = "CRLF",
                        FontZoomFactor = 1,
                        RequiresSaveAs = true,
                        FileNamePlaceholder = $"import-{index}"
                    }
                });
            }
            await SessionTestProtocol.WriteLegacySourceAsync(owner, legacy);

            var publications = new ImportCheckpointObserver();
            var store = new RecoveryRecordStore(RecoveryRootStore.CreateStore().Paths.RootPath, publications);
            await using (var host = new ControllerHost(owner, registry, store))
            {
                var backgroundStarted = new TaskCompletionSource<object>(TaskCreationOptions.RunContinuationsAsynchronously);
                var releaseBackground = new TaskCompletionSource<object>(TaskCreationOptions.RunContinuationsAsynchronously);
                var starts = new List<Guid>();
                host.Controller.BeforeRestorePreparation = async (source, cancellation) =>
                {
                    starts.Add(source.Id);
                    if (source.Id != ids[0]) return;
                    backgroundStarted.TrySetResult(null);
                    await releaseBackground.Task.WaitAsync(cancellation);
                };

                SessionTestProtocol.Check(await host.Controller.LoadLastSessionAsync() == 1,
                    "Legacy startup did not return after making its selected import usable.");
                // Only startup/completion checkpoint requests may satisfy this regression.
                host.Controller.StopSessionBackup();
                await backgroundStarted.Task.WaitAsync(TimeSpan.FromSeconds(10));
                var selected = host.Core.GetSelectedTab();
                var selectedEditor = selected?.ReadyEditor as FakeTextEditor;
                SessionTestProtocol.Check(starts.SequenceEqual(new[] { ids[1], ids[0] }) &&
                    host.Core.GetTabs().Length == 1 && selected?.Id == ids[1] &&
                    selectedEditor?.Text == texts[1] && selected.State == WorkspaceTabState.Ready,
                    "The selected legacy row was not the first and only visible imported native document while background preparation was blocked.");

                await publications.FirstCheckpoint.Task.WaitAsync(TimeSpan.FromSeconds(10));
                await VerifyImportedCheckpointAsync(owner, new[] { ids[1] }, ids[1], scrollOffset, 1);

                // An edit made after durable adoption must reach the ordinary checkpoint.
                selectedEditor.Document.Editor.InsertText(0, "edited after adoption Ω\r");
                texts[1] = "edited after adoption Ω\r" + texts[1];
                releaseBackground.SetResult(null);
                await host.Controller.WaitForRestorationCheckpointAsync().WaitAsync(TimeSpan.FromSeconds(20));
                var tabs = host.Core.GetTabs();
                SessionTestProtocol.Check(starts.SequenceEqual(new[] { ids[1], ids[0], ids[2] }) &&
                    tabs.Select(tab => tab.Id).SequenceEqual(ids) &&
                    tabs.All(tab => tab.State == WorkspaceTabState.Ready && tab.RecoverySource == null) &&
                    tabs.Select(tab => ((FakeTextEditor)tab.ReadyEditor).Text).SequenceEqual(texts) &&
                    ReferenceEquals(host.Core.GetSelectedTab(), selected) &&
                    ReferenceEquals(selected.ReadyEditor, selectedEditor),
                    "Automatic import completion lost source order, native edits, selection, or exact checkpoint acknowledgement.");
                await VerifyImportedCheckpointAsync(owner, ids, ids[1], scrollOffset, ids.Length);
            }

            await using (var restart = new ControllerHost(owner, registry))
            {
                restart.Controller.IsBackupEnabled = false;
                SessionTestProtocol.Check(await restart.Controller.LoadLastSessionAsync() == ids.Length,
                    "The automatic import checkpoint did not restore the complete workspace on restart.");
                await restart.Controller.WaitForRestorationAsync().WaitAsync(TimeSpan.FromSeconds(20));
                var tabs = restart.Core.GetTabs();
                SessionTestProtocol.Check(tabs.Select(tab => tab.Id).SequenceEqual(ids) &&
                    tabs.All(tab => tab.State == WorkspaceTabState.Ready && tab.RecoverySource?.IsOwned == true) &&
                    tabs.Select(tab => ((FakeTextEditor)tab.ReadyEditor).Text).SequenceEqual(texts) &&
                    restart.Core.GetSelectedTab()?.Id == ids[1] && restart.Core.TabScrollOffset == scrollOffset,
                    "Restart lost imported order, selected identity, Unicode edits or scroll offset, or reimported a consumed legacy row.");
                await VerifyImportedCheckpointAsync(owner, ids, ids[1], scrollOffset, ids.Length);
            }
        }
        finally { await SessionTestProtocol.CleanupAsync(owner); }
        log.AppendLine("PASS: legacy imports restore selected first and preserve order, edits and single adoption.");
    }

    private static Task VerifyImportedCheckpointAsync(Guid owner, Guid[] ids, Guid selected,
        double scrollOffset, int adoptionCount) => Task.Run(async () =>
    {
        using (await SessionRecoveryTransaction.EnterAsync())
        {
            var catalog = await SessionRecoveryCatalog.ReadScopeAsync(owner);
            var checkpoint = catalog.SelectedCheckpoint?.Record.Session;
            var adoptions = catalog.Decisions.Where(read => read.Record.Kind == RecoveryRecordKind.AdoptLegacy).ToArray();
            SessionTestProtocol.Check(!catalog.Blocked && checkpoint != null &&
                checkpoint.TextEditors.Select(row => row.Id).SequenceEqual(ids) &&
                checkpoint.SelectedTextEditor == selected && checkpoint.TabScrollViewerHorizontalOffset == scrollOffset,
                "Imports had durable adoption content but no exact automatic checkpoint of visible order and selection.");
            SessionTestProtocol.Check(adoptions.Length == adoptionCount &&
                adoptions.Select(read => read.Record.Editor.Id).Distinct().Count() == adoptionCount &&
                adoptions.All(read => ids.Contains(read.Record.Editor.Id) &&
                    read.Record.Editor.SavedBaseline.OwnerId == owner &&
                    read.Record.Editor.RecoveryBaseline.OwnerId == owner && read.Record.Editor.Journal.OwnerId == owner &&
                    read.Record.Consumptions.Count == 1 && read.Record.Consumptions[0].Kind == RecoveryConsumptionKind.Legacy &&
                    read.Record.Consumptions[0].SourceEditorId == read.Record.Editor.Id),
                "An imported native checkpoint duplicated adoption or failed to preserve independently owned content and exact legacy consumption.");
        }
    });

    private sealed class ImportCheckpointObserver : IRecoveryStorageFaults
    {
        public TaskCompletionSource<object> FirstCheckpoint { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);

        public void OnStep(RecoveryStorageStep step, RecoveryAddress address, string copy)
        {
            if (address.Area.Kind == RecoveryAreaKind.Checkpoints && step == RecoveryStorageStep.AfterRename)
                FirstCheckpoint.TrySetResult(null);
        }
    }
}
