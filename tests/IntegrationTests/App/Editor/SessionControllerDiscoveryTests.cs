// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Notepads.Composition;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Contracts.Recovery;
using Notepads.Features.Sessions.Contracts.Legacy;
using Notepads.Features.Sessions.Recovery;
using Notepads.Features.Sessions.Storage;
using Notepads.Presentation.Workspace;
using Windows.Storage;

namespace NotepadsEditorTests;

internal static partial class SessionControllerTests
{
    private static async Task VerifyDiscoveryRetryAsync(StringBuilder log)
    {
        await VerifyDiscoveryBackupRetryAsync(closing: false);
        await VerifyDiscoveryBackupRetryAsync(closing: true);
        await VerifyExplicitDiscoveryRetryAsync(reset: false);
        await VerifyExplicitDiscoveryRetryAsync(reset: true);
        await VerifyDeliberatelySkippedRecoveryAsync();
        log.AppendLine("PASS: failed discovery preserves old and new tabs through retry, restart, close and reset.");
    }

    private static async Task VerifyDiscoveryBackupRetryAsync(bool closing)
    {
        var owner = Guid.NewGuid();
        var registry = new SessionRegistry();
        var addedText = "typed after discovery failed 中文😀\r";
        try
        {
            var (ids, texts) = await CreateOutcomeWorkspaceAsync(owner, registry);
            await SessionTestProtocol.WriteLegacySourceAsync(owner, new NotepadsSessionDataV1());
            var manifest = Path.Combine(ApplicationData.Current.LocalFolder.Path, SessionScopeData.GetManifestFileName(owner));
            Guid addedId;
            await using (var host = new ControllerHost(owner, registry))
            {
                using (new FileStream(manifest, FileMode.Open, FileAccess.Read, FileShare.None))
                {
                    await ExpectDiscoveryFailureAsync(host);
                    var added = await host.Core.OpenAsync(addedText);
                    addedId = added.Id;
                    host.Core.TabScrollOffset = 47;
                    for (var attempt = 0; attempt < 2; attempt++)
                    {
                        var acknowledged = false;
                        SessionTestProtocol.Check(!await host.Controller.SaveSessionAsync(() => acknowledged = true) && !acknowledged,
                            "A failed discovery backup was acknowledged as saved.");
                        var unchanged = await SessionRecoveryCatalog.ReadScopeAsync(owner);
                        SessionTestProtocol.Check(unchanged.Selection.Documents.Where(row => row.Membership == RecoveryMembership.Owned).Select(row => row.Id).SequenceEqual(ids) &&
                            !unchanged.Decisions.Any(read => read.Record.Kind == RecoveryRecordKind.Closed),
                            "A failed discovery backup replaced or closed the original checkpoint.");
                    }
                }

                var preparation = new TaskCompletionSource<object>(TaskCreationOptions.RunContinuationsAsynchronously);
                var release = new TaskCompletionSource<object>(TaskCreationOptions.RunContinuationsAsynchronously);
                host.Controller.BeforeRestorePreparation = async (_, cancellation) =>
                {
                    preparation.TrySetResult(null);
                    await release.Task.WaitAsync(cancellation);
                };
                if (closing)
                {
                    host.Core.IsClosing = true;
                    host.Controller.PauseRestoration();
                }
                SessionTestProtocol.Check(await host.Controller.SaveSessionAsync().WaitAsync(TimeSpan.FromSeconds(15)),
                    "Backup could not retry discovery after the file lock was released.");
                var tabs = host.Core.GetTabs();
                SessionTestProtocol.Check(tabs.Select(tab => tab.Id).Order().SequenceEqual(ids.Append(addedId).Order()) &&
                    tabs.Length == 3 && host.Core.GetSelectedTab()?.Id == addedId && host.Core.TabScrollOffset == 47 &&
                    tabs.Where(tab => tab.Id != addedId).All(tab => tab.ReadyEditor == null),
                    "Discovery retry retired old tabs, duplicated admission, stole selection/scroll, or awaited payload loading.");
                var saved = await SessionRecoveryCatalog.ReadScopeAsync(owner);
                SessionTestProtocol.Check(saved.Selection.Documents.Where(row => row.Membership == RecoveryMembership.Owned).Select(row => row.Id).SequenceEqual(tabs.Select(tab => tab.Id)) &&
                    saved.Selection.SelectedEditorId == addedId &&
                    !saved.Decisions.Any(read => read.Record.Kind == RecoveryRecordKind.Closed),
                    "Discovery retry omitted an old or new tab or published an unauthorized Closed decision.");
                if (closing)
                {
                    SessionTestProtocol.Check(!preparation.Task.IsCompleted, "Terminal retry started payload loading while closing.");
                    await host.Controller.DrainAsync();
                }
                else
                {
                    await preparation.Task.WaitAsync(TimeSpan.FromSeconds(10));
                    release.SetResult(null);
                    await host.Controller.WaitForRestorationCheckpointAsync().WaitAsync(TimeSpan.FromSeconds(15));
                    SessionTestProtocol.Check(await host.Controller.LoadLastSessionAsync() == 3,
                        "Successful backup retry left the cached startup failure active.");
                    SessionTestProtocol.Check(host.Core.GetTabs().Length == 3 && host.Core.GetSelectedTab()?.Id == addedId,
                        "Load after successful retry duplicated tabs or changed selection.");
                }
            }

            await using var restarted = new ControllerHost(owner, registry);
            SessionTestProtocol.Check(await restarted.Controller.LoadLastSessionAsync() == 3,
                "Restart lost the previous session after discovery failed and backup retried.");
            await restarted.Controller.WaitForRestorationCheckpointAsync();
            var restored = restarted.Core.GetTabs().ToDictionary(tab => tab.Id, tab => ((FakeTextEditor)tab.ReadyEditor).Text);
            SessionTestProtocol.Check(ids.Select(id => restored[id]).SequenceEqual(texts) && restored[addedId] == addedText &&
                restarted.Core.GetSelectedTab()?.Id == addedId,
                "Restart changed old/new Unicode text or the user's selected tab after retry.");
        }
        finally { await SessionTestProtocol.CleanupAsync(owner); }
    }

