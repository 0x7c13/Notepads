// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents.Contracts;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Infrastructure.Storage;
using Windows.Storage;
using Windows.Storage.Provider;

namespace Notepads.Features.Documents.IO;

internal sealed class DocumentFileWriteResult
{
    internal DocumentFileWriteResult(StorageFile file) { File = file; }

    public StorageFile File { get; private set; }
    public DocumentFileVersion Version { get; internal set; } = DocumentFileVersion.Unknown;
    private byte[] _writtenHash;
    private long _writtenLength;
    private string _path;
    private string _name;
    private string _targetPath;
    private string _admittedSpelling;
    internal string TargetPath => _targetPath;
    private bool _awaitingProviderRename;

    // WinRT can refresh the grant's leaf casing both at commit and when reopening it.
    private bool HasRecordedSpelling => File.Path == _path && File.Name == _name;
    private ValueTask<bool> AtRecordedLocationAsync() => HasRecordedSpelling ? ValueTask.FromResult(true) :
        DocumentSaveTarget.IsGrantSpellingChangeAsync(File, _path, _name, _targetPath, _admittedSpelling);
    internal async ValueTask<bool> CanFollowProviderGrantAsync() => UpdateStatus == FileUpdateStatus.CompleteAndRenamed &&
        (_awaitingProviderRename || await AtRecordedLocationAsync());

    internal ValueTask<bool> ObserveLocationAsync()
    {
        if (!_awaitingProviderRename) return AtRecordedLocationAsync();
        if (HasRecordedSpelling) return ValueTask.FromResult(false);
        FollowRename(File);
        return ValueTask.FromResult(true);
    }

    internal async ValueTask RecordCommitAsync(long length, byte[] hash, string originalPath, string originalName,
        string targetPath, string admittedSpelling)
    {
        _writtenLength = length;
        _writtenHash = hash;
        _targetPath = targetPath;
        _admittedSpelling = admittedSpelling;
        _path = originalPath;
        _name = originalName;
        if (UpdateStatus == FileUpdateStatus.CompleteAndRenamed)
        {
            // The original directory entry is no longer proof of the provider's new location.
            _targetPath = null;
            _admittedSpelling = null;
            _awaitingProviderRename = HasRecordedSpelling;
            if (!_awaitingProviderRename) FollowRename(File);
        }
        else if (!await AtRecordedLocationAsync()) throw new DocumentSaveException(DocumentSaveStage.Superseded);
    }

    // Only a rename acknowledged by this provider or performed by this editor may
    // move the receipt. A coincidental byte match at an abandoned name is insufficient.
    internal void FollowRename(StorageFile renamed, string targetPath = null)
    {
        File = renamed;
        _targetPath = targetPath;
        _admittedSpelling = null;
        _path = renamed.Path;
        _name = renamed.Name;
        _awaitingProviderRename = false;
    }

    internal async Task<DocumentFileVersion> VerifyAsync(CancellationToken cancellation)
    {
        if (!await ObserveLocationAsync())
        {
            if (_awaitingProviderRename) return DocumentFileVersion.Unknown;
            throw new DocumentSaveException(DocumentSaveStage.Superseded);
        }
        try
        {
            if (_targetPath == null && UpdateStatus == FileUpdateStatus.CompleteAndRenamed)
            {
                try { _targetPath = (await DocumentSaveTarget.ResolveAsync(File, cancellation)).FinalPath; }
                catch (OperationCanceledException) when (cancellation.IsCancellationRequested) { return DocumentFileVersion.Unknown; }
                catch (DocumentSaveException) { return DocumentFileVersion.Unknown; }
            }
            var observed = await DocumentFileWriter.VerifyCommittedVersionAsync(File, _writtenLength, _writtenHash, cancellation);
            if (!await AtRecordedLocationAsync()) throw new DocumentSaveException(DocumentSaveStage.Superseded);
            if (observed.IsVerified)
            {
                if (_admittedSpelling == null || _admittedSpelling != observed.TargetPath)
                    await DocumentSaveTarget.RequireLocationAsync(_targetPath, observed.TargetPath, DocumentSaveStage.Superseded);
                _targetPath = observed.TargetPath;
                _admittedSpelling = null;
            }
            return observed;
        }
        catch (DocumentSaveException ex) when (ex.Stage == DocumentSaveStage.Superseded)
        {
            // The provider's known destination remains authoritative even if the bytes were superseded.
            ex.CommittedWrite = this;
            throw;
        }
    }

