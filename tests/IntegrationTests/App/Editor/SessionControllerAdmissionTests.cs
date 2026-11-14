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
using Notepads.Features.Sessions.Recovery;
using Notepads.Features.Sessions.Storage;

namespace NotepadsEditorTests;

internal static partial class SessionControllerTests
{
    private static async Task VerifyRecoveryAdmissionAsync(StringBuilder log)
    {
        await VerifyRestoreRequestedDuringSaveDiscoveryAsync(failDiscovery: false);
        await VerifyRestoreRequestedDuringSaveDiscoveryAsync(failDiscovery: true);
        await VerifyRestoreRequestedAfterArchiveDecisionAsync();
        log.AppendLine("PASS: save rechecks restore intent after discovery and preserves the archive decision across failure.");
    }

    private static async Task VerifyRestoreRequestedDuringSaveDiscoveryAsync(bool failDiscovery)
    {
        var owner = Guid.NewGuid();
        var registry = new SessionRegistry();
        try
        {
            var (ids, texts) = await CreateOutcomeWorkspaceAsync(owner, registry);
            await using var host = new ControllerHost(owner, registry);
            var added = await host.Core.OpenAsync("new document during discovery Ελληνικά😀\r");
            var discovering = NewLookupSignal();
            var releaseDiscovery = NewLookupSignal();
            var releasePreparation = NewLookupSignal();
            host.Controller.BeforeRecoveryDiscovery = async cancellation =>
            {
                discovering.TrySetResult(null);
                await releaseDiscovery.Task.WaitAsync(cancellation);
                if (failDiscovery) throw new IOException("Injected discovery failure after restore intent was requested.");
            };
            host.Controller.BeforeRestorePreparation = (_, cancellation) => releasePreparation.Task.WaitAsync(cancellation);
            var saving = host.Controller.SaveSessionAsync();
            await discovering.Task.WaitAsync(TimeSpan.FromSeconds(10));
            // Startup intent is synchronous and can arrive while a save owns the
            // coordinator. It must affect the save's post-discovery decision.
            host.Controller.RequestRestoration();
            releaseDiscovery.SetResult(null);
            if (failDiscovery)
            {
                SessionTestProtocol.Check(!await saving.WaitAsync(TimeSpan.FromSeconds(15)) && host.Core.GetTabCount() == 1,
                    "Failed metadata discovery admitted sources or acknowledged its save.");
                host.Controller.BeforeRecoveryDiscovery = null;
                saving = host.Controller.SaveSessionAsync();
            }
            SessionTestProtocol.Check(await saving.WaitAsync(TimeSpan.FromSeconds(15)),
                "Save did not retry and admit requested recovery after metadata discovery.");
            var tabs = host.Core.GetTabs();
            SessionTestProtocol.Check(tabs.Select(tab => tab.Id).Order().SequenceEqual(ids.Append(added.Id).Order()) &&
                host.Core.GetSelectedTab()?.Id == added.Id && tabs.Where(tab => tab.Id != added.Id).All(tab => tab.ReadyEditor == null),
                "Save ignored restore intent, changed selection, or awaited recovery payloads under its coordinator.");
            var saved = await SessionRecoveryCatalog.ReadScopeAsync(owner);
            SessionTestProtocol.Check(saved.Selection.Documents.Where(row => row.Membership == RecoveryMembership.Owned).Select(row => row.Id).Order()
                    .SequenceEqual(ids.Append(added.Id).Order()) &&
                !saved.Decisions.Any(read => read.Record.Kind == RecoveryRecordKind.Closed && ids.Contains(read.Record.ClosedEditorId)),
                "Restore intent arriving during discovery archived the requested old workspace.");
            releasePreparation.SetResult(null);
            await host.Controller.WaitForRestorationCheckpointAsync().WaitAsync(TimeSpan.FromSeconds(15));
            SessionTestProtocol.Check(ids.Select(id => ((FakeTextEditor)host.Core.GetTabs().Single(tab => tab.Id == id).ReadyEditor).Text)
                    .SequenceEqual(texts),
                "Recovery admitted by the saving discovery lost its original Unicode content.");
        }
        finally { await SessionTestProtocol.CleanupAsync(owner); }
    }

    private static async Task VerifyRestoreRequestedAfterArchiveDecisionAsync()
    {
        var owner = Guid.NewGuid();
        var registry = new SessionRegistry();
        try
        {
            var (ids, _) = await CreateOutcomeWorkspaceAsync(owner, registry);
            Guid addedId;
            await using (var host = new ControllerHost(owner, registry))
            {
                addedId = (await host.Core.OpenAsync("workspace enabled after startup العربية😀\r")).Id;
                var archiving = NewLookupSignal();
                var releaseArchive = NewLookupSignal();
                host.Controller.BeforeRecoveryArchive = async cancellation =>
                {
                    archiving.TrySetResult(null);
                    await releaseArchive.Task.WaitAsync(cancellation);
                    throw new IOException("Injected archive failure after the archive decision.");
                };
                var saving = host.Controller.SaveSessionAsync();
                await archiving.Task.WaitAsync(TimeSpan.FromSeconds(10));
                host.Controller.RequestRestoration();
                SessionTestProtocol.Check(await host.Controller.LoadLastSessionAsync().WaitAsync(TimeSpan.FromSeconds(10)) == 0 &&
                    host.Core.GetTabCount() == 1,
                    "Load after the archive decision reinterpreted the frozen plan as requested restoration.");
                releaseArchive.SetResult(null);
                SessionTestProtocol.Check(!await saving.WaitAsync(TimeSpan.FromSeconds(15)),
                    "The injected archive failure was reported as a successful save.");
                host.Controller.BeforeRecoveryArchive = null;
                SessionTestProtocol.Check(await host.Controller.LoadLastSessionAsync() == 0 &&
                    await host.Controller.SaveSessionAsync().WaitAsync(TimeSpan.FromSeconds(15)),
                    "Archive retry lost its decision or could not complete after the failure.");
                var saved = await SessionRecoveryCatalog.ReadScopeAsync(owner);
                SessionTestProtocol.Check(host.Core.GetTabCount() == 1 &&
                    saved.Selection.Documents.Where(row => row.Membership == RecoveryMembership.Owned).Select(row => row.Id).SequenceEqual(new[] { addedId }) &&
                    ids.All(id => saved.Decisions.Any(read => read.Record.Kind == RecoveryRecordKind.Closed && read.Record.ClosedEditorId == id)),
                    "A restore request arriving after the archive decision revived an archived source or prevented retry.");
            }
            await using var restarted = new ControllerHost(owner, registry);
            SessionTestProtocol.Check(await restarted.Controller.LoadLastSessionAsync() == 1 &&
                restarted.Core.GetSelectedTab()?.Id == addedId,
                "Restart revived sources archived after a late restore request.");
        }
        finally { await SessionTestProtocol.CleanupAsync(owner); }
    }
}