    private static async Task VerifyExplicitDiscoveryRetryAsync(bool reset)
    {
        var owner = Guid.NewGuid();
        var registry = new SessionRegistry();
        try
        {
            var (ids, texts) = await CreateOutcomeWorkspaceAsync(owner, registry);
            await SessionTestProtocol.WriteLegacySourceAsync(owner, new NotepadsSessionDataV1());
            var manifest = Path.Combine(ApplicationData.Current.LocalFolder.Path, SessionScopeData.GetManifestFileName(owner));
            await using var host = new ControllerHost(owner, registry);
            using (new FileStream(manifest, FileMode.Open, FileAccess.Read, FileShare.None))
                await ExpectDiscoveryFailureAsync(host);
            if (reset)
            {
                // Reset retries discovery and loads every row before erasing the old epoch.
                await host.Controller.ClearSessionDataAsync().WaitAsync(TimeSpan.FromSeconds(15));
                SessionTestProtocol.Check(host.Core.GetTabs().Select(tab => tab.Id).SequenceEqual(ids) &&
                    host.Core.GetTabs().Select(tab => ((FakeTextEditor)tab.ReadyEditor).Text).SequenceEqual(texts) &&
                    host.Core.GetTabs().All(tab => tab.RecoverySource == null),
                    "Reset after failed startup did not retry and materialize the old session before clearing recovery.");
            }
            else
            {
                var discovering = new TaskCompletionSource<object>(TaskCreationOptions.RunContinuationsAsynchronously);
                var release = new TaskCompletionSource<object>(TaskCreationOptions.RunContinuationsAsynchronously);
                host.Controller.BeforeRecoveryDiscovery = async cancellation =>
                {
                    discovering.SetResult(null);
                    await release.Task.WaitAsync(cancellation);
                };
                var retry = host.Controller.LoadLastSessionAsync();
                SessionTestProtocol.Check(ReferenceEquals(retry, host.Controller.LoadLastSessionAsync()),
                    "Concurrent explicit retries did not share the in-flight discovery task.");
                await discovering.Task.WaitAsync(TimeSpan.FromSeconds(10));
                var added = await host.Core.OpenAsync("selected during discovery Ω😀\r");
                host.Core.TabScrollOffset = 63;
                release.SetResult(null);
                SessionTestProtocol.Check(await retry == 3 && host.Core.GetSelectedTab()?.Id == added.Id &&
                    host.Core.TabScrollOffset == 63, "Retry overwrote a selection/scroll changed during discovery.");
                await host.Controller.WaitForRestorationCheckpointAsync();
                var restored = host.Core.GetTabs().ToDictionary(tab => tab.Id, tab => ((FakeTextEditor)tab.ReadyEditor).Text);
                SessionTestProtocol.Check(restored.Count == 3 && ids.Select(id => restored[id]).SequenceEqual(texts),
                    "Concurrent retry duplicated admission or changed recovered text.");
            }
        }
        finally { await SessionTestProtocol.CleanupAsync(owner); }
    }

    private static async Task VerifyDeliberatelySkippedRecoveryAsync()
    {
        var owner = Guid.NewGuid();
        var registry = new SessionRegistry();
        try
        {
            var (ids, _) = await CreateOutcomeWorkspaceAsync(owner, registry);
            Guid addedId;
            await using (var host = new ControllerHost(owner, registry))
            {
                addedId = (await host.Core.OpenAsync("snapshots enabled later Ω\r")).Id;
                SessionTestProtocol.Check(await host.Controller.SaveSessionAsync() && host.Core.GetTabCount() == 1,
                    "Enabling snapshots without requesting restoration unexpectedly admitted the old workspace.");
                var saved = await SessionRecoveryCatalog.ReadScopeAsync(owner);
                SessionTestProtocol.Check(saved.Selection.Documents.Where(row => row.Membership == RecoveryMembership.Owned).Select(row => row.Id).SequenceEqual(new[] { addedId }) &&
                    ids.All(id => saved.Decisions.Any(read => read.Record.Kind == RecoveryRecordKind.Closed && read.Record.ClosedEditorId == id)),
                    "Deliberately skipped recovery was not durably retired after enabling snapshots.");
            }
            await using var restarted = new ControllerHost(owner, registry);
            SessionTestProtocol.Check(await restarted.Controller.LoadLastSessionAsync() == 1 && restarted.Core.GetSelectedTab()?.Id == addedId,
                "Deliberately skipped tabs reappeared on restart.");
        }
        finally { await SessionTestProtocol.CleanupAsync(owner); }
    }

    private static async Task ExpectDiscoveryFailureAsync(ControllerHost host)
    {
        var failed = false;
        try { await host.Controller.LoadLastSessionAsync(); }
        catch (IOException) { failed = true; }
        SessionTestProtocol.Check(failed && host.Core.GetTabCount() == 0,
            "The exclusive legacy file lock did not fail startup discovery before tab admission.");
    }
}
