// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.IO;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Sessions.Contracts;
using Notepads.Infrastructure.Threading;

namespace Notepads.Presentation.Controls.TextEditor;

public sealed partial class TextEditorCore
{
    private bool _movingToWindow;
    private ulong _moveSelectionRevision;
    private Dictionary<string, Action> _deferredMoveSettings;

    internal WindowMoveCapture BeginWindowMove()
    {
        if (_movingToWindow || _disposed) throw new InvalidOperationException("The editor is already moving or closed.");
        _view.CancelScrollAnimation();
        var state = new WindowMoveRequest
        {
            HasAddedLogEntry = _hasAddedLogEntry,
            DetectedIndentation = _detectedIndentation,
            IndentationChecked = _indentationCheckedVersion == ContentVersion
        };
        var capture = new WindowMoveCapture(Native, state);
        _movingToWindow = true;
        _moveSelectionRevision = Native.SelectionRevision;
        return capture;
    }

    internal bool IsWindowMoveStateUnchanged => _movingToWindow && Native.SelectionRevision == _moveSelectionRevision;

    internal void EndWindowMove()
    {
        _movingToWindow = false;
        var pending = _deferredMoveSettings;
        _deferredMoveSettings = null;
        if (!_disposed && pending != null)
            foreach (var apply in pending.Values) apply();
    }

    // Called on the UI thread inside each external-settings dispatch. Coalesce by
    // setting so a long launch never retains an unbounded list of event closures.
    private void ApplyOrDeferMoveSetting(string name, Action apply)
    {
        if (_disposed) return;
        if (_movingToWindow) (_deferredMoveSettings ??= new())[name] = apply;
        else apply();
    }

    internal async Task RestoreWindowMoveStateAsync(WindowMoveRequest state, Stream input, CancellationToken cancellation)
    {
        state.Validate();
        await Native.RestoreStateAsync(input.AsInputStream(), (ulong)state.StateBytes, state.StateSha256.ToLowerInvariant())
            .AsCompletionTask(cancellation);
        _hasAddedLogEntry = state.HasAddedLogEntry;
        _detectedIndentation = state.DetectedIndentation;
        _indentationCheckedVersion = state.IndentationChecked ? ContentVersion : -1;
    }
}
