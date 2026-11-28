// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Infrastructure.Diagnostics;
using Windows.Storage;
using Windows.Storage.Streams;

namespace Notepads.Infrastructure.Storage;

internal sealed class AtomicFileWriteState
{
    public bool Opened { get; internal set; }
    public bool Committing { get; internal set; }
    public bool Committed { get; internal set; }
}

internal static class AtomicFileWriter
{
    /// <summary>
    /// Replace a file only after its complete new contents have been written.
    /// A failed writer or transaction leaves the previous version intact.
    /// </summary>
    public static Task WriteAsync(StorageFile file, Func<Stream, Task> writeAsync,
        CancellationToken cancellationToken = default)
    {
        return WriteAsync(file, writeAsync, new AtomicFileWriteState(), cancellationToken);
    }

    internal static async Task WriteAsync(StorageFile file, Func<Stream, Task> writeAsync,
        AtomicFileWriteState state, CancellationToken cancellationToken = default, Func<IInputStream, Task> verifyOriginal = null)
    {
        ArgumentNullException.ThrowIfNull(file);
        ArgumentNullException.ThrowIfNull(writeAsync);
        ArgumentNullException.ThrowIfNull(state);
        if (state.Opened || state.Committed) throw new ArgumentException("Each transaction requires a new outcome state.", nameof(state));
        cancellationToken.ThrowIfCancellationRequested();

        StorageStreamTransaction transaction = null;
        Stream stream = null;
        Exception writeFailure = null;
        try
        {
            transaction = await FileStorage.OpenTransactedWriteAsync(file, cancellationToken);
            state.Opened = true;
            cancellationToken.ThrowIfCancellationRequested();
            if (verifyOriginal != null)
            {
                using var original = transaction.Stream.GetInputStreamAt(0);
                await verifyOriginal(original);
            }
            stream = transaction.Stream.AsStreamForWrite();
            stream.Position = 0;
            stream.SetLength(0);
            await writeAsync(stream);
            cancellationToken.ThrowIfCancellationRequested();
            await stream.FlushAsync(CancellationToken.None);
            cancellationToken.ThrowIfCancellationRequested();
            // Once commit starts, finish it and publish the written revision.
            // Cancellation cannot report a successfully replaced file as failed.
            state.Committing = true;
            await FileStorage.CommitAsync(file, transaction);
            state.Committed = true;
        }
        catch (Exception ex)
        {
            writeFailure = ex;
            throw;
        }
        finally
        {
            // Close both owners even if the first close fails. A committed
            // manifest must never be mistaken for a failed generation and
            // have its referenced assets rolled back by its caller.
            Exception closeFailure = null;
            try { stream?.Dispose(); }
            catch (Exception ex) { closeFailure = ex; }
            try { transaction?.Dispose(); }
            catch (Exception ex)
            {
                if (closeFailure == null) closeFailure = ex;
                else LoggingService.LogError($"[{nameof(AtomicFileWriter)}] Additional transaction cleanup failure: {ex}");
            }
            if (closeFailure != null)
            {
                if (state.Committed || writeFailure != null)
                    LoggingService.LogError($"[{nameof(AtomicFileWriter)}] Transaction cleanup failed after {(state.Committed ? "commit" : "write failure")}: {closeFailure}");
                else throw closeFailure;
            }
        }
    }
}
