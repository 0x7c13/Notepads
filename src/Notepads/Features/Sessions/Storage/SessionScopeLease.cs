// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.IO;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Infrastructure.Storage;
using Windows.Storage;

namespace Notepads.Features.Sessions.Storage;

/// <summary>Writer ownership and reader protection shared by every process using this recovery store.</summary>
internal static class SessionScopeLease
{
    private static readonly object Sync = new();
    // Ownership ends only through explicit disposal or process exit. Keeping
    // the lease rooted also protects a workspace whose native drain failed.
    private static readonly Dictionary<Guid, WriterLease> Writers = new();

    public static async Task<IDisposable> AcquireWriterAsync(Guid ownerId, CancellationToken cancellationToken = default)
    {
        var lease = await Task.Run(() =>
        {
            cancellationToken.ThrowIfCancellationRequested();
            return FileLockLease.TryAcquire(GetPath(ownerId, "writer.lock"));
        }, cancellationToken).ConfigureAwait(false);
        if (lease == null) throw new IOException("The recovery workspace already has an active writer.");
        if (cancellationToken.IsCancellationRequested)
        {
            lease.Dispose();
            cancellationToken.ThrowIfCancellationRequested();
        }
        var writer = new WriterLease(ownerId, lease);
        try
        {
            lock (Sync) Writers.Add(ownerId, writer);
            return writer;
        }
        catch { lease.Dispose(); throw; }
    }

    public static bool HasWriterLease(Guid ownerId)
    {
        lock (Sync) return Writers.ContainsKey(ownerId);
    }

    // These nonblocking acquisitions run under store admission. Waiting there
    // would prevent a holder from publishing and releasing its own protection.
    public static IDisposable TryAcquireInactiveWriter(Guid ownerId) => HasWriterLease(ownerId) ? null :
        FileLockLease.TryAcquire(GetPath(ownerId, "writer.lock"));

    public static IDisposable AcquireReader(Guid ownerId) =>
        FileLockLease.TryAcquire(GetPath(ownerId, "readers.lock"), sharedReaders: true) ??
        throw new IOException("The recovery workspace is being retired.");

    public static IDisposable TryAcquireExclusiveReaders(Guid ownerId) =>
        FileLockLease.TryAcquire(GetPath(ownerId, "readers.lock"));

    private static string GetPath(Guid ownerId, string fileName)
    {
        if (ownerId == Guid.Empty) throw new ArgumentException("A recovery lease requires an owner.", nameof(ownerId));
        return Path.Combine(ApplicationData.Current.LocalFolder.Path, "Recovery", "Scopes", ownerId.ToString("N"), fileName);
    }

    private sealed class WriterLease : IDisposable
    {
        private readonly Guid _ownerId;
        private IDisposable _lease;

        public WriterLease(Guid ownerId, IDisposable lease) { _ownerId = ownerId; _lease = lease; }

        public void Dispose()
        {
            var lease = Interlocked.Exchange(ref _lease, null);
            if (lease == null) return;
            lock (Sync)
            {
                Writers.Remove(_ownerId);
                lease.Dispose();
            }
        }
    }
}
