// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;

namespace Notepads.Features.Documents.Text;

// A simplified form of VS Code's guessIndentation over a bounded sample.
public static class IndentationDetector
{
    public const int SampleBytes = 256 * 1024;
    public const int SampleLines = 10_000;

    // -1 = tabs, 2..8 = spaces, 0 = the sample shows no usable indentation.
    public static int Detect(ReadOnlySpan<char> sample)
    {
        Span<int> deltas = stackalloc int[9];
        int tabLines = 0, spaceLines = 0, previousSpaces = -1;
        for (var line = 0; line < SampleLines && !sample.IsEmpty; line++)
        {
            var end = sample.IndexOfAny('\r', '\n');
            var text = end < 0 ? sample : sample[..end];
            var next = end < 0 ? sample.Length : end + (sample[end] == '\r' && end + 1 < sample.Length && sample[end + 1] == '\n' ? 2 : 1);
            sample = sample[next..];
            var indent = text.IndexOfAnyExcept(' ', '\t');
            if (indent < 0) continue;
            var hasTab = text[..indent].Contains('\t');
            if (hasTab) tabLines++;
            else if (indent >= 2) spaceLines++;
            // Tabs and single spaces, such as JSDoc " * " lines, don't measure a width.
            var spaces = hasTab || indent == 1 ? -1 : indent;
            if (spaces >= 0 && previousSpaces >= 0)
            {
                var delta = Math.Abs(spaces - previousSpaces);
                if (delta is > 0 and <= 8) deltas[delta]++;
            }
            previousSpaces = spaces;
        }
        if (tabLines == spaceLines) return 0;
        if (tabLines > spaceLines) return -1;
        ReadOnlySpan<int> widths = [2, 4, 6, 8, 3, 5, 7];
        var width = 0;
        foreach (var candidate in widths)
            if (deltas[candidate] > deltas[width]) width = candidate;
        // Two wins over a guessed four when it is at least half as common.
        if (width == 4 && deltas[2] > 0 && 2 * deltas[2] >= deltas[4]) width = 2;
        return width;
    }
}