    internal bool HasAdmittedSpelling(DocumentSaveTarget.Identity current) => ReferenceEquals(File, current.File) &&
        _admittedSpelling != null && _admittedSpelling == current.FinalPath;
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

internal static partial class DocumentFileWriter
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
        CancellationToken cancellationToken = default, DocumentFileVersion? expectedVersion = null, DocumentSaveTarget.Identity target = null)
    {
        var updates = CachedUpdates;
        OnSelectUpdates(ref updates);
        return WriteAsync(file, writeCandidate, updates, cancellationToken, expectedVersion, target);
    }

    internal static async Task<DocumentFileWriteResult> WriteAsync(StorageFile file, Func<Stream, Task> writeCandidate,
        IDocumentFileUpdates updates, CancellationToken cancellationToken = default, DocumentFileVersion? expectedVersion = null,
        DocumentSaveTarget.Identity target = null)
    {
        using var measurement = OperationMetrics.Measure("document.save");
        ArgumentNullException.ThrowIfNull(file);
        ArgumentNullException.ThrowIfNull(writeCandidate);
        ArgumentNullException.ThrowIfNull(updates);
        cancellationToken.ThrowIfCancellationRequested();
        if (expectedVersion is { IsVerified: false }) throw new DocumentSaveException(DocumentSaveStage.Unverified);
        target ??= await DocumentSaveTarget.ResolveAsync(file, cancellationToken);
        await DocumentSaveTarget.RequireLocationAsync(expectedVersion?.TargetPath, target.FinalPath);
        var originalPath = file.Path;
        var originalName = file.Name;
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
                byte[] writtenHash = null;
                long writtenLength = 0;
                string admittedSpelling = null;
                try
                {
                    // Keep the original provider grant for the transaction. Pin mutable local links only for
                    // this attempt, and release them before provider completion (which can rename the file).
                    using var route = await Task.Run(() => DocumentSaveTarget.Pin(currentFile, target.FinalPath, out admittedSpelling), cancellationToken);
                    await AtomicFileWriter.WriteAsync(currentFile, async destination =>
                    {
                        using var output = new DocumentSaveOutputStream(destination);
                        await writeCandidate(output);
                        if (target.FinalPath != null)
                        {
                            var observed = (await DocumentSaveTarget.ResolveAsync(currentFile, cancellationToken)).FinalPath;
                            OnTransactionLocationObserved(ref observed);
                            await DocumentSaveTarget.RequireLocationAsync(target.FinalPath, observed);
                            if (observed != target.FinalPath) admittedSpelling = null;
                        }
                        writtenHash = output.FinishHash();
                        writtenLength = output.Length;
                    }, state, cancellationToken, verifyOriginal: expectedVersion is { } version
                        ? original => DocumentSaveTarget.VerifyAsync(currentFile, version, original, cancellationToken) : null);
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
                        if (attempt >= MaximumAttempts || !CanRetry(ex))
                        {
                            if (ex is DocumentSaveException || !DocumentSaveException.IsStorageError(ex)) throw;
                            var stage = !state.Opened ? DocumentSaveStage.OpenTarget :
                                state.Committing ? DocumentSaveStage.CommitTarget : DocumentSaveStage.WriteTarget;
                            throw new DocumentSaveException(stage, ex);
                        }
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
                                // Provider rename observation starts from this replacement grant.
                                originalPath = currentFile.Path;
                                originalName = currentFile.Name;
                                deferred = TryDeferUpdates(updates, currentFile);
                            }
                        }
                        await Task.Delay(10, cancellationToken);
                        continue;
                    }
                }

                // Cancellation belongs to candidate preparation. Once commit
                // succeeds, finish provider accounting and check which bytes survived.
                OnLocalCommit(currentFile);
                var completeCommitted = deferred;
                deferred = false;
                if (completeCommitted) await CompleteUpdatesAsync(updates, currentFile, result);
                await result.RecordCommitAsync(writtenLength, writtenHash, originalPath, originalName, target.FinalPath, admittedSpelling);
                if (!cancellationToken.IsCancellationRequested)
                    result.Version = await result.VerifyAsync(cancellationToken);
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

    static partial void OnSelectUpdates(ref IDocumentFileUpdates updates);
    static partial void OnTransactionLocationObserved(ref string observed);

    // Test builds can interleave a writer at the instant CommitAsync releases exclusivity.
    static partial void OnLocalCommit(StorageFile file);

    internal static async Task<DocumentFileVersion> VerifyCommittedVersionAsync(StorageFile file, long length,
        byte[] hash, CancellationToken cancellationToken)
    {
        try
        {
            // CommitAsync releases writer exclusivity before it returns. Attribute a
            // disk timestamp to our snapshot only while a reader pins matching bytes.
            for (int attempt = 1; ; attempt++)
            {
                try
                {
                    var observed = await DocumentSaveTarget.ReadVersionAsync(file, cancellationToken, length);
                    if (observed.Sha256 == Convert.ToHexString(hash)) return observed;
                    throw new DocumentSaveException(DocumentSaveStage.Superseded);
                }
                catch (DocumentSaveException ex) when (attempt < 3 && ex.Stage == DocumentSaveStage.VerifyVersion &&
                    ex.HResult is SharingViolation or AccessDenied)
                {
                    // Only retry observation. Replaying the committed write could destroy a later edit.
                    await Task.Delay(25 * attempt, cancellationToken);
                }
            }
        }
        catch (DocumentSaveException ex) when (ex.Stage == DocumentSaveStage.Superseded) { throw; }
        catch (DocumentSaveException ex) when (ex.HResult is unchecked((int)0x80070002) or unchecked((int)0x80070003))
        {
            throw new DocumentSaveException(DocumentSaveStage.Superseded, ex);
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested) { }
        catch (Exception ex)
        {
            LoggingService.LogError($"[{nameof(DocumentFileWriter)}] Committed bytes could not be verified: {ex}");
        }
        return DocumentFileVersion.Unknown;
    }

    private static bool CanRetry(Exception error)
    {
        return error is not OperationCanceledException and not DocumentSaveException { Stage: DocumentSaveStage.PrepareSnapshot } &&
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
            return await DocumentSaveTarget.IsSameFileAsync(file, resolved, cancellationToken) ? resolved : null;
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
        using var measurement = OperationMetrics.Measure("document.save.provider-completion");
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
        public Task<FileUpdateStatus> CompleteAsync(StorageFile file) => FileStorage.CompleteUpdatesAsync(file);
    }
}
