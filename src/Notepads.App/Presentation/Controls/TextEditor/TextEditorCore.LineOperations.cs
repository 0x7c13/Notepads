// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents.Text;

namespace Notepads.Presentation.Controls.TextEditor;

public sealed partial class TextEditorCore
{
    internal bool CanTransformSelectedLines => CanEdit && !_movingToWindow && !_diffPreview &&
        Native.Selections == 1 && !Native.SelectionIsRectangle && HasSelection && SelectedLineRange().Last > SelectedLineRange().First;

    private (long First, long Last) SelectedLineRange()
    {
        var first = Native.LineFromPosition(Native.SelectionStart);
        var last = Native.LineFromPosition(Native.SelectionEnd);
        if (last > first && Native.SelectionEnd == Native.PositionFromLine(last)) last--;
        return (first, last);
    }

    // False denotes the explicit memory/line budget, not a partial transformation.
    internal async Task<bool> TransformSelectedLinesAsync(SelectedLineOperation operation, CancellationToken cancellation)
    {
        if (!CanTransformSelectedLines) return true;
        var (first, last) = SelectedLineRange();
        var start = Native.PositionFromLine(first);
        var end = Native.GetLineEndPosition(last);
        if (end - start > SelectedLineOperations.MaximumBytes || last - first + 1 > SelectedLineOperations.MaximumLines) return false;
        var original = ReadRange(start, end);
        var revision = Native.DocumentRevision;
        var selection = Native.SelectionRevision;
        IsEnabled = false;
        try
        {
            var result = await Task.Run(() => SelectedLineOperations.Transform(original, operation, cancellation), cancellation);
            cancellation.ThrowIfCancellationRequested();
            if (_disposed || Native.ReadOnly || _movingToWindow || Native.DocumentRevision != revision ||
                Native.SelectionRevision != selection) return true;
            if (result == original) return true;
            Native.BeginUndoAction();
            try
            {
                var length = ReplaceRange(start, end, result);
                Native.SetSel(start, start + length);
            }
            finally { Native.EndUndoAction(); }
            return true;
        }
        finally { if (!_disposed) IsEnabled = true; }
    }
}
