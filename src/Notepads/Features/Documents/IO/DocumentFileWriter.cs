// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Infrastructure.Storage;
using Windows.Storage;
using Windows.Storage.Provider;

namespace Notepads.Features.Documents.IO;

internal sealed class DocumentFileWriteResult
{
    internal DocumentFileWriteResult(StorageFile file) { File = file; }

    public StorageFile File { get; }
    public FileUpdateStatus? UpdateStatus { get; private set; }
    public string ProviderError { get; private set; }

    internal void CompleteProviderUpdate(FileUpdateStatus? status, string error)
    {
        UpdateStatus = status;
        ProviderError = error;
    }
}

/// <summary>Provider update ownership is separate from the atomic local file commit.</summary>
internal interface IDocumentFileUpdates
{
    void Defer(StorageFile file);
    Task<FileUpdateStatus> CompleteAsync(StorageFile file);
}

internal static class DocumentFileWriter
{
    private const int MaximumAttempts = 5;
    private const int AccessDenied = unchecked((int)0x80070005);
    private const int SharingViolation = unchecked((int)0x80070020);
    private const int UnableToRemoveReplaced = unchecked((int)0x80070497);
    private const int GenericFailure = unchecked((int)0x80004005);
    private static readonly IDocumentFileUpdates CachedUpdates = new CachedDocumentFileUpdates();

    /// <summary>
    /// The callback owns candidate preparation and can run again before commit.
    /// It must reopen its forward reader over the same retained document revision.
    /// A returned result always represents a completed local atomic commit.
    /// </summary>
    public static Task<DocumentFileWriteResult> WriteAsync(StorageFile file, Func<Stream, Task> writeCandidate,
        CancellationToken cancellationToken = default)
    {
        return WriteAsync(file, writeCandidate, CachedUpdates, cancellationToken);
    }

    internal static async Task<DocumentFileWriteResult> WriteAsync(StorageFile file, Func<Stream, Task> writeCandidate,
        IDocumentFileUpdates updates, CancellationToken cancellationToken = default)
    {
        using var measurement = OperationMetrics.Measure("document.save");
        if (file == null) throw new ArgumentNullException(nameof(file));
        if (writeCandidate == null) throw new ArgumentNullException(nameof(writeCandidate));
        if (updates == null) throw new ArgumentNullException(nameof(updates));
        cancellationToken.ThrowIfCancellationRequested();
        var currentFile = file;
        var deferred = TryDeferUpdates(updates, currentFile);
        var resolvedPath = false;
        try
        {
            for (var attempt = 1; ; attempt++)
            {
                cancellationToken.ThrowIfCancellationRequested();
                // Allocate the publication object before committing the user file.
                var result = new DocumentFileWriteResult(currentFile);
                var state = new AtomicFileWriteState();
                try
                {
                    await AtomicFileWriter.WriteAsync(currentFile, writeCandidate, state, cancellationToken);
                }
                catch (Exception ex)
                {
                    if (state.Committed)
                    {
                        // A stream/transaction close failure must not rewrite a
                        // revision whose destination already committed successfully.
                        LoggingService.LogError($"[{nameof(DocumentFileWriter)}] File committed; transaction cleanup failed: {ex}");
                    }
                    else
                    {
                        if (attempt >= MaximumAttempts || !CanRetry(ex)) throw;
                        if (ex.HResult == AccessDenied && !state.Opened && !resolvedPath)
                        {
                            resolvedPath = true;
                            var resolved = await TryResolveWritableIdentityAsync(currentFile, cancellationToken);
                            if (resolved != null)
                            {
                                // Release the original provider deferral before
                                // taking ownership of the newly resolved identity.
                                var completePrevious = deferred;
                                deferred = false;
                                if (completePrevious) await CompleteUpdatesAsync(updates, currentFile, null);
                                currentFile = resolved;
                                deferred = TryDeferUpdates(updates, currentFile);
                            }
                        }
                        await Task.Delay(10, cancellationToken);
                        continue;
                    }
                }

                // Cancellation belongs to candidate preparation. Once commit
                // succeeds, finish provider accounting and return that success.
                var completeCommitted = deferred;
                deferred = false;
                if (completeCommitted) await CompleteUpdatesAsync(updates, currentFile, result);
                return result;
            }
        }
        finally
        {
            // Balance a deferral even when preparation, commit or cancellation
            // fails. Provider failures never hide the original write exception.
            if (deferred) await CompleteUpdatesAsync(updates, currentFile, null);
        }
    }

    private static bool CanRetry(Exception error)
    {
        return error is not OperationCanceledException &&
            (error.HResult == AccessDenied || error.HResult == SharingViolation ||
             error.HResult == UnableToRemoveReplaced || error.HResult == GenericFailure);
    }

    private static async Task<StorageFile> TryResolveWritableIdentityAsync(StorageFile file,
        CancellationToken cancellationToken)
    {
        if (string.IsNullOrEmpty(file.Path)) return null;
        try
        {
            var resolved = await StorageFile.GetFileFromPathAsync(file.Path);
            cancellationToken.ThrowIfCancellationRequested();
            // Reuse only the existing access grant and the same storage item.
            // Genuine read-only permissions still fail the next transaction.
            return resolved.IsEqual(file) ? resolved : null;
        }
        catch (OperationCanceledException) { throw; }
        catch (Exception) { return null; }
    }

    private static bool TryDeferUpdates(IDocumentFileUpdates updates, StorageFile file)
    {
        try
        {
            updates.Defer(file);
            return true;
        }
        catch (Exception ex)
        {
            // Match existing provider policy: an unavailable deferral does
            // not prevent an otherwise writable local file from being saved.
            LoggingService.LogError($"[{nameof(DocumentFileWriter)}] Could not defer provider updates: {ex}");
            return false;
        }
    }

    private static async Task CompleteUpdatesAsync(IDocumentFileUpdates updates, StorageFile file,
        DocumentFileWriteResult result)
    {
        try
        {
            var status = await updates.CompleteAsync(file);
            var error = status == FileUpdateStatus.Complete || status == FileUpdateStatus.CompleteAndRenamed ?
                null : "The file provider has not completed its update: " + status;
            result?.CompleteProviderUpdate(status, error);
            if (error != null) LoggingService.LogError($"[{nameof(DocumentFileWriter)}] {error}");
        }
        catch (Exception ex)
        {
            result?.CompleteProviderUpdate(FileUpdateStatus.Incomplete, ex.Message);
            LoggingService.LogError($"[{nameof(DocumentFileWriter)}] Provider update did not complete: {ex}");
        }
    }

    private sealed class CachedDocumentFileUpdates : IDocumentFileUpdates
    {
        public void Defer(StorageFile file) => CachedFileManager.DeferUpdates(file);
        public async Task<FileUpdateStatus> CompleteAsync(StorageFile file) => await CachedFileManager.CompleteUpdatesAsync(file);
    }
}
