// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.IO;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents.IO;
using Notepads.Features.Documents.Storage;
using Notepads.Features.Documents.Text;
using Notepads.Infrastructure.Threading;

namespace Notepads.Presentation.Controls.TextEditor;

public sealed partial class TextEditorCore
{
    internal async Task ConvertIndentationAsync(bool useTabs, Guid ownerId, CancellationToken cancellation = default)
    {
        if (!CanConvertIndentation || useTabs && _language.Id == "yaml") return;
        var selectionMode = Native.SelectionMode;
        var rectangle = selectionMode == WinUIEditor.SelectionMode.Rectangle;
        var rectangularBounds = rectangle || selectionMode == WinUIEditor.SelectionMode.Thin;
        var moveExtends = Native.MoveExtendsSelection;
        var main = Native.MainSelection;
        var selections = new ((long Line, long Column, long Virtual) Anchor, (long Line, long Column, long Virtual) Caret)[rectangle ? 0 : Native.Selections];
        (long Line, long Column, long Virtual) Capture(long position, long space) => (Native.LineFromPosition(position), Native.GetColumn(position), space);
        var rectangleAnchor = rectangularBounds ? Capture(Native.RectangularSelectionAnchor, Native.RectangularSelectionAnchorVirtualSpace) : default;
        var rectangleCaret = rectangularBounds ? Capture(Native.RectangularSelectionCaret, Native.RectangularSelectionCaretVirtualSpace) : default;
        for (var i = 0; i < selections.Length; i++)
            selections[i] = (Capture(Native.GetSelectionNAnchor(i), Native.GetSelectionNAnchorVirtualSpace(i)), Capture(Native.GetSelectionNCaret(i), Native.GetSelectionNCaretVirtualSpace(i)));
        GetScrollViewerPosition(out var horizontal, out var vertical);
        GetViewportAnchor(out var documentLine, out var subLine);
        var rowOffset = FirstVisibleRowOffset;
        var enabled = IsEnabled;
        IsEnabled = false;
        DocumentBaseline candidate = null;
        var changed = false;
        try
        {
            // A read lease freezes the source while bounded UTF-8 buffers build
            // a temporary generation. One native publication preserves undo and
            // avoids a UI-thread edit/notification for every document line.
            using (var reader = AcquireDocumentReader())
            using (var input = new EditorDocumentStream(reader, ownsReader: false))
            {
                var tabWidth = Native.TabWidth;
                candidate = await DocumentBaseline.CreateAsync(async output =>
                    changed = await IndentationConverter.CopyAsync(input, output, useTabs, tabWidth, cancellation).ConfigureAwait(false),
                    ownerId, cancellation);
            }
            cancellation.ThrowIfCancellationRequested();
            if (!changed || _disposed) return;
            using var source = await candidate.OpenReadStreamAsync(cancellation);
            using var stream = source.AsInputStream();
            await Native.LoadUtf8Async(stream, (ulong)candidate.ByteLength, preserveUndo: true).AsCompletionTask(cancellation);
            if (_disposed) return;
            if (rectangularBounds)
            {
                Native.RectangularSelectionAnchor = Native.FindColumn(rectangleAnchor.Line, rectangleAnchor.Column);
                Native.RectangularSelectionCaret = Native.FindColumn(rectangleCaret.Line, rectangleCaret.Column);
                Native.RectangularSelectionAnchorVirtualSpace = rectangleAnchor.Virtual;
                Native.RectangularSelectionCaretVirtualSpace = rectangleCaret.Virtual;
            }
            // Replacing a prefix can collapse a caret inside it. Restore endpoints
            // by visual column; a stream caret inside a new tab resolves to its edge.
            if (!rectangle)
            {
                // Allocate slots before assigning endpoints. AddSelection trims
                // intersecting ranges, so adding collapsed endpoints directly can
                // remove earlier slots and invalidate the saved main index. The
                // temporary ranges are beyond EOF in virtual space; each new range
                // starts at EOF itself and cannot trim any of the existing slots.
                Native.SetSelection(Native.Length, Native.Length);
                for (var i = 0; i < selections.Length; i++)
                {
                    if (i != 0) Native.AddSelection(Native.Length, Native.Length);
                    Native.SetSelectionNAnchorVirtualSpace(i, 1);
                    Native.SetSelectionNCaretVirtualSpace(i, 1);
                }
                var emptyCarets = selections.Length > 1 ? new Dictionary<(long Position, long Virtual), int>() : null;
                var count = 0;
                var restoredMain = 0;
                for (var i = 0; i < selections.Length; i++)
                {
                    var (anchor, caret) = selections[i];
                    var anchorPos = Native.FindColumn(anchor.Line, anchor.Column);
                    var caretPos = Native.FindColumn(caret.Line, caret.Column);
                    if (emptyCarets != null && anchorPos == caretPos && anchor.Virtual == caret.Virtual)
                    {
                        if (emptyCarets.TryGetValue((caretPos, caret.Virtual), out var index))
                        {
                            if (i == main) restoredMain = index;
                            continue;
                        }
                        emptyCarets.Add((caretPos, caret.Virtual), count);
                    }
                    if (i == main) restoredMain = count;
                    Native.SetSelectionNAnchor(count, anchorPos);
                    Native.SetSelectionNCaret(count, caretPos);
                    Native.SetSelectionNAnchorVirtualSpace(count, anchor.Virtual);
                    Native.SetSelectionNCaretVirtualSpace(count, caret.Virtual);
                    count++;
                }
                // Merge newly coincident carets before the next key: Delete acts
                // once per range and only removes duplicates after deleting.
                for (var i = selections.Length - 1; i >= count; i--) Native.DropSelectionN(i);
                Native.MainSelection = restoredMain;
            }
            // ChangeSelectionMode preserves the extension flag. SelectionMode's
            // setter toggles it, making ordinary arrow keys extend the selection.
            // Thin mode also retains its independent rectangular bounds above.
            if (!rectangle) Native.ChangeSelectionMode(selectionMode);
            Native.MoveExtendsSelection = moveExtends;
            ResetDetectedIndentation();
            IndentationChanged?.Invoke(this, EventArgs.Empty);
            SetScrollViewerInitPosition(horizontal, vertical, documentLine, subLine, preservePreferredColumn: true, rowOffset: rowOffset);
            Native.ChooseCaretX();
        }
        finally
        {
            candidate?.Dispose();
            if (!_disposed) IsEnabled = enabled;
        }
    }
}
