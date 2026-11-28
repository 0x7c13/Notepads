// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Text;
using Notepads.Features.Preferences;
using Notepads.Presentation.Helpers;
using Notepads.Presentation.Theming;
using Windows.UI.Xaml;
using WinUIEditor;

namespace Notepads.Presentation.Controls.TextEditor;

public sealed partial class TextEditorCore
{
    private const int OccurrenceIndicator = 8;
    private const int BraceIndicator = 9;
    private const int ReadingSampleBytes = 64 * 1024;
    private DispatcherTimer _readingTimer;
    private bool _updatingReadingAids;
    private bool _braceStylesChanged;
    private (long, long) _highlightedBracePair = (-1, -1);
    private (long Version, long Caret, long Anchor, long SampleStart, long SampleEnd, ReadingAids Aids)? _readingState;

    private async void OnReadingAidsChanged(object sender, EventArgs args) =>
        await Dispatcher.CallOnUIThreadAsync(() => ApplyOrDeferMoveSetting(nameof(OnReadingAidsChanged), ApplyReadingAids));

    private void ApplyReadingAids()
    {
        if (_disposed || _palette == null) return;
        var aids = _diffPreview
            ? (_diffWhitespaceVisible ? ReadingAids.Whitespace : ReadingAids.None)
            : ApplicationPreferences.EditorReadingAids;
        Native.ModEventMask = TextModifications |
            (aids.HasFlag(ReadingAids.Braces) ? ModificationFlags.ChangeStyle : ModificationFlags.None);
        Native.ViewWS = aids.HasFlag(ReadingAids.Whitespace) ? WhiteSpace.VisibleAlways : WhiteSpace.Invisible;
        Native.IndentationGuides = aids.HasFlag(ReadingAids.IndentGuides) ? IndentView.LookBoth : IndentView.None;
        var foreground = ToScintillaColor(_palette[EditorColorRole.LineNumber]);
        Native.SetWhitespaceFore(true, foreground);
        Native.StyleSetFore((int)StylesCommon.IndentGuide, foreground);
        Native.StyleSetFore((int)StylesCommon.BraceLight, foreground);
        Native.IndicSetStyle(OccurrenceIndicator, IndicatorStyle.StraightBox);
        Native.IndicSetFore(OccurrenceIndicator, ToScintillaColor(_palette[EditorColorRole.Foreground]));
        Native.IndicSetAlpha(OccurrenceIndicator, (Alpha)20);
        Native.IndicSetOutlineAlpha(OccurrenceIndicator, (Alpha)(_accessibility.HighContrast ? 255 : 90));
        Native.IndicSetUnder(OccurrenceIndicator, true);
        Native.IndicSetStyle(BraceIndicator, IndicatorStyle.Box);
        Native.IndicSetFore(BraceIndicator, ToScintillaColor(_palette[EditorColorRole.Foreground]));
        Native.BraceHighlightIndicator(true, BraceIndicator);
        InvalidateBraceStyles();
    }

    private void InvalidateBraceStyles()
    {
        _braceStylesChanged = true;
        QueueReadingAids();
    }

