// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using WinUIEditor;

namespace Notepads.Presentation.Controls.TextEditor;

// Keep native identity/revision validation inside the editor adapter. The viewer
// needs only bounded navigation rows and availability, not engine ABI values.
internal sealed class DiffComparison
{
    internal EditorDiffResult NativeResult { get; }
    internal bool IsAvailable { get; }
    internal bool IsCoarse { get; }
    internal uint AddedLines { get; }
    internal uint DeletedLines { get; }
    internal long[] ChangeRows { get; }

    internal DiffComparison(EditorDiffResult result)
    {
        NativeResult = result;
        IsAvailable = result.Status is EditorDiffStatus.Exact or EditorDiffStatus.Coarse;
        IsCoarse = result.Status == EditorDiffStatus.Coarse;
        AddedLines = result.AddedLineCount;
        DeletedLines = result.DeletedLineCount;
        // A zero-length WinRT ABI array may be projected as null. Normalize
        // the collection once at the adapter boundary, including unchanged text.
        var hunks = IsAvailable ? result.GetHunks() ?? [] : [];
        ChangeRows = new long[hunks.Length];
        for (var index = 0; index < hunks.Length; index++)
            ChangeRows[index] = checked((long)hunks[index].DisplayRow);
    }
}
