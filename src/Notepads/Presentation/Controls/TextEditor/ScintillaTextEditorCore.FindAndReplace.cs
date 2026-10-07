// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Documents.Text;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Infrastructure.Threading;
using Notepads.Presentation.Controls.FindAndReplace;
using WinUIEditor;

namespace Notepads.Presentation.Controls.TextEditor;

public sealed partial class TextEditorCore
{
    // Mirrors the native MaximumRegexInput (NativeRegex.h).
    internal const long SearchPatternLimit = 32 * 1024;
    private string _emptyMatchPattern;
    private bool _emptyMatchCase;
    private bool _emptyMatchPrevious;
    private ulong _emptyMatchRevision;
    private ulong _emptyMatchSelectionRevision;
    private long _emptyMatchPosition = -1;

    public string GetSearchString()
    {
        var selection = HasSelection;
        var start = selection ? Native.SelectionStart : Native.WordStartPosition(Native.CurrentPos, true);
        var end = selection ? Native.SelectionEnd : Native.WordEndPosition(Native.CurrentPos, true);
        // Check before reading: the native regex rejects longer patterns, and a
        // range's UTF-8 bytes never undercount its UTF-16 units.
        if (end - start > SearchPatternLimit) return string.Empty;
        var text = ReadRange(start, end);
        if (selection) text = text.Trim();
        return text.Contains("\r") ? string.Empty : text;
    }

    private long FindLiteral(SearchContext context, long start, long end)
    {
        Native.SearchFlags = (context.MatchCase ? FindOption.MatchCase : FindOption.None)
            | (context.MatchWholeWord ? FindOption.WholeWord : FindOption.None);
        Native.SetTargetRange(start, end);
        return Native.SearchInTarget(Encoding.UTF8.GetByteCount(context.SearchText), context.SearchText);
    }

    private static EditorSearchResult SearchResult(EditorSearchStatus status) => new EditorSearchResult
    {
        Status = status,
        Start = -1,
        End = -1,
        ErrorOffset = -1
    };

    public async Task<EditorSearchResult> FindAsync(SearchContext context, bool previous = false,
        bool stopAtBoundary = false, CancellationToken cancellationToken = default)
    {
        if (_disposed || !IsEnabled) return SearchResult(EditorSearchStatus.Stale);
        if (context == null || string.IsNullOrEmpty(context.SearchText)) return SearchResult(EditorSearchStatus.NotFound);
        cancellationToken.ThrowIfCancellationRequested();
        var origin = previous ? Native.SelectionStart : Native.SelectionEnd;
        var selectionRevision = Native.SelectionRevision;
        EditorSearchResult result;
        if (context.UseRegex)
        {
            var excludedEmpty = _emptyMatchPattern == context.SearchText && _emptyMatchCase == context.MatchCase &&
                _emptyMatchPrevious == previous && _emptyMatchRevision == Native.DocumentRevision &&
                _emptyMatchSelectionRevision == Native.SelectionRevision && _emptyMatchPosition == origin
                ? origin : -1;
            result = await Native.FindRegexAsync(context.SearchText, context.MatchCase, origin, previous,
                !stopAtBoundary, excludedEmpty).AsCompletionTask(cancellationToken);
            if (_disposed || cancellationToken.IsCancellationRequested) return SearchResult(EditorSearchStatus.Canceled);
            if (result.Status == EditorSearchStatus.Found && (!IsEnabled || result.Revision != Native.DocumentRevision ||
                selectionRevision != Native.SelectionRevision))
            {
                return SearchResult(EditorSearchStatus.Stale);
            }
        }
        else
        {
            var found = FindLiteral(context, origin, previous ? 0 : Native.Length);
            // Wrap over the whole document, so a sole match around the caret is found.
            if (found < 0 && !stopAtBoundary)
                found = FindLiteral(context, previous ? Native.Length : 0, previous ? 0 : Native.Length);
            result = SearchResult(found >= 0 ? EditorSearchStatus.Found : EditorSearchStatus.NotFound);
            if (found >= 0)
            {
                result.Start = Native.TargetStart;
                result.End = Native.TargetEnd;
                result.MatchCount = 1;
            }
        }

        if (result.Status != EditorSearchStatus.Found) return result;
        Native.SetSel(result.Start, result.End);
        Native.ScrollCaret();
        _emptyMatchPattern = context.UseRegex && result.Start == result.End ? context.SearchText : null;
        _emptyMatchCase = context.MatchCase;
        _emptyMatchPrevious = previous;
        _emptyMatchPosition = result.Start;
        _emptyMatchRevision = Native.DocumentRevision;
        _emptyMatchSelectionRevision = Native.SelectionRevision;
        return result;
    }

