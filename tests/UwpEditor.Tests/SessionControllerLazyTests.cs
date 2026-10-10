// ---------------------------------------------------------------------------------------------
// Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
// See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Composition;
using Notepads.Features.Sessions.Contracts.Recovery;
using Notepads.Features.Sessions.Recovery;
using Notepads.Features.Sessions.Storage;
using Notepads.Presentation.Workspace;

namespace NotepadsEditorTests;

internal static partial class SessionControllerTests
{
    private static async Task VerifyLazyWorkspaceAsync(StringBuilder log)
    {
        var owner = Guid.NewGuid();
        var registry = new SessionRegistry();
        Guid[] ids;
        try
        {
            await using (var seed = new ControllerHost(owner, registry))
            {
                for (var index = 0; index < 4; index++) await seed.Core.OpenAsync($"lazy {index} العربية 😀\r");
                ids = seed.Core.GetTabs().Select(tab => tab.Id).ToArray();
                SessionTestProtocol.Check(await seed.Controller.SaveSessionAsync(), "Lazy fixture did not publish.");
            }
            await using (var host = new ControllerHost(owner, registry))
            {
                var blocked = new TaskCompletionSource<object>(TaskCreationOptions.RunContinuationsAsynchronously);
                var release = new TaskCompletionSource<object>(TaskCreationOptions.RunContinuationsAsynchronously);
                var starts = new List<Guid>();
                host.Controller.BeforeRestorePreparation = async (source, cancellation) =>
                {
                    starts.Add(source.Id);
                    if (source.Id == ids[0])
                    {
                        blocked.TrySetResult(null);
                        await release.Task.WaitAsync(cancellation);
                    }
                };
                SessionTestProtocol.Check(await host.Controller.LoadLastSessionAsync() == 4,
                    "The initial index did not restore all tab entries.");
                await blocked.Task.WaitAsync(TimeSpan.FromSeconds(10));
                var tabs = host.Core.GetTabs();
                SessionTestProtocol.Check(tabs.Select(tab => tab.Id).SequenceEqual(ids) &&
                    tabs.Count(tab => tab.ReadyEditor != null) == 1 && host.Core.GetSelectedReadyEditor()?.Id == ids[3],
                    "Startup did not present the selected document before blocked background content.");
                var retained = tabs[0].RecoverySource;
                SessionTestProtocol.Check(await host.Controller.SaveForSuspensionAsync(CancellationToken.None).WaitAsync(TimeSpan.FromSeconds(10)),
                    "A blocked payload prevented a suspension checkpoint.");
                SessionTestProtocol.Check(ReferenceEquals(retained, tabs[0].RecoverySource) &&
                    tabs[3].RecoverySource == null, "A retained capture acknowledged a pending representation, or a native capture was not acknowledged.");
                await Task.Run(async () =>
                {
                    using (await SessionRecoveryTransaction.EnterAsync())
                    {
                        var catalog = await SessionRecoveryCatalog.ReadScopeAsync(owner);
                        SessionTestProtocol.Check(catalog.Selection.Documents.Where(row => row.Membership == RecoveryMembership.Owned).Select(row => row.Id).SequenceEqual(ids) &&
                            catalog.Selection.Documents.Where(row => row.Membership == RecoveryMembership.Owned).Select(row => row.Descriptor).First().Journal.GenerationId == retained.Descriptor.Journal.GenerationId,
                            "A mixed checkpoint lost order or replaced unread content.");
                    }
                });
                var closed = new HashSet<Guid>();
                SessionTestProtocol.Check(await host.Controller.PrepareExplicitCloseAsync(new[] { ids[1] }, closed) && closed.Contains(ids[1]),
                    "Pending closure was not durable.");
                host.Core.CloseTab(tabs[1]);
                host.Core.SwitchTo(tabs[2]);
                var requested = host.Controller.EnsureLoadedAsync(tabs[2]);
                release.SetResult(null);
                SessionTestProtocol.Check((await requested.WaitAsync(TimeSpan.FromSeconds(15)))?.Id == ids[2],
                    "Selecting a pending tab did not complete its original task.");
                await host.Controller.WaitForRestorationCheckpointAsync();
                SessionTestProtocol.Check(starts.SequenceEqual(new[] { ids[3], ids[0], ids[2] }) &&
                    host.Core.GetTabs().All(tab => tab.State == WorkspaceTabState.Ready && tab.RecoverySource == null),
                    "Selection priority, durable closure, or native checkpoint acknowledgement failed.");
            }
            await using (var restart = new ControllerHost(owner, registry))
            {
                SessionTestProtocol.Check(await restart.Controller.LoadLastSessionAsync() == 3, "A closed pending document resurrected.");
                await restart.Controller.WaitForRestorationCheckpointAsync();
                SessionTestProtocol.Check(restart.Core.GetTabs().Select(tab => tab.Id).SequenceEqual(new[] { ids[0], ids[2], ids[3] }) &&
                    restart.Core.GetSelectedTab()?.Id == ids[2] &&
                    restart.Core.GetTabs().All(tab => ((FakeTextEditor)tab.ReadyEditor).Text.Contains("العربية 😀", StringComparison.Ordinal)),
                    "Fresh restoration lost ordered identities, selection, or Unicode content.");
            }
            await using (var reset = new ControllerHost(owner, registry))
            {
                var admitted = new TaskCompletionSource<object>(TaskCreationOptions.RunContinuationsAsynchronously);
                var release = new TaskCompletionSource<object>(TaskCreationOptions.RunContinuationsAsynchronously);
                reset.Controller.BeforeRestorePreparation = async (source, cancellation) =>
                {
                    admitted.TrySetResult(null);
                    await release.Task.WaitAsync(cancellation);
                };
                // Start reset before discovery has admitted the index. Its preflight
                // must include the tabs that discovery is about to add.
                var loading = reset.Controller.LoadLastSessionAsync();
                var clearing = reset.Controller.ClearSessionDataAsync();
                await admitted.Task.WaitAsync(TimeSpan.FromSeconds(10));
                SessionTestProtocol.Check(!clearing.IsCompleted, "Reset discarded startup entries before materializing them.");
                var oldEpoch = reset.Service.CurrentStamp.EpochId;
                release.SetResult(null);
                await Task.WhenAll(loading, clearing).WaitAsync(TimeSpan.FromSeconds(20));
                SessionTestProtocol.Check(reset.Service.CurrentStamp.EpochId != oldEpoch &&
                    reset.Core.GetTabs().All(tab => tab.ReadyEditor != null && tab.RecoverySource == null),
                    "Reset left an unloaded document or a source from the retired epoch.");
                SessionTestProtocol.Check(await reset.Controller.SaveSessionAsync(), "Materialized documents could not publish in the new epoch.");
            }
            await using (var closing = new ControllerHost(owner, registry))
            {
                var discovery = new TaskCompletionSource<object>(TaskCreationOptions.RunContinuationsAsynchronously);
                var release = new TaskCompletionSource<object>(TaskCreationOptions.RunContinuationsAsynchronously);
                closing.Controller.BeforeRecoveryDiscovery = async cancellation =>
                {
                    discovery.SetResult(null);
                    await release.Task.WaitAsync(cancellation);
                };
                var loading = closing.Controller.LoadLastSessionAsync();
                await discovery.Task;
                closing.Core.IsClosing = true;
                closing.Controller.PauseRestoration();
                var terminalSave = closing.Controller.SaveSessionAsync(() => closing.Controller.IsBackupEnabled = false);
                release.SetResult(null);
                SessionTestProtocol.Check(await terminalSave.WaitAsync(TimeSpan.FromSeconds(15)) &&
                    closing.Core.GetTabCount() == 3 && closing.Core.GetTabs().All(tab => tab.ReadyEditor == null),
                    "Closing during index discovery archived or dropped the previous roster.");
                await closing.Controller.DrainAsync();
                await loading;
            }
            await using (var afterEarlyClose = new ControllerHost(owner, registry))
            {
                SessionTestProtocol.Check(await afterEarlyClose.Controller.LoadLastSessionAsync() == 3,
                    "Closing before native loading lost the previous session.");
                await afterEarlyClose.Controller.WaitForRestorationCheckpointAsync();
            }
        }
        finally { await SessionTestProtocol.CleanupAsync(owner); }
        log.AppendLine("PASS: lazy restoration preserves mixed captures, pending tabs and selection priority.");
    }
}
