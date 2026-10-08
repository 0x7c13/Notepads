// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Globalization;
using System.Text;
using System.Threading.Tasks;
using Notepads.Features.Preferences;
using Notepads.Features.WebSearch;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Presentation.Input;
using Windows.System;
using Windows.UI.Xaml;

namespace Notepads.Presentation.Controls.TextEditor;

public sealed partial class TextEditorCore
{
    private KeyboardCommandHandler GetKeyboardCommandHandler()
    {
        return new KeyboardCommandHandler(
        [
            new(VirtualKeyModifiers.Control, VirtualKey.Z, () => Undo()),
            new(VirtualKeyModifiers.Control | VirtualKeyModifiers.Shift, VirtualKey.Z, () => Redo()),
            new(VirtualKeyModifiers.Control, VirtualKey.C, () => CopyTextToWindowsClipboardRequested?.Invoke(this, EventArgs.Empty)),
            new(VirtualKeyModifiers.Control, VirtualKey.X, () => CutSelectedTextToWindowsClipboardRequested?.Invoke(this, EventArgs.Empty)),
            new(VirtualKeyModifiers.Control, VirtualKey.V, async () => await PastePlainTextFromWindowsClipboardAsync(null)),
            new(VirtualKeyModifiers.Shift, VirtualKey.Insert, async () => await PastePlainTextFromWindowsClipboardAsync(null)),
            new(VirtualKeyModifiers.Control, VirtualKey.Insert, () => CopyTextToWindowsClipboardRequested?.Invoke(this, EventArgs.Empty)),
            new(VirtualKeyModifiers.Shift, VirtualKey.Delete, () => CutSelectedTextToWindowsClipboardRequested?.Invoke(this, EventArgs.Empty)),
            new(VirtualKeyModifiers.Menu, VirtualKey.Z, () => TextWrapping = TextWrapping == TextWrapping.NoWrap ? TextWrapping.Wrap : TextWrapping.NoWrap),
            // Reserve the old reading-order shortcuts until default-RTL layout
            // is implemented. Ctrl+L would otherwise cut the current line.
            new(VirtualKeyModifiers.Control, VirtualKey.L, () => { }),
            new(VirtualKeyModifiers.Control, VirtualKey.R, () => { }),
            new(VirtualKeyModifiers.Control, VirtualKey.Add, () => IncreaseFontSize(0.1)),
            new(VirtualKeyModifiers.Control, (VirtualKey)187, () => IncreaseFontSize(0.1)),
            new(VirtualKeyModifiers.Control, VirtualKey.Subtract, () => DecreaseFontSize(0.1)),
            new(VirtualKeyModifiers.Control, (VirtualKey)189, () => DecreaseFontSize(0.1)),
            new(VirtualKeyModifiers.Control, VirtualKey.Number0, () => ResetFontSizeToDefault()),
            new(VirtualKeyModifiers.Control, VirtualKey.NumberPad0, () => ResetFontSizeToDefault()),
            new(VirtualKeyModifiers.None, VirtualKey.F5, () => TypeText(DateTime.Now.ToString(CultureInfo.CurrentCulture))),
            new(VirtualKeyModifiers.Control, VirtualKey.E, async () => await SearchInWebAsync()),
            new(VirtualKeyModifiers.Control, VirtualKey.D, () => Native.SelectionDuplicate()),
            new(VirtualKeyModifiers.Control, VirtualKey.J, () => JoinText()),
            new(VirtualKeyModifiers.None, VirtualKey.Tab, () => ChangeIndentation(false)),
            new(VirtualKeyModifiers.Shift, VirtualKey.Tab, () => ChangeIndentation(true)),
            new(VirtualKeyModifiers.None, VirtualKey.Back, () => DeleteBackWithUnindent()),
            new(VirtualKeyModifiers.Menu, VirtualKey.Up, () => Native.MoveSelectedLinesUp()),
            new(VirtualKeyModifiers.Menu, VirtualKey.Down, () => Native.MoveSelectedLinesDown()),
            new(VirtualKeyModifiers.Menu, VirtualKey.Left, () => MoveWords(false)),
            new(VirtualKeyModifiers.Menu, VirtualKey.Right, () => MoveWords(true)),
            new(VirtualKeyModifiers.None, VirtualKey.Enter, () => EnterWithAutoIndentation()),
            new(VirtualKeyModifiers.Shift, VirtualKey.Enter, () => EnterWithAutoIndentation()),
        ]);
    }

