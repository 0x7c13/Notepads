// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Documents.IO;
using Notepads.Features.Documents.Storage;
using Notepads.Features.Documents.Text;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Infrastructure.Storage;
using Notepads.Presentation.Workspace;
using Windows.Storage;
using Windows.Storage.Provider;
using WinUIEditor;

namespace Notepads.Presentation.Controls.TextEditor;

public sealed partial class TextEditor
{
    private sealed partial record SaveRevision(DocumentBaseline Baseline, long Version, ulong Sequence,
        Encoding Encoding, LineEnding LineEnding) : IDisposable
    {
        internal SaveRevision Retain() => this with { Baseline = Baseline.Retain() };
        public void Dispose() => Baseline.Dispose();

        internal async Task WriteAsync(Stream destination, CancellationToken cancellation)
        {
            try
            {
                using var source = await Baseline.OpenReadStreamAsync(cancellation).ConfigureAwait(false);
                await DocumentTextCodec.EncodeAsync(source, destination, Encoding, LineEnding, cancellation).ConfigureAwait(false);
            }
            catch (Exception ex) when (ex is not DocumentSaveException && DocumentSaveException.IsStorageError(ex))
            {
                throw new DocumentSaveException(DocumentSaveStage.PrepareSnapshot, ex);
            }
        }
    }

    private enum SaveTargetRelation { Different, Bound, Unknown }

    private abstract record SaveRequest(StorageFile File);
    private sealed record InPlaceSave(StorageFile File) : SaveRequest(File);
    private sealed record PickedSave(StorageFile File, DocumentFileVersion Version) : SaveRequest(File);
    private sealed record ConfirmSave(Guid Id) : SaveRequest((StorageFile)null);

    private sealed record PendingFileSave(Guid Id, DocumentFileWriteResult Result, DocumentSaveTarget.Identity Target,
        SaveRevision Revision, SaveTargetRelation Relation)
    {
        internal bool WasBound => Relation == SaveTargetRelation.Bound;
    }

    private readonly record struct SaveAdmission(DocumentSaveTarget.Identity Target, SaveTargetRelation Relation,
        DocumentFileVersion ExpectedVersion, bool MatchesReceipt, bool ResumeReceipt);

    private PendingFileSave _pendingFileSave;
    private SaveTargetRelation _activeSaveTarget;
    internal bool IsFileSaveActive { get; private set; }

    private void ClearPendingFileSave()
    {
        _pendingFileSave?.Revision.Dispose();
        _pendingFileSave = null;
    }

    partial void OnSaveReaderCaptured(ref Task observation);
    partial void OnResolvingSaveTarget(ref Task observation);

    // Pin the revision on the editor apartment. The destination and any existing
    // commit receipt can then be checked before creating a private baseline.
    private sealed partial class SaveRevisionCapture : IDisposable
    {
        internal readonly long Version;
        internal readonly ulong Sequence;
        internal readonly Encoding Encoding;
        internal readonly LineEnding LineEnding;
        internal readonly DocumentBaseline ExistingBaseline;
        internal readonly EditorUtf8Reader Reader;
        internal readonly EditorJournalCheckpoint Checkpoint;
        private bool _disposed;

        internal SaveRevisionCapture(TextEditor editor, CancellationToken cancellation)
        {
            cancellation.ThrowIfCancellationRequested();
            var core = editor.TextEditorCore;
            Version = core.ContentVersion;
            Sequence = core.DocumentSequence;
            Encoding = editor.GetEncoding();
            LineEnding = editor.GetLineEnding();
            try
            {
                if (Version == editor._publishedContentVersion || (editor.HasCurrentContentComparison && editor._comparedEqual))
                    ExistingBaseline = editor.LastSavedSnapshot.Baseline.Retain();
                else if (core.DocumentLength == 0)
                    ExistingBaseline = DocumentBaseline.CreateEmpty(editor.DocumentOwnerId);
                else
                {
                    Checkpoint = editor.TryCaptureSaveCheckpoint(Sequence, core.DocumentLength);
                    Reader = core.AcquireSaveReader();
                }
            }
            catch (Exception ex)
            {
                Dispose();
                if (IsSnapshotPreparationError(ex, cancellation)) throw new DocumentSaveException(DocumentSaveStage.PrepareSnapshot, ex);
                throw;
            }
        }

        public void Dispose()
        {
            if (_disposed) return;
            _disposed = true;
            Reader?.Dispose();
            Checkpoint?.Dispose();
            ExistingBaseline?.Dispose();
        }
    }

