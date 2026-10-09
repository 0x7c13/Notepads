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
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Recovery;
using Notepads.Features.Sessions.Storage;

namespace NotepadsEditorTests;

internal static class SessionPerformanceRegressionTests
{
    public static async Task RunAsync(StringBuilder log)
    {
        await CheckDeltaRescueAsync();
        log.AppendLine("PASS: delta rescue publication, damaged evidence replacement, failed-checkpoint retry and reset epoch.");
    }

    private static async Task CheckDeltaRescueAsync()
    {
        var owner = Guid.NewGuid();
        var faults = new PublicationObserver();
        var store = new RecoveryRecordStore(RecoveryRootStore.CreateStore().Paths.RootPath, faults);
        var service = SessionTestProtocol.CreateService(owner, store);
        var documents = new List<SessionTestDocument>();
        var identities = Enumerable.Range(0, 3).Select(_ => Guid.NewGuid()).ToArray();
        try
        {
            await service.InitializeAuthorityAsync(CancellationToken.None);
            foreach (var id in identities)
                documents.Add(await SessionTestDocument.CreateAsync(owner, "delta العربية😀\r"));
            using (var initial = SessionTestProtocol.CaptureDocuments(documents, identities, service.CurrentStamp))
                SessionTestProtocol.Check((await service.SaveAsync(initial, CancellationToken.None)).Succeeded, "Initial delta fixture failed.");
            SessionTestProtocol.Check(faults.RescueCopies == 6, "First publication must establish all rescue mirrors.");
            using (var selection = SessionTestProtocol.CaptureDocuments(documents, identities, service.CurrentStamp))
            {
                selection.SelectedEditorId = identities[1];
                var result = await service.SaveAsync(selection, CancellationToken.None);
                SessionTestProtocol.Check(result.Succeeded && result.Changed && faults.RescueCopies == 6,
                    "Selection-only publication rewrote unchanged rescue evidence.");
            }
            documents[0].Editor.InsertText(0, "edited\r");
            using (var changed = SessionTestProtocol.CaptureDocuments(documents, identities, service.CurrentStamp))
            {
                SessionTestProtocol.Check((await service.SaveAsync(changed, CancellationToken.None)).Succeeded && faults.RescueCopies == 8,
                    "One document change must publish just its two rescue copies.");
            }

            using (await SessionRecoveryTransaction.EnterAsync())
            {
                var address = store.Enumerate(store.GetScopeArea(owner, RecoveryAreaKind.Rescue, identities[1])).Entries.MaxBy(item => item.Ordinal);
                File.WriteAllText(store.Paths.CopyPath(address, "a"), "damaged A");
                File.WriteAllText(store.Paths.CopyPath(address, "b"), "damaged B");
            }
            using (var missing = SessionTestProtocol.CaptureDocuments(documents, identities, service.CurrentStamp))
            {
                missing.TabScrollOffset = 5;
                SessionTestProtocol.Check((await service.SaveAsync(missing, CancellationToken.None)).Succeeded && faults.RescueCopies == 10,
                    "A committed descriptor cannot substitute for missing verified rescue evidence.");
            }
            documents[0].Editor.InsertText(0, "retry\r");
            faults.FailCheckpoint = true;
            using (var failed = SessionTestProtocol.CaptureDocuments(documents, identities, service.CurrentStamp))
            {
                SessionTestProtocol.Check(!(await service.SaveAsync(failed, CancellationToken.None)).Succeeded && faults.RescueCopies == 12,
                    "The fixture must fail its checkpoint after publishing the changed rescue.");
            }

            faults.FailCheckpoint = false;
            using (var retry = SessionTestProtocol.CaptureDocuments(documents, identities, service.CurrentStamp))
            {
                SessionTestProtocol.Check((await service.SaveAsync(retry, CancellationToken.None)).Succeeded && faults.RescueCopies == 14,
                    "An uncommitted checkpoint must remain retryable without rewriting other documents.");
            }

            var previousEpoch = service.CurrentStamp.EpochId;
            await service.ClearSessionDataAsync([]);
            SessionTestProtocol.Check(service.CurrentStamp.EpochId != previousEpoch, "Reset did not change the epoch.");
            using (var reset = SessionTestProtocol.CaptureDocuments(documents, identities, service.CurrentStamp))
            {
                SessionTestProtocol.Check((await service.SaveAsync(reset, CancellationToken.None)).Succeeded && faults.RescueCopies == 20,
                    "A new epoch must publish fresh rescue evidence for every document.");
            }

            foreach (var document in documents) await document.DisposeAsync();
            documents.Clear();
            await service.DisposeAsync();
            service = SessionTestProtocol.CreateService(owner);
            SessionTestProtocol.Check(await service.PrepareExplicitCloseAsync(new[] { identities[1] }),
                "Explicit close must initialize authority in a fresh service after reset.");
            using (await SessionRecoveryTransaction.EnterAsync())
            {
                var snapshot = await SessionRecoveryCatalog.ReadScopeAsync(owner);
                SessionTestProtocol.Check(snapshot.Session.TextEditors.Select(editor => editor.Id).SequenceEqual(new[] { identities[0], identities[2] }),
                    "Delta checkpoints must retain full tab order and honor durable closure.");
            }
        }
        finally
        {
            foreach (var document in documents) await document.DisposeAsync();
            await service.DisposeAsync();
            await SessionTestProtocol.CleanupAsync(owner);
        }
    }

    private sealed class PublicationObserver : IRecoveryStorageFaults
    {
        public int RescueCopies { get; private set; }
        public bool FailCheckpoint { get; set; }
        public void OnStep(RecoveryStorageStep step, RecoveryAddress address, string copy)
        {
            if (step != RecoveryStorageStep.BeforeRename) return;
            if (address.Area.Kind == RecoveryAreaKind.Rescue) RescueCopies++;
            if (FailCheckpoint && address.Area.Kind == RecoveryAreaKind.Checkpoints)
                throw new IOException("Injected checkpoint failure after rescue publication.");
        }
    }
}
