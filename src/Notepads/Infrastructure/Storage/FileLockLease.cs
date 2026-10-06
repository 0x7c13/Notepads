// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Threading;
using System.Threading.Tasks;

namespace Notepads.Infrastructure.Storage;

/// <summary>A process-released Windows file-sharing lease, independent of application policy.</summary>
internal sealed class FileLockLease : IDisposable
{
    private FileStream _handle;

    private FileLockLease(FileStream handle) => _handle = handle;

    public static Task<FileLockLease> AcquireExclusiveAsync(string path, CancellationToken cancellationToken)
    {
        return Task.Run(async () =>
        {
            while (true)
            {
                cancellationToken.ThrowIfCancellationRequested();
                var lease = TryAcquire(path);
                if (lease != null)
                {
                    if (!cancellationToken.IsCancellationRequested) return lease;
                    lease.Dispose();
                    cancellationToken.ThrowIfCancellationRequested();
                }
                await Task.Delay(20, cancellationToken).ConfigureAwait(false);
            }
        }, cancellationToken);
    }

    public static FileLockLease TryAcquire(string path, bool sharedReaders = false)
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
                catch (IOException ex) when ((ex.HResult & 0xffff) == 80 || (ex.HResult & 0xffff) == 183)
                {
                    // Another creator completed the permanent lock-file identity.
                }
            }
            return new FileLockLease(new FileStream(path,
                sharedReaders ? FileMode.Open : FileMode.OpenOrCreate,
                sharedReaders ? FileAccess.Read : FileAccess.ReadWrite,
                sharedReaders ? FileShare.Read : FileShare.None));
        }
        catch (IOException ex) when ((ex.HResult & 0xffff) == 32 || (ex.HResult & 0xffff) == 33)
        {
            return null;
        }
    }

    public void Dispose() => Interlocked.Exchange(ref _handle, null)?.Dispose();
}