    private async Task<SaveRevision> CaptureSaveRevisionAsync(CancellationToken cancellation)
    {
        using var capture = new SaveRevisionCapture(this, cancellation);
        return await MaterializeSaveRevisionAsync(capture, cancellation);
    }

    private async Task<SaveRevision> MaterializeSaveRevisionAsync(SaveRevisionCapture capture, CancellationToken cancellation)
    {
        if (capture.ExistingBaseline != null)
            return new SaveRevision(capture.ExistingBaseline.Retain(), capture.Version, capture.Sequence, capture.Encoding, capture.LineEnding);
        using var measurement = OperationMetrics.Measure("document.save.capture");
        try
        {
            DocumentBaseline baseline;
            try
            {
                using var source = new EditorDocumentStream(capture.Reader, ownsReader: false);
                Task observation = null;
                OnSaveReaderCaptured(ref observation);
                if (observation != null) await observation.WaitAsync(cancellation);
                baseline = await DocumentBaseline.CreateAsync(output => source.CopyToAsync(output, 65536, cancellation),
                    DocumentOwnerId, cancellation);
            }
            catch (Exception ex) when (!cancellation.IsCancellationRequested && IsSnapshotInvalidated(ex))
            {
                // A large deletion or rollback can invalidate this cursor. Replay
                // only the original prefix. A later edit belongs to a separate save.
                if (capture.Checkpoint == null) throw new DocumentSaveException(DocumentSaveStage.PrepareSnapshot, ex);
                baseline = await CreateCheckpointBaselineAsync(_recoveryBaseline, capture.Checkpoint, cancellation);
            }
            return new SaveRevision(baseline, capture.Version, capture.Sequence, capture.Encoding, capture.LineEnding);
        }
        catch (Exception ex) when (IsSnapshotPreparationError(ex, cancellation))
        {
            throw new DocumentSaveException(DocumentSaveStage.PrepareSnapshot, ex);
        }
    }

    private static bool IsSnapshotPreparationError(Exception error, CancellationToken cancellation) =>
        error is not DocumentSaveException && !cancellation.IsCancellationRequested &&
        (DocumentSaveException.IsStorageError(error) || error is ArgumentException or InvalidOperationException);

    private static bool IsSnapshotInvalidated(Exception error) =>
        error.HResult is unchecked((int)0x80070718) or unchecked((int)0x800704D5);

    private EditorJournalCheckpoint TryCaptureSaveCheckpoint(ulong sequence, long length)
    {
        if (length <= 65536 || (ulong)length > MaximumReplayBytes || _recoveryBaseline == null ||
            (ulong)_recoveryBaseline.ByteLength > MaximumReplayBytes || _documentJournal == null || TextEditorCore.IsJournalFaulted)
            return null;
        try
        {
            var checkpoint = TextEditorCore.AcquireJournalCheckpoint();
            if (checkpoint.CommittedSequence == sequence && checkpoint.CommittedDocumentByteLength == (ulong)length)
                return checkpoint;
            checkpoint.Dispose();
        }
        catch (Exception ex) when (DocumentSaveException.IsStorageError(ex) || ex is InvalidOperationException)
        {
            // A damaged journal or an open edit group has no usable recovery prefix.
        }
        return null;
    }

    public Task<DocumentFileVersion> ReadPickedFileVersionAsync(StorageFile file, CancellationToken cancellation) =>
        _documentOperations.ObserveAsync(token => DocumentSaveTarget.ReadVersionAsync(file, token), cancellation);

    public Task<StorageFile> SaveContentToFileAndUpdateEditorStateAsync(StorageFile file, DocumentFileVersion? pickedFileVersion = null,
        CancellationToken cancellation = default, Guid? verificationId = null)
    {
        // Convert the existing caller API once, before queueing. Admission keeps
        // this intent even if another save changes the file binding in the meantime.
        SaveRequest request = verificationId is { } id ? new ConfirmSave(id) :
            pickedFileVersion is { } version ? new PickedSave(file, version) : new InPlaceSave(file);
        return SaveContentToFileAndUpdateEditorStateAsync(request, cancellation);
    }

