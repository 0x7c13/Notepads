// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Diagnostics;
using System.IO;
using System.Threading;
using System.Threading.Tasks;

namespace Notepads.Infrastructure.Storage;

/// <summary>A process-released Windows file-sharing lease, independent of application policy.</summary>
internal sealed partial class FileLockLease : IDisposable
{
    private const int FileExists = 80;
    private const int AlreadyExists = 183;
    private const int SharingViolation = 32;
    private const int LockViolation = 33;
    private FileStream _handle;

    private FileLockLease(FileStream handle) => _handle = handle;

    public static Task<FileLockLease> AcquireExclusiveAsync(string path, CancellationToken cancellationToken) =>
        AcquireExclusiveAsync(path, cancellationToken, temporaryLeaseTimeout: null);

    public static Task<FileLockLease> AcquireTemporaryExclusiveAsync(string path, TimeSpan timeout, CancellationToken cancellationToken) =>
        AcquireExclusiveAsync(path, cancellationToken, timeout);

    private static Task<FileLockLease> AcquireExclusiveAsync(string path, CancellationToken cancellationToken, TimeSpan? temporaryLeaseTimeout)
    {
        return Task.Run(async () =>
        {
            var started = Stopwatch.GetTimestamp();
            var observedContention = false;
            while (true)
            {
                cancellationToken.ThrowIfCancellationRequested();
                var expired = temporaryLeaseTimeout.HasValue && Stopwatch.GetElapsedTime(started) >= temporaryLeaseTimeout.Value;
                FileLockLease lease;
                try { lease = TryAcquire(path, sharedReaders: false, deleteOnClose: temporaryLeaseTimeout.HasValue); }
                catch (UnauthorizedAccessException) when (temporaryLeaseTimeout.HasValue && !expired)
                {
                    // A metadata reader can release the delete-pending name as the open fails.
                    // Only a fresh open can distinguish that race from persistent access denial.
                    lease = null;
                }
                if (lease != null)
                {
                    if (!cancellationToken.IsCancellationRequested) return lease;
                    lease.Dispose();
                    cancellationToken.ThrowIfCancellationRequested();
                }
                if (expired) throw new TimeoutException();
                if (!observedContention)
                {
                    OnWaitingForLease(path);
                    observedContention = true;
                }
                await Task.Delay(20, cancellationToken).ConfigureAwait(false);
            }
        }, cancellationToken);
    }

    static partial void OnWaitingForLease(string path);

    public static FileLockLease TryAcquire(string path, bool sharedReaders = false) =>
        TryAcquire(path, sharedReaders, deleteOnClose: false);

    private static FileLockLease TryAcquire(string path, bool sharedReaders, bool deleteOnClose)
    {
        ArgumentException.ThrowIfNullOrEmpty(path);
        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(path)));
        try
        {
            if (sharedReaders && !File.Exists(path))
            {
                try
                {
                    using var created = new FileStream(path, FileMode.CreateNew,
                        FileAccess.ReadWrite, FileShare.ReadWrite);
                }
                catch (IOException ex) when (Win32Code(ex) is FileExists or AlreadyExists)
                {
                    // Another creator completed the permanent lock-file identity.
                }
            }
            return new FileLockLease(new FileStream(path,
                sharedReaders ? FileMode.Open : FileMode.OpenOrCreate,
                sharedReaders ? FileAccess.Read : FileAccess.ReadWrite,
                sharedReaders ? FileShare.Read : FileShare.None, bufferSize: 1,
                deleteOnClose ? FileOptions.DeleteOnClose : FileOptions.None));
        }
        catch (IOException ex) when (Win32Code(ex) is SharingViolation or LockViolation)
        {
            return null;
        }
    }

    private static int Win32Code(IOException exception) => exception.HResult & 0xffff;

    public void Dispose() => Interlocked.Exchange(ref _handle, null)?.Dispose();
}
