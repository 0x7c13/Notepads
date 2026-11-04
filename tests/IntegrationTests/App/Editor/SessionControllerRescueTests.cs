// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
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
using Notepads.Features.Sessions.Contracts.Legacy;
using Notepads.Features.Sessions.Recovery;
using Notepads.Features.Sessions.Storage;
using Notepads.Presentation.Workspace;
using Windows.Storage;

namespace NotepadsEditorTests;

internal static partial class SessionControllerTests
{
    private static async Task VerifyPendingRescuePersistenceAsync(StringBuilder log)
    {
        await VerifyPartiallyAdoptedRescueRestartAsync();
        await VerifyClosingRescueDiscoveryRetryAsync();
        await VerifyFailedRescueRestartAsync();
        log.AppendLine("PASS: hidden rescues survive partial restoration, recover once and honor durable close.");
    }

    private static async Task VerifyPartiallyAdoptedRescueRestartAsync()
    {
        var owner = Guid.NewGuid();
        var registry = new SessionRegistry();
        try
        {
            var (originalIds, texts) = await CreateRescueWorkspaceAsync(owner, registry);
            var blocked = new TaskCompletionSource<object>(TaskCreationOptions.RunContinuationsAsynchronously);
            var canceled = new TaskCompletionSource<object>(TaskCreationOptions.RunContinuationsAsynchronously);
            Guid selectedId;
            await using (var host = new ControllerHost(owner, registry))
            {
                // Publish exactly one explicit snapshot while the second import is still hidden.
                host.Controller.IsBackupEnabled = false;
                var started = new List<Guid>();
                host.Controller.BeforeRestorePreparation = async (source, cancellation) =>
                {
                    started.Add(source.Id);
                    if (started.Count != 2) return;
                    blocked.TrySetResult(null);
                    try { await Task.Delay(Timeout.Infinite, cancellation); }
                    finally { canceled.TrySetResult(null); }
                };

                SessionTestProtocol.Check(await host.Controller.LoadLastSessionAsync().WaitAsync(TimeSpan.FromSeconds(15)) == 1,
                    "Rescue startup did not return after its first durable adoption.");
                await blocked.Task.WaitAsync(TimeSpan.FromSeconds(10));
                var selected = host.Core.GetSelectedTab();
                var editor = selected?.ReadyEditor as FakeTextEditor;
                SessionTestProtocol.Check(host.Core.GetTabCount() == 1 && editor != null && texts.Contains(editor.Text) &&
                    editor.Document.Metadata.RequiresSaveAs && started.Count == 2 &&
                    started.Distinct().Count() == 2 && started.All(id => !originalIds.Contains(id)),
                    "Rescue startup exposed an unadopted row, reused an original identity, or lost the first SaveAs document.");
                selectedId = selected.Id;
                host.Controller.IsBackupEnabled = true;
                SessionTestProtocol.Check(await host.Controller.SaveSessionAsync().WaitAsync(TimeSpan.FromSeconds(15)) &&
                    host.Core.GetTabCount() == 1 && ReferenceEquals(host.Core.GetSelectedTab(), selected) &&
                    !canceled.Task.IsCompleted,
                    "The partial rescue snapshot waited for hidden payload loading or changed visible membership/selection.");
            }
            SessionTestProtocol.Check(canceled.Task.IsCompleted,
                "Disposing the first rescue host did not cancel and drain its blocked preparation.");

            await VerifyRescueRestartAndCloseAsync(owner, registry, originalIds, texts, selectedId);
        }
        finally { await SessionTestProtocol.CleanupAsync(owner); }
    }

