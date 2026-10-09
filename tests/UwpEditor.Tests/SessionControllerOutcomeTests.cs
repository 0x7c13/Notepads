// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Composition;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Storage;
using Notepads.Presentation.Workspace;

namespace NotepadsEditorTests;

internal static partial class SessionControllerTests
{
    private static async Task VerifyBackgroundOutcomesAsync(StringBuilder log)
    {
        await VerifyBackgroundRestoreOutcomeAsync();
        await VerifyRestorationBackupOutcomeAsync();
        log.AppendLine("PASS: background recovery failure reports one Partial/count change while the selected editor remains usable; failed restoration checkpoints report one backup failure across repeated requests, preserve ready content/sources, and acknowledge sources after an explicit successful save.");
    }

    private static async Task VerifyBackgroundRestoreOutcomeAsync()
    {
        var owner = Guid.NewGuid();
        var registry = new SessionRegistry();
        try
        {
            var (ids, texts) = await CreateOutcomeWorkspaceAsync(owner, registry);
            await using var host = new ControllerHost(owner, registry);
            host.Controller.IsBackupEnabled = false;
            var backgroundStarted = new TaskCompletionSource<object>(TaskCreationOptions.RunContinuationsAsynchronously);
            var failBackground = new TaskCompletionSource<object>(TaskCreationOptions.RunContinuationsAsynchronously);
            var outcomes = new List<(SessionRecoveryOutcome Outcome, int Failed)>();
            host.Controller.RecoveryStateChanged += (_, _) =>
                outcomes.Add((host.Controller.RecoveryOutcome, host.Controller.UnrecoveredEditorCount));
            host.Controller.BeforeRestorePreparation = async (source, cancellation) =>
            {
                if (source.Id != ids[0]) return;
                backgroundStarted.TrySetResult(null);
                await failBackground.Task.WaitAsync(cancellation);
                throw new IOException("Injected background preparation failure after selected readiness.");
            };

            SessionTestProtocol.Check(await host.Controller.LoadLastSessionAsync() == 2,
                "Background-outcome startup did not expose both recovery entries.");
            await backgroundStarted.Task.WaitAsync(TimeSpan.FromSeconds(10));
            var selected = host.Core.GetSelectedTab();
            var selectedEditor = selected?.ReadyEditor as FakeTextEditor;
            var failedTab = host.Core.GetTabs().Single(tab => tab.Id == ids[0]);
            var failedSource = failedTab.RecoverySource;
            SessionTestProtocol.Check(selected?.Id == ids[1] && selectedEditor?.Text == texts[1] &&
                failedTab.State == WorkspaceTabState.Loading &&
                !outcomes.Any(item => item.Outcome == SessionRecoveryOutcome.Partial || item.Failed != 0),
                "The selected document was not usable before its background peer failed.");

            failBackground.SetResult(null);
            await host.Controller.WaitForRestorationAsync().WaitAsync(TimeSpan.FromSeconds(10));
            SessionTestProtocol.Check(failedTab.State == WorkspaceTabState.Failed && failedTab.ReadyEditor == null &&
                ReferenceEquals(failedTab.RecoverySource, failedSource) && host.Controller.UnrecoveredEditorCount == 1 &&
                host.Controller.RecoveryOutcome == SessionRecoveryOutcome.Partial &&
                outcomes.Count(item => item.Outcome == SessionRecoveryOutcome.Partial && item.Failed == 1) == 1,
                "A background restore failure did not retain its source and report one Partial/count change.");
            SessionTestProtocol.Check(ReferenceEquals(host.Core.GetSelectedTab(), selected) &&
                ReferenceEquals(selected.ReadyEditor, selectedEditor) && selectedEditor.Text == texts[1],
                "Background failure changed selection or replaced the already usable native document.");

            selectedEditor.Document.Editor.InsertText(0, "still editable Ω\r");
            SessionTestProtocol.Check(selectedEditor.Text == "still editable Ω\r" + texts[1] &&
                ReferenceEquals(await host.Controller.EnsureLoadedAsync(selected), selectedEditor),
                "The selected native document stopped accepting edits after background recovery failed.");
            await host.Controller.WaitForRestorationAsync();
            SessionTestProtocol.Check(outcomes.Count(item => item.Outcome == SessionRecoveryOutcome.Partial && item.Failed == 1) == 1,
                "An unchanged failed recovery state emitted a duplicate Partial/count notification.");
        }
        finally { await SessionTestProtocol.CleanupAsync(owner); }
    }

