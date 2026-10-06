// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Infrastructure.Storage;
using Windows.Storage;

namespace Notepads.Features.Sessions.Storage;

/// <summary>
/// Serialize durable root/grant publication and reference-scan-through-delete.
/// OS admission protects every process; scope pins and asset leases protect
/// native writers, immutable candidates and readers outside this short gate.
/// </summary>
internal static class SessionRecoveryTransaction
{
    private static readonly SemaphoreSlim Gate = new(1, 1);

    public static async Task<IDisposable> EnterAsync(CancellationToken cancellationToken = default)
    {
        await Gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            var path = Path.Combine(ApplicationData.Current.LocalFolder.Path, "Recovery", "store.lock");
            var fileLease = await FileLockLease.AcquireExclusiveAsync(path, cancellationToken).ConfigureAwait(false);
            return new Lease(fileLease);
        }
        catch
        {
            Gate.Release();
            throw;
        }
    }

    private sealed class Lease : IDisposable
    {
        private int _disposed;
        private readonly IDisposable _fileLease;

        public Lease(IDisposable fileLease) => _fileLease = fileLease;

        public void Dispose()
        {
            if (Interlocked.Exchange(ref _disposed, 1) != 0) return;
            try { _fileLease.Dispose(); }
            finally { Gate.Release(); }
        }
    }
}