    public async Task<EditorSearchResult> ReplaceAsync(SearchContext context, string replacement, bool previous = false,
        CancellationToken cancellationToken = default)
    {
        if (!CanEdit) return SearchResult(EditorSearchStatus.ReadOnly);
        if (context == null || string.IsNullOrEmpty(context.SearchText)) return SearchResult(EditorSearchStatus.NotFound);
        cancellationToken.ThrowIfCancellationRequested();
        var origin = previous ? Native.SelectionEnd : Native.SelectionStart;
        EditorSearchResult result;
        if (context.UseRegex)
        {
            result = await Native.ReplaceRegexAsync(context.SearchText, context.MatchCase, replacement ?? string.Empty,
                origin, previous, false).AsCompletionTask(cancellationToken);
            if (result.Status != EditorSearchStatus.Found || _disposed || cancellationToken.IsCancellationRequested ||
                !CanEdit || result.Revision != Native.DocumentRevision ||
                result.SelectionRevision != Native.SelectionRevision)
            {
                return result;
            }
            // Continue from the edit, preserving V1's selection of the
            // following/preceding match.
            var caret = previous ? result.Start : result.End;
            try { Native.SetSel(caret, caret); }
            catch (Exception error)
            {
                Infrastructure.Diagnostics.LoggingService.LogError($"[{nameof(TextEditorCore)}] Replacement caret update failed: {error.Message}");
                return result;
            }
        }
        else
        {
            Native.SetSel(origin, origin);
            result = await FindAsync(context, previous, true, cancellationToken);
            if (result.Status != EditorSearchStatus.Found) return result;
            TypeText(replacement ?? string.Empty);
        }

        if (!_disposed && !cancellationToken.IsCancellationRequested)
        {
            try { await FindAsync(context, previous, true, cancellationToken); }
            catch (OperationCanceledException) { /* Replacement already committed. */ }
            catch (Exception error)
            {
                Infrastructure.Diagnostics.LoggingService.LogError($"[{nameof(TextEditorCore)}] Search after replacement failed: {error.Message}");
            }
        }
        return result;
    }

    public async Task<EditorSearchResult> ReplaceAllAsync(SearchContext context, string replacement,
        CancellationToken cancellationToken = default)
    {
        if (!CanEdit) return SearchResult(EditorSearchStatus.ReadOnly);
        if (context == null || string.IsNullOrEmpty(context.SearchText)) return SearchResult(EditorSearchStatus.NotFound);
        cancellationToken.ThrowIfCancellationRequested();
        if (context.UseRegex)
        {
            return await Native.ReplaceRegexAsync(context.SearchText, context.MatchCase, replacement ?? string.Empty,
                0, false, true).AsCompletionTask(cancellationToken);
        }

        replacement = Features.Documents.Text.LineEndingUtility.ApplyLineEnding(replacement ?? string.Empty, Features.Documents.Contracts.LineEnding.Cr);
        var result = SearchResult(EditorSearchStatus.NotFound);
        Native.BeginUndoAction();
        try
        {
            long position = 0;
            while (CanEdit && FindLiteral(context, position, Native.Length) >= 0)
            {
                var start = Native.TargetStart;
                var contentVersion = ContentVersion;
                var length = Native.ReplaceTarget(Encoding.UTF8.GetByteCount(replacement), replacement);
                if (ContentVersion == contentVersion) break;
                position = start + length;
                result.Status = EditorSearchStatus.Found;
                result.MatchCount++;
            }
        }
        finally { Native.EndUndoAction(); }
        return result;
    }
}
