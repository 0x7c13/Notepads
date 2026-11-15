// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Presentation.Controls.TextEditor;
using Windows.Storage;

namespace Notepads.Presentation.Workspace;

public sealed partial class NotepadsCore
{
    private readonly List<FileOpenRequest> _openingFiles = [];
    private readonly List<FileAdmission> _fileAdmissions = [];
    private CancellationTokenSource _fileOperationLifetime = new();
    private bool _isClosing;

    // The isolated workspace host can hold the real admission path before I/O.
    internal Func<Task> BeforeFilePreparation { get; set; }

    private sealed class FileOpenRequest(StorageFile file, CancellationToken cancellation, Func<bool> mayPublish)
    {
        internal StorageFile File { get; } = file;
        internal bool IsCurrent => !cancellation.IsCancellationRequested && mayPublish?.Invoke() != false;
        internal bool Publishing { get; private set; }
        internal bool HasOriginGuard => mayPublish != null;
        internal TaskCompletionSource<WorkspaceFileOpenResult> Completion { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        private readonly List<FileOpenFollower> _followers = [];

        internal FileOpenFollower Follow(Func<bool> mayFollow)
        {
            var follower = new FileOpenFollower(mayFollow);
            _followers.Add(follower);
            return follower;
        }

        internal void StopFollowing(FileOpenFollower follower) => _followers.Remove(follower);

        internal void BeginPublication()
        {
            Publishing = true;
            // Check each origin while it is still visible. Installing or selecting
            // the destination can hide it before the waiting caller resumes.
            foreach (var follower in _followers) follower.Admitted = follower.MayFollow();
            _followers.Clear();
        }
    }

    private sealed class FileOpenFollower(Func<bool> mayFollow)
    {
        internal Func<bool> MayFollow { get; } = mayFollow;
        internal bool Admitted { get; set; }
    }

    private sealed partial class FileAdmission(NotepadsCore owner, StorageFile file) : IDisposable
    {
        internal StorageFile File { get; } = file;
        internal TaskCompletionSource<object> Completion { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public void Dispose()
        {
            owner._fileAdmissions.Remove(this);
            Completion.TrySetResult(null);
        }
    }

    private CancellationTokenSource CaptureFileOperation() => CancellationTokenSource.CreateLinkedTokenSource(
        _fileOperationLifetime.Token, _sessionController().FileOperationToken);

    public WorkspaceTab FindReadyTab(StorageFile file) => file == null ? null :
        GetTabs().FirstOrDefault(tab => tab.ReadyEditor?.EditingFile?.IsEqual(file) == true);

    public async Task<bool> OpenFileAsync(StorageFile file, Func<WorkspaceFileOpenResult, Task> presentResultAsync,
        Func<bool> mayPublish = null)
    {
        if (_disposed || IsClosing || file == null || mayPublish?.Invoke() == false) return false;
        using var lifetime = CaptureFileOperation();
        var cancellation = lifetime.Token;
        var pending = _openingFiles.FirstOrDefault(request =>
            request.File.IsEqual(file) && request.IsCurrent &&
            (mayPublish == null ? !request.HasOriginGuard : !request.Publishing));
        if (pending != null)
        {
            var follower = mayPublish == null ? null : pending.Follow(() => !cancellation.IsCancellationRequested && mayPublish());
            try
            {
                var shared = await pending.Completion.Task.WaitAsync(cancellation);
                if (follower?.Admitted == true && shared.Tab != null)
                    await PresentOpenResultAsync(new(WorkspaceFileOpenKind.Existing, shared.Tab), presentResultAsync, cancellation);
            }
            catch (OperationCanceledException) { }
            finally { if (follower != null) pending.StopFollowing(follower); }
            return false;
        }

        var request = new FileOpenRequest(file, cancellation, mayPublish);
        _openingFiles.Add(request);
        var result = new WorkspaceFileOpenResult(WorkspaceFileOpenKind.Cancelled);
        try
        {
            result = await TrackWorkAsync(OpenCandidateAsync(file, async () =>
            {
                if (BeforeFilePreparation != null) await BeforeFilePreparation();
                if (!request.IsCurrent) throw new OperationCanceledException();
                return await CreateTextEditorAsync(Guid.NewGuid(), file);
            }, publishReceiptAsync: null, atIndex: -1, cancellation, request));

            // Presentation retains only the request record. Native shutdown does
            // not wait for a dialog, and UI failure cannot discard an installed tab.
            if (result.Kind != WorkspaceFileOpenKind.Failed || request.IsCurrent)
                await PresentOpenResultAsync(result, presentResultAsync, cancellation);
            return result.Kind == WorkspaceFileOpenKind.Created;
        }
        finally
        {
            _openingFiles.Remove(request);
            request.Completion.TrySetResult(result);
        }
    }

