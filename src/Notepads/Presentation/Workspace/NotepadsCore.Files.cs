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

    private sealed class FileOpenRequest(StorageFile file, CancellationToken cancellation)
    {
        internal StorageFile File { get; } = file;
        internal CancellationToken Cancellation { get; } = cancellation;
        internal TaskCompletionSource<object> Completion { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
    }

    private sealed class FileAdmission(NotepadsCore owner, StorageFile file) : IDisposable
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

    public async Task<bool> OpenFileAsync(StorageFile file, Func<WorkspaceFileOpenResult, Task> presentResultAsync)
    {
        if (_disposed || IsClosing || file == null) return false;
        using var lifetime = CaptureFileOperation();
        var cancellation = lifetime.Token;
        var pending = _openingFiles.FirstOrDefault(request =>
            !request.Cancellation.IsCancellationRequested && request.File.IsEqual(file));
        if (pending != null)
        {
            try { await pending.Completion.Task.WaitAsync(cancellation); }
            catch (OperationCanceledException) { }
            return false;
        }

        var request = new FileOpenRequest(file, cancellation);
        _openingFiles.Add(request);
        try
        {
            var result = await TrackWorkAsync(OpenCandidateAsync(file, async () =>
            {
                if (BeforeFilePreparation != null) await BeforeFilePreparation();
                cancellation.ThrowIfCancellationRequested();
                return await CreateTextEditorAsync(Guid.NewGuid(), file);
            }, publishReceiptAsync: null, atIndex: -1, cancellation));

            // Presentation retains only the request record. Native shutdown does
            // not wait for a dialog, and UI failure cannot discard an installed tab.
            if (!_disposed && !IsClosing && !cancellation.IsCancellationRequested && presentResultAsync != null)
            {
                try { await presentResultAsync(result); }
                catch (Exception ex) { LoggingService.LogException(ex); }
            }
            return result.Kind == WorkspaceFileOpenKind.Created;
        }
        finally
        {
            _openingFiles.Remove(request);
            request.Completion.TrySetResult(null);
        }
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
        int atIndex, CancellationToken cancellation)
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
            if (FindReadyTab(file) is WorkspaceTab existing)
            {
                SwitchTo(existing);
                return new(WorkspaceFileOpenKind.Existing, existing);
            }

            candidate = await prepareAsync();
            cancellation.ThrowIfCancellationRequested();
            ThrowIfDisposedOrClosing();
            if (FindReadyTab(file) is WorkspaceTab duplicate)
            {
                SwitchTo(duplicate);
                return new(WorkspaceFileOpenKind.Existing, duplicate);
            }

            WorkspaceTab installed = null;
            if (publishReceiptAsync == null)
            {
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