    private async Task<StorageFile> SaveContentToFileAndUpdateEditorStateAsync(SaveRequest request, CancellationToken cancellation)
    {
        if (request is not ConfirmSave) ArgumentNullException.ThrowIfNull(request.File);
        var originalBinding = EditingFile;
        StorageFile committedFile = null;
        await RunDocumentOperationAsync(async token =>
        {
            IsFileSaveActive = true;
            try
            {
                if (Mode == TextEditorMode.DiffPreview) CloseSideBySideDiffViewer();
                var pending = _pendingFileSave;
                if (request is ConfirmSave confirmation && pending?.Id != confirmation.Id)
                    throw new DocumentSaveException(DocumentSaveStage.Conflict);
                bool reusePendingRevision = pending != null && (request is ConfirmSave ||
                    (TextEditorCore.ContentVersion == pending.Revision.Version &&
                    EncodingCatalog.Equals(GetEncoding(), pending.Revision.Encoding) && GetLineEnding() == pending.Revision.LineEnding));
                // Preserve the editor's text in a checkpoint before looking up an
                // alternate name. Only a known bound write can mark that file unconfirmed.
                if (request is not ConfirmSave && EditingFile != null)
                    _activeSaveTarget = ReferenceEquals(EditingFile, request.File) ? SaveTargetRelation.Bound : SaveTargetRelation.Unknown;
                using var capture = reusePendingRevision ? null : new SaveRevisionCapture(this, token);
                if (!await DocumentSaveTarget.IsSameFileAsync(EditingFile, originalBinding, token))
                    throw new DocumentSaveException(DocumentSaveStage.Conflict);
                committedFile = await SaveAndUpdateEditorStateAsync(request, token, capture);
            }
            finally
            {
                _activeSaveTarget = SaveTargetRelation.Different;
                IsFileSaveActive = false;
            }
        }, cancellation);
        return committedFile;
    }

    private async Task<SaveAdmission> ResolveSaveAdmissionAsync(SaveRequest request, StorageFile file,
        PendingFileSave pending, CancellationToken cancellation)
    {
        Task observation = null;
        OnResolvingSaveTarget(ref observation);
        if (observation != null) await observation.WaitAsync(cancellation);
        var target = await DocumentSaveTarget.ResolveAsync(file, cancellation);
        var bound = ReferenceEquals(EditingFile, file) ? target : await DocumentSaveTarget.ResolveAsync(EditingFile, cancellation);
        var relation = (await target.MatchesAsync(bound)) switch
        {
            true => SaveTargetRelation.Bound,
            false => SaveTargetRelation.Different,
            null => SaveTargetRelation.Unknown
        };
        if (request is not ConfirmSave) _activeSaveTarget = relation;
        var committedTarget = pending?.Target;
        if (pending != null && committedTarget == null)
            committedTarget = ReferenceEquals(file, pending.Result.File) ? target :
                await DocumentSaveTarget.ResolveAsync(pending.Result.File, cancellation);
        var matchesReceipt = pending != null && (pending.Result.HasAdmittedSpelling(target) ||
            await target.MatchesAsync(committedTarget) == true);
        var resumeReceipt = matchesReceipt && (request switch
        {
            ConfirmSave => true, // Its exact receipt ID was checked before capture.
            InPlaceSave => pending.WasBound,
            _ => false
        });
        var expectedVersion = request is PickedSave pickedRequest ? pickedRequest.Version : LastSavedSnapshot.FileVersion;
        if (!resumeReceipt && request is not ConfirmSave)
        {
            if (request is InPlaceSave)
            {
                // Only an explicit picker choice can authorize a different target.
                // A pending independent copy cannot grant that authority to Ctrl+S.
                if (relation != SaveTargetRelation.Bound)
                    throw new DocumentSaveException(DocumentSaveStage.Conflict);
                if (HasUnconfirmedFileSave)
                    throw new DocumentSaveException(DocumentSaveStage.Unverified);
            }
            if (!expectedVersion.IsVerified)
                throw new DocumentSaveException(DocumentSaveStage.Unverified);
        }
        return new SaveAdmission(target, relation, expectedVersion, matchesReceipt, resumeReceipt);
    }

    private async Task<(DocumentFileVersion Version, StorageFile RenamedBinding)> ConfirmPendingSaveAsync(PendingFileSave pending,
        DocumentSaveTarget.Identity target, bool confirmsRequestedRevision, CancellationToken cancellation)
    {
        using var lease = await DocumentSaveTarget.AcquireAsync(target, cancellation);
        var observed = await pending.Result.VerifyAsync(cancellation);
        var renamedBinding = pending.WasBound ? await RefreshProviderRenameAsync(pending.Result) : null;
        cancellation.ThrowIfCancellationRequested();
        if (!observed.IsVerified)
            throw new DocumentSaveException(confirmsRequestedRevision ? DocumentSaveStage.ConfirmCommit : DocumentSaveStage.VerifyVersion)
            {
                VerificationId = confirmsRequestedRevision ? pending.Id : null,
                CommittedWrite = pending.Result, RenamedBinding = renamedBinding
            };
        return (observed, renamedBinding);
    }