    private async Task PresentOpenResultAsync(WorkspaceFileOpenResult result, Func<WorkspaceFileOpenResult, Task> presentResultAsync,
        CancellationToken cancellation)
    {
        if (_disposed || IsClosing || cancellation.IsCancellationRequested || presentResultAsync == null) return;
        try { await presentResultAsync(result); }
        catch (Exception ex) { LoggingService.LogException(ex); }
    }

    internal async Task<WorkspaceFileOpenResult> OpenTransferredFileAsync(StorageFile file,
        Func<Task<ITextEditor>> prepareAsync, Func<ITextEditor, Task> publishReceiptAsync,
        int atIndex, CancellationToken requestCancellation)
    {
        if (_disposed || IsClosing) return new(WorkspaceFileOpenKind.Cancelled);
        using var lifetime = CaptureFileOperation();
        using var linked = CancellationTokenSource.CreateLinkedTokenSource(lifetime.Token, requestCancellation);
        return await TrackWorkAsync(OpenCandidateAsync(file, prepareAsync,
            publishReceiptAsync ?? throw new ArgumentNullException(nameof(publishReceiptAsync)), atIndex, linked.Token));
    }

    private async Task<FileAdmission> AcquireFileAdmissionAsync(StorageFile file, CancellationToken cancellation)
    {
        while (true)
        {
            cancellation.ThrowIfCancellationRequested();
            ThrowIfDisposedOrClosing();
            var preceding = file == null ? null : _fileAdmissions.FirstOrDefault(active => active.File?.IsEqual(file) == true);
            if (preceding == null)
            {
                var admission = new FileAdmission(this, file);
                _fileAdmissions.Add(admission);
                return admission;
            }
            await preceding.Completion.Task.WaitAsync(cancellation);
        }
    }

    private async Task<WorkspaceFileOpenResult> OpenCandidateAsync(StorageFile file,
        Func<Task<ITextEditor>> prepareAsync, Func<ITextEditor, Task> publishReceiptAsync,
        int atIndex, CancellationToken cancellation, FileOpenRequest request = null)
    {
        ITextEditor candidate = null;
        FileAdmission admission = null;
        try
        {
            cancellation.ThrowIfCancellationRequested();
            ThrowIfDisposedOrClosing();
            if (file != null) await _sessionController().ResolveFileAsync(file, cancellation);
            // Recovery publication needs the session gate, so never await it
            // while holding file admission.
            admission = await AcquireFileAdmissionAsync(file, cancellation);
            if (request?.IsCurrent == false) return new(WorkspaceFileOpenKind.Cancelled);
            if (FindReadyTab(file) is WorkspaceTab existing)
            {
                request?.BeginPublication();
                SwitchTo(existing);
                return new(WorkspaceFileOpenKind.Existing, existing);
            }

            candidate = await prepareAsync();
            cancellation.ThrowIfCancellationRequested();
            ThrowIfDisposedOrClosing();
            if (request?.IsCurrent == false) return new(WorkspaceFileOpenKind.Cancelled);
            if (FindReadyTab(file) is WorkspaceTab duplicate)
            {
                request?.BeginPublication();
                SwitchTo(duplicate);
                return new(WorkspaceFileOpenKind.Existing, duplicate);
            }

            WorkspaceTab installed = null;
            if (publishReceiptAsync == null)
            {
                request?.BeginPublication();
                OpenTextEditor(candidate, atIndex);
                installed = GetTab(candidate);
            }
            else
            {
                await _sessionController().PublishTransferredEditorAsync(
                    () => publishReceiptAsync(candidate),
                    () =>
                    {
                        installed = new WorkspaceTab(candidate);
                        OpenTab(installed, atIndex, allowWhileClosing: true);
                    }, cancellation);
            }
            // Controller drain may finish and close the tab before this
            // continuation. Return the installed identity, not a fresh lookup.
            candidate = null;
            FocusOnSelectedTextEditor();
            return new(WorkspaceFileOpenKind.Created, installed);
        }
        catch (OperationCanceledException) { return new(WorkspaceFileOpenKind.Cancelled); }
        catch (Exception ex) { return new(WorkspaceFileOpenKind.Failed, Error: ex); }
        finally
        {
            try
            {
                if (candidate != null && GetTab(candidate) == null)
                    await DiscardUnattachedEditorAsync(candidate);
            }
            finally { admission?.Dispose(); }
        }
    }
}
