// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Diagnostics.CodeAnalysis;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Sessions.Recovery;
using Notepads.Presentation.Controls.TextEditor;

namespace Notepads.Presentation.Workspace;

public enum WorkspaceTabState { Pending, Loading, Ready, Failed, Closed }

/// <summary>A workspace document exists before its native editor is materialized.</summary>
[SuppressMessage("Design", "CA1001:Types that own disposable fields should be disposable",
    Justification = "The core and restore controller own tab lifetime; MarkClosed cancels and disposes the lifetime source.")]
public sealed class WorkspaceTab
{
    private readonly CancellationTokenSource _lifetime = new();
    private readonly TaskCompletionSource<ITextEditor> _loadCompletion = new(
        TaskCreationOptions.RunContinuationsAsynchronously);

    internal WorkspaceTab(RecoveryDocumentSource source)
    {
        LifetimeToken = _lifetime.Token;
        RecoverySource = source ?? throw new ArgumentNullException(nameof(source));
        Id = source.Id;
    }

    public WorkspaceTab(ITextEditor editor)
    {
        LifetimeToken = _lifetime.Token;
        AttachedEditor = editor ?? throw new ArgumentNullException(nameof(editor));
        Id = editor.Id;
        State = WorkspaceTabState.Ready;
        RepresentationVersion = 1;
        _loadCompletion.SetResult(editor);
    }

    public Guid Id { get; }
    public WorkspaceTabState State { get; private set; }
    public long RepresentationVersion { get; private set; }
    public ITextEditor ReadyEditor => State == WorkspaceTabState.Ready ? AttachedEditor : null;
    public Task<ITextEditor> LoadCompletion => _loadCompletion.Task;
    public CancellationToken LifetimeToken { get; }
    public Exception LoadError { get; private set; }
    public string Title
    {
        get
        {
            if (ReadyEditor is ITextEditor editor) return editor.DisplayTitle;
            var metadata = RecoverySource?.Metadata;
            return RecoverySource?.EditingFileName ??
                (metadata?.UseContentBasedTitle == true ? metadata.ContentTitle : null) ?? metadata?.FileNamePlaceholder;
        }
    }
    public string EditingFilePath => ReadyEditor is ITextEditor editor ? editor.EditingFilePath : RecoverySource?.EditingFilePath;
    public bool IsModified => ReadyEditor?.IsModified ?? RecoverySource?.Metadata?.IsModified ?? false;

    internal RecoveryDocumentSource RecoverySource { get; private set; }
    internal RecoveryFileAccessBinding FileAccessBinding { get; set; }
    internal ITextEditor AttachedEditor { get; private set; }
    internal bool HasBeenPresented { get; set; }
    internal event EventHandler StateChanged;

    internal bool BeginLoading()
    {
        if (State != WorkspaceTabState.Pending) return false;
        State = WorkspaceTabState.Loading;
        StateChanged?.Invoke(this, EventArgs.Empty);
        return true;
    }

    internal bool TrySetReady(ITextEditor editor)
    {
        if (editor == null || editor.Id != Id || State != WorkspaceTabState.Loading) return false;
        AttachedEditor = editor;
        State = WorkspaceTabState.Ready;
        RepresentationVersion++;
        StateChanged?.Invoke(this, EventArgs.Empty);
        return true;
    }

    internal void CompleteLoading() => _loadCompletion.TrySetResult(ReadyEditor);

    internal void ReturnToPending()
    {
        if (State != WorkspaceTabState.Loading) return;
        State = WorkspaceTabState.Pending;
        StateChanged?.Invoke(this, EventArgs.Empty);
    }

    internal void CancelLoading()
    {
        if (State != WorkspaceTabState.Closed) _lifetime.Cancel();
    }

    internal void FailLoading(Exception error)
    {
        if (State is WorkspaceTabState.Closed or WorkspaceTabState.Ready) return;
        LoadError = error;
        State = WorkspaceTabState.Failed;
        StateChanged?.Invoke(this, EventArgs.Empty);
        _loadCompletion.TrySetResult(null);
    }

    internal void MarkClosed()
    {
        if (State == WorkspaceTabState.Closed) return;
        State = WorkspaceTabState.Closed;
        RepresentationVersion++;
        _lifetime.Cancel();
        _lifetime.Dispose();
        _loadCompletion.TrySetResult(null);
        StateChanged?.Invoke(this, EventArgs.Empty);
    }

    // A retained capture never retires this source, including one committed after Ready.
    internal bool AcknowledgeNativeCapture(RecoveryDocumentSource source, long version, ITextEditor editor)
    {
        if (State != WorkspaceTabState.Ready || RepresentationVersion != version ||
            !ReferenceEquals(ReadyEditor, editor) || !ReferenceEquals(RecoverySource, source)) return false;
        RecoverySource = null;
        return true;
    }

    internal void RetireRecoverySourceAfterReset()
    {
        if (State != WorkspaceTabState.Ready) throw new InvalidOperationException("Only materialized tabs can survive reset.");
        RecoverySource = null;
    }
}