    private async Task<StorageFile> SaveAndUpdateEditorStateAsync(SaveRequest request, CancellationToken cancellation,
        SaveRevisionCapture capture)
    {
        var pending = _pendingFileSave;
        // A confirmation uses the grant that actually committed, including a provider rename.
        var file = request is ConfirmSave ? pending.Result.File : request.File;
        var admission = await ResolveSaveAdmissionAsync(request, file, pending, cancellation);
        var (target, relation, expectedVersion, matchesReceipt, resumeReceipt) = admission;
        StorageFile renamedBinding = null;
        try
        {
            if (request is ConfirmSave && !resumeReceipt)
                throw new DocumentSaveException(DocumentSaveStage.Superseded);
            if (resumeReceipt)
            {
                if (pending.Result.UpdateStatus != FileUpdateStatus.CompleteAndRenamed &&
                    DocumentSaveTarget.RequiresLocationConfirmation(pending.Result.TargetPath, target.FinalPath))
                    throw new DocumentSaveException(DocumentSaveStage.Unverified);
                // Confirming an older revision is preparation for this request, not its
                // successful commit. Retry must still save the newer captured revision.
                (expectedVersion, renamedBinding) = await ConfirmPendingSaveAsync(pending, target, capture == null, cancellation);
                if (capture == null)
                {
                    pending.Result.Version = expectedVersion;
                    await PublishSavedFileAsync(pending.Result, pending.Revision);
                    return pending.Result.File;
                }

                if (pending.WasBound && request is InPlaceSave)
                {
                    file = pending.Result.File;
                    relation = _activeSaveTarget = SaveTargetRelation.Bound;
                }
                target = await DocumentSaveTarget.ResolveAsync(file, cancellation);
            }
            else
            {
                await DocumentSaveTarget.RequireLocationAsync(expectedVersion.TargetPath, target.FinalPath);
                if (request is InPlaceSave && DocumentSaveTarget.RequiresLocationConfirmation(expectedVersion.TargetPath, target.FinalPath))
                    throw new DocumentSaveException(DocumentSaveStage.Unverified);
            }

            using var revision = capture == null ? pending.Revision.Retain() : await MaterializeSaveRevisionAsync(capture, cancellation);
            capture?.Dispose();
            DocumentFileWriteResult result;
            using (await DocumentSaveTarget.AcquireAsync(target, cancellation))
            {
                result = await DocumentFileWriter.WriteAsync(file, destination => revision.WriteAsync(destination, cancellation),
                    cancellation, expectedVersion, target);
            }
            if (_disposed) return result.File;
            ClearPendingFileSave();
            if (!result.Version.IsVerified)
            {
                var confirmedTarget = result.UpdateStatus == FileUpdateStatus.CompleteAndRenamed ? null : target;
                _pendingFileSave = new PendingFileSave(Guid.NewGuid(), result, confirmedTarget, revision.Retain(), relation);
                if (relation == SaveTargetRelation.Bound)
                {
                    _boundFileState = BoundFileState.Unconfirmed;
                    FileModificationState = FileModificationState.Unverified;
                    renamedBinding = await RefreshProviderRenameAsync(result) ?? renamedBinding;
                    RefreshModificationState();
                }
                cancellation.ThrowIfCancellationRequested();
                throw new DocumentSaveException(DocumentSaveStage.ConfirmCommit)
                {
                    VerificationId = _pendingFileSave?.Id, CommittedWrite = result, RenamedBinding = renamedBinding
                };
            }
            await PublishSavedFileAsync(result, revision);
            return result.File;
        }
        catch (DocumentSaveException ex)
        {
            if (ex.Stage is DocumentSaveStage.Conflict or DocumentSaveStage.Superseded)
            {
                var affectsBound = relation == SaveTargetRelation.Bound ||
                    ((resumeReceipt || request is ConfirmSave) && pending.WasBound);
                // A picked version may predate our own unconfirmed commit. Its
                // mismatch does not disprove that separate commit receipt.
                var preservesReceipt = matchesReceipt && !resumeReceipt && ex.Stage == DocumentSaveStage.Conflict;
                // A stale picker can disagree with our own newer confirmed save.
                // Only the transaction's actual bytes establish that the bound file
                // still matches; a difference in picker timestamps alone cannot.
                var preservesConfirmedVersion = request is PickedSave && _boundFileState == BoundFileState.Current &&
                    ex.Stage == DocumentSaveStage.Conflict && LastSavedSnapshot.FileVersion.IsVerified &&
                    string.Equals(ex.ObservedOriginalSha256, LastSavedSnapshot.FileVersion.Sha256, StringComparison.OrdinalIgnoreCase) &&
                    !DocumentSaveTarget.RequiresLocationConfirmation(LastSavedSnapshot.FileVersion.TargetPath, target.FinalPath) &&
                    (LastSavedSnapshot.FileVersion.TargetPath == null ||
                    await DocumentSaveTarget.SameLocationAsync(LastSavedSnapshot.FileVersion.TargetPath, target.FinalPath));
                if (affectsBound && ex.CommittedWrite != null)
                    ex.RenamedBinding = await RefreshProviderRenameAsync(ex.CommittedWrite);
                if ((matchesReceipt || request is ConfirmSave) && !preservesReceipt &&
                    ReferenceEquals(_pendingFileSave, pending)) ClearPendingFileSave();
                if (affectsBound && !_disposed && !(preservesReceipt && pending.WasBound) && !preservesConfirmedVersion)
                {
                    _boundFileState = HasUnconfirmedFileSave || ex.Stage == DocumentSaveStage.Superseded
                        ? BoundFileState.Superseded : BoundFileState.Changed;
                    FileModificationState = FileModificationState.Modified;
                    RefreshModificationState();
                }
            }
            ex.RenamedBinding ??= renamedBinding;
            throw;
        }
    }

