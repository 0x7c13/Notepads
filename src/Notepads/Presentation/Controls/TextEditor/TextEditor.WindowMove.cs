// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Diagnostics.CodeAnalysis;
using System.IO;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Sessions.Contracts;

namespace Notepads.Presentation.Controls.TextEditor;

public sealed partial class TextEditor
{
    private bool _windowMoveReserved;
    private bool _windowMoveActive;
    private bool _skipMovedAttachmentLogEntry;
    private bool _deferredMoveLogEntry;
    [SuppressMessage("Usage", "CA2213:Disposable fields should be disposed",
        Justification = "Borrows the using-scoped cancellation source in WithWindowMoveAsync until that operation drains.")]
    private CancellationTokenSource _windowMoveCancellation;
    internal bool IsMovingToWindow => _windowMoveReserved;
    internal void CancelWindowMove() => _windowMoveCancellation?.Cancel();

    internal async Task WithWindowMoveAsync(Func<WindowMoveCapture, CancellationToken, Task> operation,
        CancellationToken cancellation)
    {
        if (_disposed || _windowMoveReserved) throw new InvalidOperationException("The tab is already moving or closed.");
        _windowMoveReserved = true; // Reserve before waiting for preceding save/reload operations.
        using var requested = CancellationTokenSource.CreateLinkedTokenSource(cancellation);
        _windowMoveCancellation = requested;
        try
        {
            await RunDocumentOperationAsync(async token =>
            {
                var enabled = TextEditorCore.IsEnabled;
                var viewEnabled = IsEnabled;
                IsEnabled = false; // Includes the diff panes, not just the hidden source editor.
                TextEditorCore.IsEnabled = false;
                try
                {
                    using var capture = TextEditorCore.BeginWindowMove();
                    _windowMoveActive = true;
                    // This callback owns preparation, launch, admitted retirement and
                    // acknowledgement. Cancellation requests retirement; it never
                    // abandons the callback and releases these guards underneath it.
                    await operation(capture, token);
                }
                finally
                {
                    _windowMoveActive = false;
                    TextEditorCore.EndWindowMove();
                    if (!_disposed)
                    {
                        IsEnabled = viewEnabled;
                        TextEditorCore.IsEnabled = enabled;
                        if (_deferredMoveLogEntry && IsLoaded) TextEditorCore.TryInsertNewLogEntry();
                    }
                    _deferredMoveLogEntry = false;
                }
            }, requested.Token);
        }
        finally { _windowMoveReserved = false; _windowMoveCancellation = null; }
    }

    internal async Task<bool> CommitReservedMoveAsync(Func<bool> sourceUnchanged,
        Func<Task> acknowledge, Action removeLogicalSource)
    {
        if (_disposed || !_windowMoveActive || !TextEditorCore.IsWindowMoveStateUnchanged || !sourceUnchanged()) return false;
        _transferCommitInProgress = true;
        try
        {
            await acknowledge();
            removeLogicalSource();
            return true;
        }
        finally { _transferCommitInProgress = false; }
    }

    internal async Task RestoreWindowMoveStateAsync(WindowMoveRequest state, Stream input, CancellationToken cancellation)
    {
        await TextEditorCore.RestoreWindowMoveStateAsync(state, input, cancellation);
        // This is an existing open document, even if it was initially empty and
        // the user subsequently typed .LOG. Reattachment must not append a date.
        _skipMovedAttachmentLogEntry = true;
    }
}
