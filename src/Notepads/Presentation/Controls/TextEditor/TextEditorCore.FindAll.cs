// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Infrastructure.Threading;
using Notepads.Presentation.Controls.FindAndReplace;
using WinUIEditor;

namespace Notepads.Presentation.Controls.TextEditor;

public sealed record DocumentSearchRow(long Start, long End, string Label);
internal sealed record DocumentSearchResults(EditorSearchResult Summary, List<DocumentSearchRow> Rows, bool Limited);

public sealed partial class TextEditorCore
{
    internal const int MaximumSearchResults = 2000;

    internal async Task<DocumentSearchResults> FindAllAsync(SearchContext context, CancellationToken cancellation)
    {
        var rows = new List<DocumentSearchRow>();
        var summary = SearchResult(EditorSearchStatus.NotFound);
        if (_disposed || !IsEnabled || _movingToWindow) return new(summary, rows, false);
        if (string.IsNullOrEmpty(context?.SearchText)) return new(summary, rows, false);
        if (context.SearchText.Length > SearchPatternLimit)
            return new(SearchResult(EditorSearchStatus.ResourceLimit), rows, false);
        var revision = Native.DocumentRevision;
        var watch = Stopwatch.StartNew();
        long lastYield = -8;
        long position = 0;
        long excludedEmpty = -1;
        bool limited = false;
        while (true)
        {
            cancellation.ThrowIfCancellationRequested();
            if (_disposed || !IsEnabled || _movingToWindow || Native.DocumentRevision != revision)
                return new(SearchResult(EditorSearchStatus.Stale), [], false);
            if (position > Native.Length) break;
            if (watch.Elapsed.TotalSeconds >= 5) { limited = true; break; }
            EditorSearchResult match;
            if (context.UseRegex)
            {
                // Use the same ICU engine, timeout and draining read lease as Find Next.
                match = await Native.FindRegexAsync(context.SearchText, context.MatchCase, position, false,
                    false, excludedEmpty).AsCompletionTask(cancellation);
                if (_disposed || !IsEnabled || Native.DocumentRevision != revision)
                    return new(SearchResult(EditorSearchStatus.Stale), [], false);
            }
            else
            {
                // Native bounds candidate starts and comparison work, while a match
                // may cross the slice boundary. It preserves target/selection state.
                match = Native.FindLiteralSlice(context.SearchText, context.MatchCase, context.MatchWholeWord, position);
                if (match.Status == EditorSearchStatus.NotFound && match.Start < Native.Length)
                {
                    position = match.Start;
                    // Yield the first slice promptly, then coalesce cheap slices
                    // into short UI turns rather than delaying once per pattern-sized span.
                    if (watch.ElapsedMilliseconds - lastYield >= 8)
                    {
                        await Task.Delay(1, cancellation);
                        lastYield = watch.ElapsedMilliseconds;
                    }
                    continue;
                }
            }
            if (match.Status != EditorSearchStatus.Found)
            {
                if (match.Status == EditorSearchStatus.NotFound) break;
                if (match.Status == EditorSearchStatus.TimedOut) { limited = true; break; }
                return new(match, [], false);
            }
            if (rows.Count == MaximumSearchResults) { limited = true; break; }
            var line = Native.LineFromPosition(match.Start);
            var snippetStart = Native.PositionFromLine(line);
            if (match.Start - snippetStart > 160) snippetStart = match.Start;
            var snippetEnd = Math.Min(Native.GetLineEndPosition(line), snippetStart + 256);
            while (snippetEnd < Native.Length && (Native.GetCharAt(snippetEnd) & 0xc0) == 0x80) snippetEnd--;
            var snippet = ReadRange(snippetStart, snippetEnd).Replace('\0', '\u2400').Replace('\t', ' ');
            if (snippetStart != Native.PositionFromLine(line)) snippet = "…" + snippet;
            if (snippetEnd < Native.GetLineEndPosition(line)) snippet += "…";
            rows.Add(new(match.Start, match.End, $"{line + 1}: {snippet}"));
            position = match.End;
            excludedEmpty = match.Start == match.End ? position : -1;
            // Regex's excluded-empty path advances using ICU's Unicode boundaries.
            if (rows.Count % 32 == 0 || watch.ElapsedMilliseconds - lastYield >= 8)
            {
                await Task.Delay(1, cancellation);
                lastYield = watch.ElapsedMilliseconds;
            }
        }
        cancellation.ThrowIfCancellationRequested();
        if (_disposed || Native.DocumentRevision != revision) return new(SearchResult(EditorSearchStatus.Stale), [], false);
        summary.Status = rows.Count == 0 ? EditorSearchStatus.NotFound : EditorSearchStatus.Found;
        summary.MatchCount = (ulong)rows.Count;
        summary.Revision = revision;
        return new(summary, rows, limited);
    }

    internal bool NavigateSearchResult(DocumentSearchResults results, int index)
    {
        if (!CanEdit || _movingToWindow || results.Summary.Revision != Native.DocumentRevision ||
            index < 0 || index >= results.Rows.Count) return false;
        var row = results.Rows[index];
        Native.SetSel(row.Start, row.End);
        Native.ScrollCaret();
        return true;
    }
}
