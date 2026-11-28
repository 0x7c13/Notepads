// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.IO;
using System.Runtime.CompilerServices;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Infrastructure.Threading;
using Windows.Storage;
using Windows.Storage.FileProperties;
using Windows.Storage.Provider;
using Windows.Storage.Streams;

namespace Notepads.Infrastructure.Storage;

public static class FileStorage
{
    private static readonly ConditionalWeakTable<StorageFile, SemaphoreSlim> ItemOperations = new();

    private static async Task<T> RunItemOperationAsync<T>(StorageFile file, Func<Task<T>> operation,
        CancellationToken cancellation)
    {
        // GetBasicPropertiesAsync rejects overlapping async calls on the same
        // IStorageItem. Hold this gate through native completion, never through
        // the returned stream's lifetime. Other grants remain independent.
        var gate = ItemOperations.GetValue(file, static _ => new SemaphoreSlim(1, 1));
        await gate.WaitAsync(cancellation).ConfigureAwait(false);
        try
        {
            cancellation.ThrowIfCancellationRequested();
            return await operation().ConfigureAwait(false);
        }
        finally { gate.Release(); }
    }

    internal static Task<BasicProperties> GetFilePropertiesAsync(StorageFile file, CancellationToken cancellation = default) =>
        RunItemOperationAsync(file, () => file.GetBasicPropertiesAsync().AsCompletionTask(cancellation), cancellation);

    internal static Task AppendLinesAsync(StorageFile file, IEnumerable<string> lines, CancellationToken cancellation = default) =>
        RunItemOperationAsync(file, async () =>
        {
            await FileIO.AppendLinesAsync(file, lines).AsCompletionTask(cancellation).ConfigureAwait(false);
            return true;
        }, cancellation);

    internal static Task<IRandomAccessStream> OpenReadAsync(StorageFile file, StorageOpenOptions options = StorageOpenOptions.None,
        CancellationToken cancellation = default) =>
        RunItemOperationAsync(file, () => file.OpenAsync(FileAccessMode.Read, options).AsCompletionTask(cancellation), cancellation);

    internal static async Task<Stream> OpenStreamForReadAsync(StorageFile file, CancellationToken cancellation = default)
    {
        var stream = await OpenReadAsync(file, cancellation: cancellation).ConfigureAwait(false);
        try { return stream.AsStreamForRead(); }
        catch { stream.Dispose(); throw; }
    }

    internal static Task<StorageStreamTransaction> OpenTransactedWriteAsync(StorageFile file, CancellationToken cancellation = default) =>
        RunItemOperationAsync(file, () => file.OpenTransactedWriteAsync().AsCompletionTask(cancellation), cancellation);

    internal static Task RenameAsync(StorageFile file, string name, CancellationToken cancellation = default) =>
        RunItemOperationAsync(file, async () =>
        {
            await file.RenameAsync(name).AsCompletionTask(cancellation).ConfigureAwait(false);
            return true;
        }, cancellation);

    internal static Task CommitAsync(StorageFile file, StorageStreamTransaction transaction) =>
        RunItemOperationAsync(file, async () =>
        {
            await transaction.CommitAsync().AsCompletionTask(CancellationToken.None).ConfigureAwait(false);
            return true;
        }, CancellationToken.None);

    internal static Task<FileUpdateStatus> CompleteUpdatesAsync(StorageFile file) =>
        RunItemOperationAsync(file, () => CachedFileManager.CompleteUpdatesAsync(file).AsCompletionTask(CancellationToken.None), CancellationToken.None);

    internal static Task<StorageFolder> GetParentAsync(StorageFile file, CancellationToken cancellation = default) =>
        RunItemOperationAsync(file, () => file.GetParentAsync().AsCompletionTask(cancellation), cancellation);

    public static async Task<long> GetDateModifiedAsync(StorageFile file, CancellationToken cancellation = default)
    {
        var properties = await GetFilePropertiesAsync(file, cancellation);
        var dateModified = properties.DateModified;
        return dateModified.ToFileTime();
    }

    public static bool IsFileReadOnly(StorageFile file)
    {
        return (file.Attributes & Windows.Storage.FileAttributes.ReadOnly) != 0;
    }

    public static async Task<StorageFile> GetFileAsync(string filePath)
    {
        try
        {
            return await StorageFile.GetFileFromPathAsync(filePath);
        }
        catch
        {
            return null;
        }
    }

    public static async Task<bool> FileExistsAsync(StorageFile file, CancellationToken cancellation = default)
    {
        try
        {
            using var stream = await OpenStreamForReadAsync(file, cancellation);
            return true;
        }
        catch (OperationCanceledException) when (cancellation.IsCancellationRequested) { throw; }
        catch (FileNotFoundException)
        {
            return false;
        }
        catch (Exception ex)
        {
            LoggingService.LogError($"[{nameof(FileStorage)}] Failed to check if file [{file.Path}] exists: {ex.Message}", consoleOnly: true);
            return true;
        }
    }
}
