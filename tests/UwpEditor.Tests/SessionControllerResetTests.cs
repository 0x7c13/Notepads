// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Notepads.Composition;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Storage;
using Notepads.Presentation.Workspace;

namespace NotepadsEditorTests;

internal static partial class SessionControllerTests
{
    private static async Task VerifyInterruptedResetAsync(StringBuilder log)
    {
        var owner = Guid.NewGuid();
        var registry = new SessionRegistry();
        var faults = new ControllerResetDecisionFailure();
        var store = new RecoveryRecordStore(RecoveryRootStore.CreateStore().Paths.RootPath, faults);
        var expectedText = new[] { "first العربية😀\r中文 e\u0301\r", "second Ελληνικά\r日本語🚀\r" };
        Guid[] identities;
        Guid resetEpoch;
        Guid addedId;
        const string addedText = "new native document after reset: Україна😀\r";
        try
        {
            await using (var initial = new ControllerHost(owner, registry))
            {
                await initial.Controller.InitializeAuthorityAsync();
                var first = await initial.Core.OpenAsync(expectedText[0]);
                var second = await initial.Core.OpenAsync(expectedText[1]);
                first.Document.Editor.InsertText(0, "journaled before restart\r");
                expectedText[0] = "journaled before restart\r" + expectedText[0];
                identities = new[] { first.Id, second.Id };
                SessionTestProtocol.Check(await initial.Controller.SaveSessionAsync(),
                    "The controller interrupted-reset fixture did not commit its original workspace.");
            }

            await using (var restored = new ControllerHost(owner, registry, store))
            {
                // Native materialization must retain its original source until a normal native capture commits.
                // Disabling automatic backup makes the source-retirement assertion independent of timer timing.
                restored.Controller.IsBackupEnabled = false;
                await restored.Controller.InitializeAuthorityAsync();
                SessionTestProtocol.Check(await restored.Controller.LoadLastSessionAsync() == identities.Length,
                    "The interrupted-reset controller did not restore its complete original roster.");
                await restored.Controller.WaitForRestorationAsync();
                var tabs = restored.Core.GetTabs();
                var editors = tabs.Select(tab => (FakeTextEditor)tab.ReadyEditor).ToArray();
                var oldEpoch = restored.Service.CurrentStamp.EpochId;
                SessionTestProtocol.Check(tabs.Select(tab => tab.Id).SequenceEqual(identities) &&
                    tabs.All(tab => tab.State == WorkspaceTabState.Ready && tab.RecoverySource?.TargetStamp.EpochId == oldEpoch) &&
                    editors.Select(editor => editor.Text).SequenceEqual(expectedText),
                    "The interrupted-reset fixture lost native Unicode text or acknowledged its recovery sources prematurely.");

                // This edit exists only in the live native editor when reset begins.
                editors[0].Document.Editor.InsertText(0, "unsnapshotted after restore Ω\r");
                expectedText[0] = "unsnapshotted after restore Ω\r" + expectedText[0];
                faults.Enabled = true;
                var failed = false;
                try { await restored.Controller.ClearSessionDataAsync(); }
                catch (IOException) { failed = true; }
                finally { faults.Enabled = false; }

                resetEpoch = restored.Service.CurrentStamp.EpochId;
                SessionTestProtocol.Check(failed && faults.IntentCommitted && faults.FinalRecordRejected && resetEpoch != oldEpoch,
                    "The controller did not observe a failed final reset record after its epoch-changing intent committed.");
                SessionTestProtocol.Check(restored.Core.GetTabs().SequenceEqual(tabs) &&
                    tabs.All(tab => tab.State == WorkspaceTabState.Ready && tab.RecoverySource == null) &&
                    tabs.Select(tab => tab.ReadyEditor).SequenceEqual(editors) &&
                    editors.Select(editor => editor.Text).SequenceEqual(expectedText),
                    "A partially committed reset retained obsolete source associations or replaced live Unicode native content.");
                SessionTestProtocol.Check(!restored.Core.IsEditorPresentationPaused &&
                    ReferenceEquals(await restored.Controller.EnsureLoadedAsync(tabs[0]), editors[0]),
                    "The interrupted reset did not resume usable editor presentation and materialization access.");
                await restored.Controller.WaitForRestorationAsync();

                var added = await restored.Core.OpenAsync(addedText);
                addedId = added.Id;
                restored.Controller.IsBackupEnabled = true;
                var terminalCaptureAccepted = false;
                SessionTestProtocol.Check(await restored.Controller.SaveSessionAsync(() => terminalCaptureAccepted = true) && terminalCaptureAccepted,
                    "A controller native capture could not commit in the reconciled epoch after the reset exception.");
                SessionTestProtocol.Check(editors.Select(editor => editor.Text).SequenceEqual(expectedText) && added.Text == addedText,
                    "The first snapshot after interrupted reset changed the usable native documents.");
            }

            await using (var restarted = new ControllerHost(owner, registry))
            {
                restarted.Controller.IsBackupEnabled = false;
                await restarted.Controller.InitializeAuthorityAsync();
                SessionTestProtocol.Check(await restarted.Controller.LoadLastSessionAsync() == 3,
                    "The snapshot after interrupted reset did not preserve all documents on restart.");
                await restarted.Controller.WaitForRestorationAsync();
                var tabs = restarted.Core.GetTabs();
                SessionTestProtocol.Check(restarted.Service.CurrentStamp.EpochId == resetEpoch &&
                    tabs.Select(tab => tab.Id).SequenceEqual(identities.Append(addedId)) &&
                    tabs.Select(tab => ((FakeTextEditor)tab.ReadyEditor).Text).SequenceEqual(expectedText.Append(addedText)),
                    "Restart after interrupted controller reset lost the new epoch, tab order or exact Unicode bytes.");
            }
        }
        finally { await SessionTestProtocol.CleanupAsync(owner); }
        log.AppendLine("PASS: controller reset retains live Unicode/native identity after a committed intent and failed final record, retires obsolete sources, resumes presentation, and commits/restarts an exact new-epoch workspace.");
    }

    private sealed class ControllerResetDecisionFailure : IRecoveryStorageFaults
    {
        public bool Enabled { get; set; }
        public bool IntentCommitted { get; private set; }
        public bool FinalRecordRejected { get; private set; }

        public void OnStep(RecoveryStorageStep step, RecoveryAddress address, string copy)
        {
            if (!Enabled || address.Area.Kind != RecoveryAreaKind.Decisions) return;
            if (copy == "intent" && step == RecoveryStorageStep.AfterRename) IntentCommitted = true;
            if (IntentCommitted && copy != "intent" && step == RecoveryStorageStep.BeforeRename)
            {
                FinalRecordRejected = true;
                throw new IOException("Injected controller reset failure after its durable intent.");
            }
        }
    }
}
