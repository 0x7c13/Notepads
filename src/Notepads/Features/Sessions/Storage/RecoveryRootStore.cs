// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Sessions.Contracts;
using Windows.Storage;

namespace Notepads.Features.Sessions.Storage;

/// <summary>WinRT root adapter; every new record uses the shared immutable metadata repository.</summary>
internal static class RecoveryRootStore
{
    internal static RecoveryRecordStore CreateStore() =>
        new(Path.Combine(ApplicationData.Current.LocalFolder.Path, "Recovery"));

    internal static async Task<RecoveryReadResult> ReadTransferAsync(Guid transferId, RecoveryRecordKind kind,
        CancellationToken cancellation = default)
    {
        var store = CreateStore();
        var area = store.GetGlobalArea(RecoveryAreaKind.Transfers, transferId);
        var scan = store.Enumerate(area);
        if (!scan.IsComplete) throw new InvalidDataException("The transfer directory cannot be enumerated reliably.");
        RecoveryReadResult found = null;
        foreach (var address in scan.Entries)
        {
            var read = await store.ReadAsync(address, true, cancellation);
            if (read.State != RecoveryReadState.Valid)
                throw new InvalidDataException("A durable transfer record is unreadable or conflicting.", read.Error);
            if (read.Record.Kind != kind) continue;
            if (found != null) throw new InvalidDataException("A transfer has conflicting lifecycle operations.");
            found = read;
        }
        return found ?? throw new FileNotFoundException("The transfer lifecycle record is absent.");
    }

    internal static Task<RecoveryPublicationResult> PublishTransferAsync(Guid transferId, RecoveryRecord record,
        CancellationToken cancellation = default)
    {
        var store = CreateStore();
        var area = store.GetGlobalArea(RecoveryAreaKind.Transfers, transferId);
        record.Stamp.Ordinal = store.AllocateOrdinal(area);
        record.Stamp.OperationId = Guid.NewGuid();
        return store.PublishAsync(area, record, cancellation);
    }

    internal static void RequireCommitted(RecoveryPublicationResult result)
    {
        if (!result.IsCommitted)
        {
            throw new IOException(result.State == RecoveryPublicationState.Indeterminate ?
            "Recovery publication may have committed; resources have been retained and closure is blocked." :
            "Recovery publication did not commit.", result.Error);
        }
    }
}