    private static async Task VerifyClosingRescueDiscoveryRetryAsync()
    {
        var owner = Guid.NewGuid();
        var registry = new SessionRegistry();
        const string addedText = "terminal save after rescue discovery failed Україна😀\r";
        try
        {
            var (originalIds, texts) = await CreateRescueWorkspaceAsync(owner, registry);
            await SessionTestProtocol.WriteLegacySourceAsync(owner, new NotepadsSessionDataV1());
            var manifest = Path.Combine(ApplicationData.Current.LocalFolder.Path, SessionScopeData.GetManifestFileName(owner));
            Guid addedId;
            await using (var host = new ControllerHost(owner, registry))
            {
                var preparations = 0;
                host.Controller.BeforeRestorePreparation = (_, _) =>
                {
                    preparations++;
                    throw new InvalidOperationException("Terminal rescue discovery must not load payloads.");
                };
                using (new FileStream(manifest, FileMode.Open, FileAccess.Read, FileShare.None))
                {
                    await ExpectDiscoveryFailureAsync(host);
                    addedId = (await host.Core.OpenAsync(addedText)).Id;
                    host.Core.TabScrollOffset = 53;
                    host.Core.IsClosing = true;
                    host.Controller.PauseRestoration();
                }

                SessionTestProtocol.Check(await host.Controller.SaveSessionAsync().WaitAsync(TimeSpan.FromSeconds(15)),
                    "The terminal rescue snapshot could not retry discovery after the legacy file lock was released.");
                SessionTestProtocol.Check(preparations == 0 && host.Core.GetTabCount() == 1 &&
                    host.Core.GetSelectedTab()?.Id == addedId && host.Core.TabScrollOffset == 53 &&
                    ((FakeTextEditor)host.Core.GetSelectedReadyEditor()).Text == addedText,
                    "Terminal rescue discovery loaded hidden payloads or changed the user's current tab/scroll.");
            }

            await VerifyRescueRestartAndCloseAsync(owner, registry, originalIds, texts, addedId, addedText, 53);
        }
        finally { await SessionTestProtocol.CleanupAsync(owner); }
    }

    private static async Task VerifyFailedRescueRestartAsync()
    {
        var owner = Guid.NewGuid();
        var registry = new SessionRegistry();
        const string addedText = "new tab after failed rescues 日本語𐐷\r";
        try
        {
            var (originalIds, texts) = await CreateRescueWorkspaceAsync(owner, registry);
            Guid addedId;
            await using (var host = new ControllerHost(owner, registry))
            {
                host.Controller.IsBackupEnabled = false;
                var preparations = 0;
                host.Controller.BeforeRestorePreparation = (_, _) =>
                {
                    preparations++;
                    throw new IOException("Injected failure before loading either rescue payload.");
                };
                SessionTestProtocol.Check(await host.Controller.LoadLastSessionAsync().WaitAsync(TimeSpan.FromSeconds(15)) == 0 &&
                    preparations == texts.Length && host.Controller.UnrecoveredEditorCount == texts.Length &&
                    host.Core.GetTabCount() == 0,
                    "The failed-rescue fixture did not leave both imports failed and hidden.");
                addedId = (await host.Core.OpenAsync(addedText)).Id;
                host.Controller.IsBackupEnabled = true;
                SessionTestProtocol.Check(await host.Controller.SaveSessionAsync().WaitAsync(TimeSpan.FromSeconds(15)) &&
                    host.Core.GetTabCount() == 1 && host.Core.GetSelectedTab()?.Id == addedId && preparations == texts.Length,
                    "Saving the new tab retried failed rescue payloads or changed current selection.");
            }

            await VerifyRescueRestartAndCloseAsync(owner, registry, originalIds, texts, addedId, addedText);
        }
        finally { await SessionTestProtocol.CleanupAsync(owner); }
    }

    private static async Task<(Guid[] Ids, string[] Texts)> CreateRescueWorkspaceAsync(Guid owner, SessionRegistry registry)
    {
        var fixture = await CreateOutcomeWorkspaceAsync(owner, registry);
        await Task.Run(async () =>
        {
            using (await SessionRecoveryTransaction.EnterAsync())
            {
                var store = RecoveryRootStore.CreateStore();
                var snapshot = await SessionRecoveryCatalog.ReadScopeAsync(owner);
                SessionTestProtocol.Check(snapshot.Checkpoints.Count != 0 &&
                    fixture.Ids.All(id => snapshot.Rescues.Any(read => read.Record.Editor.Id == id)),
                    "The rescue fixture did not create checkpoints and independent rescue records for both native documents.");
                foreach (var checkpoint in snapshot.Checkpoints)
                    foreach (var copy in new[] { "a", "b" })
                        File.WriteAllText(store.Paths.CopyPath(checkpoint.Address, copy), "invalid checkpoint");
            }
        });
        return fixture;
    }