    private void JoinText()
    {
        var first = Native.LineFromPosition(Native.SelectionStart);
        var last = Native.LineFromPosition(Native.SelectionEnd);
        if (last > first && Native.SelectionEnd == Native.PositionFromLine(last)) last--;
        if (first == last) return;
        var start = Native.PositionFromLine(first);
        var end = Native.GetLineEndPosition(last);
        Native.SetTargetRange(start, end);
        Native.LinesJoin();
        Native.SetSel(start, Native.TargetEnd);
        Native.ScrollCaret();
    }

    private void MoveWords(bool right)
    {
        var start = Native.SelectionStart;
        var end = Native.SelectionEnd;
        if (start == end)
        {
            start = Native.WordStartPosition(start, true);
            end = Native.WordEndPosition(end, true);
        }
        if (start == end) return;
        long otherStart, otherEnd;
        if (right)
        {
            otherStart = end;
            while (otherStart < Native.Length && Native.WordEndPosition(otherStart, true) == otherStart)
                otherStart = Native.PositionAfter(otherStart);
            otherEnd = Native.WordEndPosition(otherStart, true);
        }
        else
        {
            otherEnd = start;
            while (otherEnd > 0 && Native.WordStartPosition(otherEnd, true) == otherEnd)
                otherEnd = Native.PositionBefore(otherEnd);
            otherStart = Native.WordStartPosition(otherEnd, true);
        }
        if (otherStart == otherEnd) return;
        var word = ReadRange(start, end);
        var other = ReadRange(otherStart, otherEnd);
        var separator = right ? ReadRange(end, otherStart) : ReadRange(otherEnd, start);
        var rangeStart = right ? start : otherStart;
        var rangeEnd = right ? otherEnd : end;
        ReplaceRange(rangeStart, rangeEnd, right ? other + separator + word : word + separator + other);
        var selectionStart = right ? rangeStart + Encoding.UTF8.GetByteCount(other + separator) : rangeStart;
        Native.SetSel(selectionStart, selectionStart + Encoding.UTF8.GetByteCount(word));
    }

    private bool _hasAddedLogEntry;
    public void TryInsertNewLogEntry()
    {
        if (_hasAddedLogEntry || Native.Length < 4 || Native.GetCharAt(0) != '.'
            || Native.GetCharAt(1) != 'L' || Native.GetCharAt(2) != 'O' || Native.GetCharAt(3) != 'G')
        {
            return;
        }

        _hasAddedLogEntry = true;
        Native.SetSel(Native.Length, Native.Length);
        TypeText("\r" + DateTime.Now.ToString("h:mm tt M/dd/yyyy") + "\r");
    }

    public async Task SearchInWebAsync()
    {
        try
        {
            if (!HasSelection) return;

            var text = GetSelectedText().Trim();
            if (text.Length > 2000) text = text.Substring(0, 2000);

            if (Uri.TryCreate(text, UriKind.Absolute, out var url) &&
                (url.Scheme == Uri.UriSchemeHttp || url.Scheme == Uri.UriSchemeHttps))
            {
                await Launcher.LaunchUriAsync(url);
            }
            else
            {
                var searchUrl = SearchUrls.GetSearchUrlBySearchEngine(ApplicationPreferences.EditorDefaultSearchEngine, ApplicationPreferences.EditorCustomMadeSearchUrl);
                await Launcher.LaunchUriAsync(new Uri(string.Format(searchUrl, Uri.EscapeDataString(text))));
            }
        }
        catch (Exception ex)
        {
            LoggingService.LogError($"[{nameof(TextEditorCore)}] Failed to open search link: {ex.Message}");
        }
    }
}