    private static async Task VerifyRestorationBackupOutcomeAsync()
    {
        var owner = Guid.NewGuid();
        var registry = new SessionRegistry();
        var faults = new OutcomeCheckpointFailure { Enabled = true };
        try
        {
            var (ids, texts) = await CreateOutcomeWorkspaceAsync(owner, registry);
            var store = new RecoveryRecordStore(RecoveryRootStore.CreateStore().Paths.RootPath, faults);
            await using var host = new ControllerHost(owner, registry, store);
            var backgroundStarted = new TaskCompletionSource<object>(TaskCreationOptions.RunContinuationsAsynchronously);
            var releaseBackground = new TaskCompletionSource<object>(TaskCreationOptions.RunContinuationsAsynchronously);
            var firstFailureReported = new TaskCompletionSource<object>(TaskCreationOptions.RunContinuationsAsynchronously);
            var backupFailures = 0;
            host.Controller.RestorationBackupFailed += (_, _) =>
            {
                backupFailures++;
                firstFailureReported.TrySetResult(null);
            };
            host.Controller.BeforeRestorePreparation = async (source, cancellation) =>
            {
                if (source.Id != ids[0]) return;
                backgroundStarted.TrySetResult(null);
                await releaseBackground.Task.WaitAsync(cancellation);
            };

            SessionTestProtocol.Check(await host.Controller.LoadLastSessionAsync() == 2,
                "Backup-outcome startup did not expose its complete recovery roster.");
            // Automatic startup/final-restoration requests exercise coalescing; periodic timer attempts are unrelated.
            host.Controller.StopSessionBackup();
            await backgroundStarted.Task.WaitAsync(TimeSpan.FromSeconds(10));
            await firstFailureReported.Task.WaitAsync(TimeSpan.FromSeconds(10));
            var selected = host.Core.GetSelectedTab();
            var selectedEditor = (FakeTextEditor)selected.ReadyEditor;
            SessionTestProtocol.Check(selected.Id == ids[1] && selectedEditor.Text == texts[1] &&
                selected.RecoverySource != null && backupFailures == 1 && faults.FailedAttempts >= 1,
                "A failed initial restoration checkpoint revoked ready content, acknowledged its source, or omitted its failure event.");

            selectedEditor.Document.Editor.InsertText(0, "edited during backup failure العربية\r");
            texts[1] = "edited during backup failure العربية\r" + texts[1];
            releaseBackground.SetResult(null);
            await host.Controller.WaitForRestorationCheckpointAsync().WaitAsync(TimeSpan.FromSeconds(15));
            var tabs = host.Core.GetTabs();
            SessionTestProtocol.Check(faults.FailedAttempts >= 2 && backupFailures == 1 &&
                tabs.All(tab => tab.State == WorkspaceTabState.Ready && tab.RecoverySource != null) &&
                tabs.Select(tab => ((FakeTextEditor)tab.ReadyEditor).Text).SequenceEqual(texts) &&
                host.Controller.UnrecoveredEditorCount == 0,
                "Repeated restoration checkpoint failures duplicated notifications, lost ready Unicode edits, or acknowledged uncommitted sources.");

            faults.Enabled = false;
            SessionTestProtocol.Check(await host.Controller.SaveSessionAsync().WaitAsync(TimeSpan.FromSeconds(10)),
                "An explicit snapshot could not recover after the injected checkpoint fault was disabled.");
            SessionTestProtocol.Check(tabs.All(tab => tab.State == WorkspaceTabState.Ready && tab.RecoverySource == null) &&
                tabs.Select(tab => ((FakeTextEditor)tab.ReadyEditor).Text).SequenceEqual(texts) && backupFailures == 1,
                "The successful retry did not acknowledge exact native sources or changed ready content/failure notification count.");
        }
        finally { await SessionTestProtocol.CleanupAsync(owner); }
    }

    private static async Task<(Guid[] Ids, string[] Texts)> CreateOutcomeWorkspaceAsync(Guid owner, SessionRegistry registry)
    {
        var texts = new[] { "background العربية😀\r", "selected 日本語Ω\r" };
        await using var seed = new ControllerHost(owner, registry);
        foreach (var text in texts) await seed.Core.OpenAsync(text);
        var ids = seed.Core.GetTabs().Select(tab => tab.Id).ToArray();
        SessionTestProtocol.Check(await seed.Controller.SaveSessionAsync(), "The background-outcome fixture did not commit.");
        return (ids, texts);
    }

    private sealed class OutcomeCheckpointFailure : IRecoveryStorageFaults
    {
        private int _failedAttempts;
        public bool Enabled { get; set; }
        public int FailedAttempts => Volatile.Read(ref _failedAttempts);

        public void OnStep(RecoveryStorageStep step, RecoveryAddress address, string copy)
        {
            if (Enabled && address.Area.Kind == RecoveryAreaKind.Checkpoints && step == RecoveryStorageStep.BeforeRename)
            {
                Interlocked.Increment(ref _failedAttempts);
                throw new IOException("Injected restoration checkpoint publication failure.");
            }
        }
    }
}
