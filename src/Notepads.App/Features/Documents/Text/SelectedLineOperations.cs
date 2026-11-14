// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Threading;

namespace Notepads.Features.Documents.Text;

public enum SelectedLineOperation { SortAscending, SortDescending, RemoveDuplicates }

public static class SelectedLineOperations
{
    public const int MaximumBytes = 4 * 1024 * 1024;
    public const int MaximumLines = 100_000;

    // Input is the selected whole-line content, excluding its final line ending.
    // Ordinal, case-sensitive comparison is deterministic across machines/locales.
    public static string Transform(string text, SelectedLineOperation operation, CancellationToken cancellation = default)
    {
        ArgumentNullException.ThrowIfNull(text);
        if (text.Length > MaximumBytes) throw new ArgumentOutOfRangeException(nameof(text));
        if (!Enum.IsDefined(operation)) throw new ArgumentOutOfRangeException(nameof(operation));
        cancellation.ThrowIfCancellationRequested();
        if (text.AsSpan().Count('\r') >= MaximumLines) throw new ArgumentOutOfRangeException(nameof(text));
        var lines = text.Split('\r');
        cancellation.ThrowIfCancellationRequested();
        if (operation == SelectedLineOperation.RemoveDuplicates)
        {
            var seen = new HashSet<string>(StringComparer.Ordinal);
            var output = new List<string>(lines.Length);
            for (var i = 0; i < lines.Length; i++)
            {
                if ((i & 1023) == 0) cancellation.ThrowIfCancellationRequested();
                if (seen.Add(lines[i])) output.Add(lines[i]);
            }
            cancellation.ThrowIfCancellationRequested();
            return string.Join("\r", output);
        }
        Array.Sort(lines, StringComparer.Ordinal);
        if (operation == SelectedLineOperation.SortDescending) Array.Reverse(lines);
        cancellation.ThrowIfCancellationRequested();
        return string.Join("\r", lines);
    }
}