    private static async Task VerifyRescueRestartAndCloseAsync(Guid owner, SessionRegistry registry,
        Guid[] originalIds, string[] rescueTexts, Guid selectedId, string addedText = null, double scrollOffset = 0)
    {
        var expectedTexts = addedText == null ? rescueTexts : rescueTexts.Append(addedText).ToArray();
        Dictionary<Guid, string> retained;
        Guid retainedSelection;
        await using (var restarted = new ControllerHost(owner, registry))
        {
            restarted.Controller.IsBackupEnabled = false;
            await restarted.Controller.LoadLastSessionAsync().WaitAsync(TimeSpan.FromSeconds(15));
            await restarted.Controller.WaitForRestorationAsync().WaitAsync(TimeSpan.FromSeconds(15));
            var tabs = restarted.Core.GetTabs();
            SessionTestProtocol.Check(tabs.Length == expectedTexts.Length &&
                tabs.All(tab => tab.State == WorkspaceTabState.Ready && tab.ReadyEditor is FakeTextEditor),
                "A fresh host lost a hidden rescue after an ordinary snapshot and disposal of the old recovery plan.");
            var editors = tabs.Select(tab => (FakeTextEditor)tab.ReadyEditor).ToArray();
            SessionTestProtocol.Check(expectedTexts.All(text => editors.Count(editor => editor.Text == text) == 1) &&
                editors.Where(editor => rescueTexts.Contains(editor.Text)).All(editor =>
                    !originalIds.Contains(editor.Id) && editor.Document.Metadata.RequiresSaveAs && editor.EditingFile == null) &&
                restarted.Core.GetSelectedTab()?.Id == selectedId && restarted.Core.TabScrollOffset == scrollOffset,
                "Fresh rescue recovery duplicated or changed Unicode content, reused an original identity/file binding, or lost current selection/scroll.");

            restarted.Controller.IsBackupEnabled = true;
            SessionTestProtocol.Check(await restarted.Controller.SaveSessionAsync().WaitAsync(TimeSpan.FromSeconds(15)),
                "The fully recovered rescue workspace did not commit its adopted documents.");
            // Keep the user's current selection and close another recovered document.
            var closing = editors.First(editor => rescueTexts.Contains(editor.Text) && editor.Id != selectedId);
            var closed = new HashSet<Guid>();
            SessionTestProtocol.Check(await restarted.Controller.PrepareExplicitCloseAsync(new[] { closing.Id }, closed) &&
                closed.SetEquals(new[] { closing.Id }), "An adopted rescue could not be closed durably.");
            restarted.Core.Close(closing);
            SessionTestProtocol.Check(await restarted.Controller.SaveSessionAsync().WaitAsync(TimeSpan.FromSeconds(15)),
                "The rescue workspace did not commit after closing an adopted document.");
            retained = restarted.Core.GetTabs().ToDictionary(tab => tab.Id, tab => ((FakeTextEditor)tab.ReadyEditor).Text);
            retainedSelection = restarted.Core.GetSelectedTab().Id;
        }

        await using var afterClose = new ControllerHost(owner, registry);
        afterClose.Controller.IsBackupEnabled = false;
        await afterClose.Controller.LoadLastSessionAsync().WaitAsync(TimeSpan.FromSeconds(15));
        await afterClose.Controller.WaitForRestorationAsync().WaitAsync(TimeSpan.FromSeconds(15));
        var finalTabs = afterClose.Core.GetTabs();
        SessionTestProtocol.Check(finalTabs.Length == retained.Count &&
            finalTabs.All(tab => tab.ReadyEditor is FakeTextEditor editor &&
                retained.TryGetValue(tab.Id, out var text) && editor.Text == text) &&
            finalTabs.Select(tab => tab.Id).Distinct().Count() == retained.Count &&
            afterClose.Core.GetSelectedTab()?.Id == retainedSelection,
            "Restart duplicated a rescue, resurrected its durably closed source, or lost the retained Unicode tab/selection.");
    }
}