    private void QueueReadingAids()
    {
        if (_disposed || _diffPreview || _updatingReadingAids || !IsLoaded) return;
        if (_readingTimer == null)
        {
            _readingTimer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(120) };
            _readingTimer.Tick += ReadingTimer_Tick;
        }
        // Coalesce updates, but do not starve the refresh during continuous scrolling.
        if (!_readingTimer.IsEnabled) _readingTimer.Start();
    }

    private void ReadingTimer_Tick(object sender, object args)
    {
        _readingTimer.Stop();
        if (_disposed || !IsLoaded || !IsEnabled || _movingToWindow) return;
        // A native worker may hold a read lease. Refresh after it has drained.
        if (Native.ReadOnly) { QueueReadingAids(); return; }
        RefreshReadingAids();
    }

    private void ReadingAids_Unloaded(object sender, RoutedEventArgs args) => _readingTimer?.Stop();
    private void ReadingAids_Loaded(object sender, RoutedEventArgs args) => QueueReadingAids();

    internal void RefreshReadingAids()
    {
        if (_disposed || _diffPreview || Native.ReadOnly) return;
        _updatingReadingAids = true;
        try
        {
            var aids = ApplicationPreferences.EditorReadingAids;
            var caret = Native.CurrentPos;
            var anchor = Native.Anchor;
            var start = Math.Min(caret, anchor);
            var end = Math.Max(caret, anchor);
            var sampleRange = aids.HasFlag(ReadingAids.Occurrences) && end > start && end - start <= 256 && Native.Selections == 1
                ? GetOccurrenceSampleRange() : (0L, 0L);
            // Width-only reflow can expose different document lines without changing
            // the display row count or scroll position. Cache the range actually read.
            var state = (ContentVersion, caret, anchor, sampleRange.Item1, sampleRange.Item2, aids);
            var stateChanged = _readingState != state;
            if (!stateChanged && !_braceStylesChanged) return;
            _braceStylesChanged = false;
            if (stateChanged)
            {
                _readingState = state;
                Native.IndicatorCurrent = OccurrenceIndicator;
                // Indicator runs move with edits, so clear by document range (a run-tree operation).
                Native.IndicatorClearRange(0, Native.Length);
                if (sampleRange.Item2 > sampleRange.Item1) HighlightOccurrences(start, end, sampleRange);
            }
            // Styling can finish after the text/selection update. Refresh the pair
            // without rescanning occurrences or mutating document/selection state.
            var pair = aids.HasFlag(ReadingAids.Braces) ? FindBracePair() : (-1L, -1L);
            Native.BraceHighlight(pair.Item1, pair.Item2);
            _highlightedBracePair = pair;
        }
        finally { _updatingReadingAids = false; }
    }

    private (long Start, long End) GetOccurrenceSampleRange()
    {
        var firstVisible = Native.FirstVisibleLine;
        var first = Native.DocLineFromVisible(firstVisible);
        var last = Native.DocLineFromVisible(firstVisible + Native.LinesOnScreen + 1);
        var from = Native.PositionFromLine(Math.Max(0, first));
        var length = Native.Length;
        var to = last < 0 || last + 1 >= Native.LineCount ? length : Native.PositionFromLine(last + 1);
        to = Math.Min(to, from + ReadingSampleBytes);
        while (to < length && (Native.GetCharAt(to) & 0xc0) == 0x80) to--;
        return (from, to);
    }

    private void HighlightOccurrences(long start, long end, (long Start, long End) sampleRange)
    {
        var query = ReadRange(start, end);
        if (string.IsNullOrWhiteSpace(query) || query.AsSpan().IndexOfAny('\r', '\n', '\0') >= 0) return;
        var sample = ReadRange(sampleRange.Start, sampleRange.End);
        int offset = 0, count = 0;
        long byteOffset = sampleRange.Start;
        while (offset < sample.Length && count < 1000)
        {
            var found = sample.AsSpan(offset).IndexOf(query.AsSpan(), StringComparison.Ordinal);
            if (found < 0) break;
            byteOffset += Encoding.UTF8.GetByteCount(sample.AsSpan(offset, found));
            var bytes = Encoding.UTF8.GetByteCount(query);
            Native.IndicatorFillRange(byteOffset, bytes);
            offset += found + query.Length;
            byteOffset += bytes;
            count++;
        }
    }

    // Bounded, style-aware ASCII brace scan. A missing pair can mean the budget
    // was exhausted, so never paint a false syntax-error indication.
    private (long, long) FindBracePair()
    {
        var position = Native.CurrentPos;
        if (position > 0 && IsBrace(Native.GetCharAt(position - 1))) position--;
        if (position >= Native.Length || !IsBrace(Native.GetCharAt(position))) return (-1, -1);
        var brace = Native.GetCharAt(position);
        var opening = brace is '(' or '[' or '{';
        var opposite = brace switch { '(' => ')', ')' => '(', '[' => ']', ']' => '[', '{' => '}', _ => '{' };
        var start = opening ? position + 1 : Math.Max(0, position - ReadingSampleBytes);
        while (start < Native.Length && (Native.GetCharAt(start) & 0xc0) == 0x80) start++;
        var length = (int)Math.Min(ReadingSampleBytes, opening ? Native.Length - start : position - start);
        while (start + length < Native.Length && (Native.GetCharAt(start + length) & 0xc0) == 0x80) length--;
        if (length <= 0) return (-1, -1);
        var bytes = Encoding.UTF8.GetBytes(ReadRange(start, start + length));
        var style = Native.GetStyleAt(position);
        var depth = 1;
        var candidates = 0;
        for (var i = opening ? 0 : bytes.Length - 1; i >= 0 && i < bytes.Length; i += opening ? 1 : -1)
        {
            if (bytes[i] != brace && bytes[i] != opposite) continue;
            if (++candidates > 4096) break;
            var at = start + i;
            if (at <= Native.EndStyled && Native.GetStyleAt(at) != style) continue;
            depth += bytes[i] == brace ? 1 : -1;
            if (depth == 0) return (position, at);
        }
        return (-1, -1);
    }

    private static bool IsBrace(int ch) => ch is '(' or ')' or '[' or ']' or '{' or '}';

}