    private void UpdateSavedTargetPath(string targetPath)
    {
        // A location update does not verify saved bytes. Pending commits keep
        // their bound/active save guard until a verified revision is published.
        var previous = LastSavedSnapshot;
        if (previous.FileVersion.TargetPath == targetPath) return;
        LastSavedSnapshot = new DocumentSnapshot(previous.Baseline, previous.Encoding, previous.LineEnding,
            previous.DateModifiedFileTime, previous.FileVersion.Sha256, targetPath);
        previous.Dispose();
    }

    private async ValueTask<StorageFile> RefreshProviderRenameAsync(DocumentFileWriteResult result,
        PendingFileSave expectedPending = null, CancellationToken cancellation = default)
    {
        if (!await result.CanFollowProviderGrantAsync() || _disposed || cancellation.IsCancellationRequested ||
            (expectedPending != null && !ReferenceEquals(_pendingFileSave, expectedPending))) return null;
        UpdateSavedTargetPath(result.TargetPath);
        // Keep the grant that actually committed even when its provider exposes the
        // new name later. An older grant may continue to resolve a reused pathname.
        if (!ReferenceEquals(EditingFile, result.File) || EditingFilePath != result.File.Path || EditingFileName != result.File.Name)
        {
            EditingFile = result.File;
            FileRenamed?.Invoke(this, EventArgs.Empty);
        }
        return result.File;
    }

    private async Task PublishSavedFileAsync(DocumentFileWriteResult result, SaveRevision revision)
    {
        using var measurement = OperationMetrics.Measure("document.save.publish");
        if (revision.Version == TextEditorCore.ContentVersion)
            await TryRotateRecoveryJournalAsync(revision.Baseline, revision.Sequence);
        if (_disposed) return;

        // Encoding/EOL controls remain available during a save. Preserve
        // their latest effective choice, including a return to the old default.
        var currentEncoding = GetEncoding();
        var currentLineEnding = GetLineEnding();
        var previous = LastSavedSnapshot;
        // The saved snapshot exists only once its file has committed.
        LastSavedSnapshot = new DocumentSnapshot(revision.Baseline, revision.Encoding, revision.LineEnding, result.Version.ModifiedTime, result.Version.Sha256, result.Version.TargetPath);
        previous.Dispose();
        ClearPendingFileSave();
        EditingFile = result.File;
        _boundFileState = BoundFileState.Current;
        FileModificationState = FileModificationState.Untouched;
        _requiresSaveAs = false;
        _requestedEncoding = EncodingCatalog.Equals(currentEncoding, revision.Encoding) ? null : currentEncoding;
        RequestedLineEnding = currentLineEnding == revision.LineEnding ? (LineEnding?)null : currentLineEnding;
        _publishedContentVersion = revision.Version;
        if (revision.Version == TextEditorCore.ContentVersion) TextEditorCore.MarkSaved();
        else TextEditorCore.MarkModified();
        RefreshModificationState();
        _activeSaveTarget = SaveTargetRelation.Different;
        FileSaved?.Invoke(this, EventArgs.Empty);
        StartCheckingFileStatusPeriodically();
        if (result.ProviderError != null)
        {
            NotificationCenter.Instance.PostNotification(
                _resourceLoader.GetString("TextEditor_NotificationMsg_CloudSyncPending"), 5000);
        }
    }
}
