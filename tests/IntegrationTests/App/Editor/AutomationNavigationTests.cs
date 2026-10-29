// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Text;
using System.Threading.Tasks;
using Notepads.Presentation.Controls.TextEditor;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Automation.Provider;
using Windows.UI.Xaml.Automation.Text;
using Windows.UI.Xaml.Controls;
using static NotepadsEditorTests.EditorTestLayout;

namespace NotepadsEditorTests;

internal static class AutomationNavigationTests
{
    public static async Task RunAsync(StringBuilder log, TextEditorCore core)
    {
        var view = (WinUIEditor.EditorBaseControl)((Grid)core.Content).Children[0];
        var native = view.Editor;
        var peer = FrameworkElementAutomationPeer.CreatePeerForElement(view);
        var provider = (ITextProvider)peer.GetPattern(PatternInterface.Text);
        var standardEditType = new TextBoxAutomationPeer(new TextBox()).GetLocalizedControlType();
        Check(!string.IsNullOrEmpty(standardEditType) && peer.GetLocalizedControlType() == standardEditType,
            "editor uses the platform's localized Edit control type");
        var width = core.Width;
        var wrapping = core.TextWrapping;
        try
        {
            core.TextWrapping = TextWrapping.NoWrap;
            await core.LoadTextAsync(string.Empty);
            CheckUnits(core, native, provider, TextUnit.Word, ["  ", "alpha,\t", "beta  ", "你好 😀 ", "end "]);
            CheckUnits(core, native, provider, TextUnit.Word, ["😀,\t\r "]);
            CheckUnits(core, native, provider, TextUnit.Word, ["hello"]);
            CheckUnits(core, native, provider, TextUnit.Line, ["alpha\r", "\r", "你好 😀\r", "end"]);
            CheckUnits(core, native, provider, TextUnit.Line, ["alpha\r", "\r"]);
            CheckUnits(core, native, provider, TextUnit.Paragraph, ["alpha\r", "\r", "你好 😀\r", "end"]);
            CheckNormalization(core, native, provider);
            CheckFallbacks(core, provider);
            CheckEmpty(core, provider);
            CheckStaleOffset(core, provider);
            CheckRawLineEndings(core, native, provider);
            CheckTextSearch(core, native, provider);
            CheckLegacyTextSearch();
            await CheckImmediateWrappingAsync(core, native, provider);
            await CheckPendingSyntaxWrappingAsync(core, native, provider);

            core.TextWrapping = TextWrapping.Wrap;
            foreach (var testWidth in new[] { 220.0, 420.0 })
            {
                var text = string.Concat(System.Linq.Enumerable.Repeat("alpha 你好 😀 beta, ", 15)) + "\r\rfinal";
                await AfterPaintAsync(native, () =>
                {
                    core.Width = testWidth;
                    core.SetText(text);
                    core.UpdateLayout();
                });
                Check(native.WrapCount(0) > 1, "fixture has wrapped visual lines");

                // The oracle is rendered character Y coordinates, independent
                // of the provider's layout-range implementation.
                var boundaries = new List<long> { 0 };
                var previousY = native.PointYFromPosition(0);
                for (long position = native.PositionAfter(0); position < native.Length; position = native.PositionAfter(position))
                {
                    var y = native.PointYFromPosition(position);
                    if (y != previousY) boundaries.Add(position);
                    previousY = y;
                }
                boundaries.Add(native.Length);
                var bytes = Encoding.UTF8.GetBytes(text);
                var units = new string[boundaries.Count - 1];
                for (var i = 0; i < units.Length; ++i)
                    units[i] = Encoding.UTF8.GetString(bytes, (int)boundaries[i], (int)(boundaries[i + 1] - boundaries[i]));
                CheckPartition(native, provider, TextUnit.Line, units);

                var paragraph = At(provider, 2);
                paragraph.ExpandToEnclosingUnit(TextUnit.Paragraph);
                Equal(text[..(text.IndexOf('\r') + 1)], paragraph.GetText(-1), "paragraph remains a complete logical line when wrapped");
                var firstLine = At(provider, 2);
                firstLine.ExpandToEnclosingUnit(TextUnit.Line);
                Check(firstLine.GetText(-1).Length < paragraph.GetText(-1).Length, "visual line is smaller than its logical paragraph");
            }
        }
        finally
        {
            core.Width = width;
            core.TextWrapping = wrapping;
            core.UpdateLayout();
        }
        log.AppendLine("PASS: UIA word, visual-line and paragraph navigation, trailing separators, Unicode, empty/EOF boundaries, normalization, endpoint crossing, supported-unit fallback, wrapped layout, bounded text search and unchanged editor state.");
    }

    private static void CheckTextSearch(TextEditorCore core, WinUIEditor.Editor native, ITextProvider provider)
    {
        const string text = "first Äpfel 中文 😀\rsecond äPFEL 中文 😀\rliteral a.b\0tail";
        core.SetText(text);
        native.SetSel(2, 5);
        native.TargetStart = 3;
        native.TargetEnd = 7;
        native.SearchFlags = WinUIEditor.FindOption.WholeWord | WinUIEditor.FindOption.RegExp;
        var state = (native.CurrentPos, native.Anchor, native.TargetStart, native.TargetEnd,
            native.SearchFlags, native.FirstVisibleLine, native.XOffset, native.CanUndo(), native.Modify);
        var document = provider.DocumentRange;
        var unchanged = document.Clone();

        Match(document, "Äpfel", false, false, "Äpfel", text.IndexOf("Äpfel", StringComparison.Ordinal));
        Match(document, "äpfel", true, true, "äPFEL", text.IndexOf("äPFEL", StringComparison.Ordinal));
        Match(document, "äpfel", false, true, "Äpfel", text.IndexOf("Äpfel", StringComparison.Ordinal));
        Match(document, "中文 😀", true, false, "中文 😀", text.LastIndexOf("中文 😀", StringComparison.Ordinal));
        Match(document, "😀\rsecond", false, false, "😀\rsecond", text.IndexOf("😀", StringComparison.Ordinal));
        Match(document, "a.b\0tail", false, false, "a.b\0tail", text.IndexOf("a.b", StringComparison.Ordinal));
        Match(document, "\0", true, false, "\0", text.IndexOf('\0'));
        Check(document.FindText("not present", false, false) == null, "UIA missing text returns no range");
        Check(document.FindText("ÄPFEL", false, false) == null, "UIA case-sensitive search preserves case");
        Check(At(provider, 0).FindText("first", false, false) == null, "UIA empty range cannot match text");
        var partial = At(provider, 6, 4);
        Check(partial.FindText("Äpfel", false, true) == null, "UIA match cannot cross the range end");
        Check(partial.FindText("first", true, true) == null, "UIA backward match cannot escape the range start");
        Match(partial, "Äpfe", true, false, "Äpfe", 6);
        var rejectedEmpty = false;
        try { document.FindText(string.Empty, false, false); }
        catch (ArgumentException) { rejectedEmpty = true; }
        Check(rejectedEmpty, "UIA rejects an empty search string");
        Check(document.Compare(unchanged), "UIA search does not modify its source range");
        Check(state == (native.CurrentPos, native.Anchor, native.TargetStart, native.TargetEnd,
            native.SearchFlags, native.FirstVisibleLine, native.XOffset, native.CanUndo(), native.Modify),
            "UIA search preserves selection, target, flags, viewport and edit state");
        Equal(text, core.GetText(), "UIA search preserves document bytes");

        foreach (var backward in new[] { false, true })
        foreach (var ignoreCase in new[] { false, true })
        {
            core.SetText("abcd");
            var bounded = At(provider, 0, 3);
            Check(bounded.FindText("cd", backward, ignoreCase) == null,
                "UIA candidate near range end cannot match text outside the range");
            Match(bounded, "bc", backward, ignoreCase, "bc", 1);
            Match(bounded, "c", backward, ignoreCase, "c", 2);
            core.SetText("abc");
            Check(provider.DocumentRange.FindText("c\0", backward, ignoreCase) == null,
                "UIA search cannot treat the byte after EOF as a real NUL");
            core.SetText("abc\0");
            Match(provider.DocumentRange, "c\0", backward, ignoreCase, "c\0", 2);
            core.SetText("甲乙丙");
            bounded = At(provider, 0, 2);
            Check(bounded.FindText("乙丙", backward, ignoreCase) == null,
                "UIA multibyte match cannot extend past the range end");
            Match(bounded, "甲乙", backward, ignoreCase, "甲乙", 0);
        }

        core.SetText("one 中文 😀");
        var multipleSelection = native.MultipleSelection;
        native.MultipleSelection = true;
        try
        {
            native.SetSelection(3, 0);
            native.AddSelection(10, 4);
            native.AddSelection(15, 11);
            var selections = provider.GetSelection();
            Check(selections.Length == 3, "UIA returns all disjoint selections");
            Equal("one", selections[0].GetText(-1), "UIA first selection");
            Equal("中文", selections[1].GetText(-1), "UIA second selection");
            Equal("😀", selections[2].GetText(-1), "UIA third selection");
        }
        finally
        {
            native.SetSel(0, 0);
            native.MultipleSelection = multipleSelection;
        }

        core.SetText("abc");
        var stale = provider.DocumentRange;
        core.SetText("x😀B");
        Check(stale.FindText("😀", false, false) == null, "UIA stale range ending inside UTF-8 cannot expose a partial match");
        core.SetText("abc");
        Match(document, "bc", true, false, "bc", 1);
        core.SetText("\uFFFD");
        foreach (var backward in new[] { false, true })
        foreach (var ignoreCase in new[] { false, true })
        {
            Check(provider.DocumentRange.FindText("\uD800", backward, ignoreCase) == null,
                "UIA malformed UTF-16 query must not match a replacement character");
            Match(provider.DocumentRange, "\uFFFD", backward, ignoreCase, "\uFFFD", 0);
        }
        core.SetText(string.Empty);
        Check(document.FindText("a", true, false) == null, "UIA stale range clamps to an empty document");
        native.SearchFlags = WinUIEditor.FindOption.None;

        void Match(ITextRangeProvider range, string query, bool backward, bool ignoreCase, string expected, int utf16Offset)
        {
            var result = range.FindText(query, backward, ignoreCase);
            Check(result != null, "UIA text match exists: " + query);
            Equal(expected, result.GetText(-1), "UIA returns the exact matched text");
            var prefix = core.GetText()[..utf16Offset];
            var position = At(provider, CharacterCount(prefix));
            Check(result.CompareEndpoints(TextPatternRangeEndpoint.Start, position, TextPatternRangeEndpoint.Start) == 0,
                "UIA match uses document UTF-8 coordinates");
            Check(result.CompareEndpoints(TextPatternRangeEndpoint.End, position, TextPatternRangeEndpoint.Start) > 0 &&
                position.CompareEndpoints(TextPatternRangeEndpoint.Start, result, TextPatternRangeEndpoint.End) < 0,
                "UIA endpoint comparison preserves ordering in both directions");
        }
    }

    private static void CheckLegacyTextSearch()
    {
        // Exercise the generic control without changing Notepads' UTF-8 host.
        var view = new WinUIEditor.EditorBaseControl();
        var native = view.Editor;
        native.CodePage = 0;
        native.StyleSetCharacterSet((int)WinUIEditor.StylesCommon.Default, WinUIEditor.CharacterSet.Ansi);
        byte[] bytes = [(byte)'?', (byte)'A', 0, 0xE9, (byte)'?', (byte)'A'];
        native.AddTextFromBuffer(bytes.Length, bytes.AsBuffer());
        var provider = (ITextProvider)FrameworkElementAutomationPeer.CreatePeerForElement(view).GetPattern(PatternInterface.Text);
        var document = provider.DocumentRange;
        var state = (native.CurrentPos, native.Anchor, native.TargetStart, native.TargetEnd,
            native.SearchFlags, native.CanUndo(), native.Modify);
        foreach (var backward in new[] { false, true })
        foreach (var ignoreCase in new[] { false, true })
        {
            Check(document.FindText("漢", backward, ignoreCase) == null,
                "UIA CP1252 query must not match a substituted question mark");
            Check(document.FindText("\uFF21", backward, ignoreCase) == null,
                "UIA CP1252 query must not match a best-fit ASCII letter");
            Check(document.FindText("😀", backward, ignoreCase) == null,
                "UIA CP1252 query must not substitute a supplementary character");
            Match("?", backward ? 4 : 0, 1);
            Match("A", backward ? 5 : 1, 1);
            Match("é", 3, 1);
            Match("A\0é", 1, 3);
            if (ignoreCase) Match("É", 3, 1);
            else Check(document.FindText("É", backward, false) == null, "UIA CP1252 search preserves case");

            void Match(string query, int start, int length)
            {
                var result = document.FindText(query, backward, ignoreCase);
                Check(result != null && result.Compare(At(provider, start, length)),
                    "UIA CP1252 query returns the exact expected byte range");
            }
        }
        Check(state == (native.CurrentPos, native.Anchor, native.TargetStart, native.TargetEnd,
            native.SearchFlags, native.CanUndo(), native.Modify), "UIA legacy search preserves editor state");
        Check(native.Length == bytes.Length, "UIA legacy search preserves document length");
        for (var index = 0; index < bytes.Length; ++index)
            Check(unchecked((byte)native.GetCharAt(index)) == bytes[index], "UIA legacy search preserves document bytes");
        GC.KeepAlive(view);
    }

    private static void CheckUnits(TextEditorCore core, WinUIEditor.Editor native, ITextProvider provider, TextUnit unit, string[] units)
    {
        core.SetText(string.Concat(units));
        core.UpdateLayout();
        CheckPartition(native, provider, unit, units);
    }

    private static void CheckPartition(WinUIEditor.Editor native, ITextProvider provider, TextUnit unit, string[] units)
    {
        native.SetSel(0, native.PositionAfter(0));
        var caret = native.CurrentPos;
        var anchor = native.Anchor;
        var offset = 0;
        for (var index = 0; index < units.Length; ++index)
        {
            var characters = CharacterCount(units[index]);
            for (var within = 0; within < characters; ++within)
            {
                var expanded = At(provider, offset + within);
                expanded.ExpandToEnclosingUnit(unit);
                Equal(units[index], expanded.GetText(-1), unit + " expansion and exact boundary ownership");
            }
            var forward = At(provider, offset);
            forward.ExpandToEnclosingUnit(unit);
            var original = forward.Clone();
            Check(forward.Move(unit, 0) == 0 && forward.Compare(original), unit + " zero count preserves range");
            Check(forward.Move(unit, int.MaxValue) == units.Length - index - 1, unit + " positive movement clamps count");
            Equal(units[^1], forward.GetText(-1), unit + " movement retains final nonempty unit");
            original = forward.Clone();
            Check(forward.Move(unit, 1) == 0 && forward.Compare(original), unit + " final range cannot move beyond EOF");
            Check(forward.Move(unit, int.MinValue) == -(units.Length - 1), unit + " negative movement clamps count");
            Equal(units[0], forward.GetText(-1), unit + " movement reaches first unit");

            var empty = At(provider, offset);
            Check(empty.Move(unit, 1) == 1 && empty.GetText(-1) == "", unit + " degenerate movement remains empty");
            var next = At(provider, offset + characters);
            Check(empty.Compare(next), unit + " degenerate movement reaches following boundary");
            Check(empty.Move(unit, -1) == -1 && empty.Compare(At(provider, offset)), unit + " reverse movement reaches preceding boundary");
            offset += characters;
        }
        var eof = At(provider, offset);
        var eofOriginal = eof.Clone();
        eof.ExpandToEnclosingUnit(unit);
        Equal(unit != TextUnit.Word && (units[^1].EndsWith('\r') || units[^1].EndsWith('\n')) ? "" : units[^1], eof.GetText(-1),
            unit + " EOF expansion preserves final word or logical line");
        eof = eofOriginal.Clone();
        Check(eof.Move(unit, 1) == 0 && eof.Compare(eofOriginal), unit + " degenerate movement stops at EOF");
        Check(eof.MoveEndpointByUnit(TextPatternRangeEndpoint.Start, unit, int.MinValue) == -units.Length,
            unit + " backward endpoint count includes final unit");
        Equal(string.Concat(units), eof.GetText(-1), unit + " backward endpoint spans entire document");
        Check(eof.MoveEndpointByUnit(TextPatternRangeEndpoint.Start, unit, int.MaxValue) == units.Length,
            unit + " forward endpoint count includes final unit");
        Check(eof.Compare(eofOriginal), unit + " endpoint returns to EOF");
        Check(native.CurrentPos == caret && native.Anchor == anchor && native.Selections == 1,
            unit + " navigation never moves the editor selection");
    }

    private static void CheckNormalization(TextEditorCore core, WinUIEditor.Editor native, ITextProvider provider)
    {
        core.SetText("alpha beta gamma");
        var range = At(provider, 7, 2); // "et" inside beta
        Check(range.Move(TextUnit.Word, 1) == 1, "word normalization is not included in movement count");
        Equal("gamma", range.GetText(-1), "positive move normalizes around original start");
        range = At(provider, 7, 2);
        Check(range.Move(TextUnit.Word, -1) == -1, "negative word movement count");
        Equal("alpha ", range.GetText(-1), "negative move normalizes around original start");
        range = At(provider, 1, 2);
        var original = range.Clone();
        Check(range.Move(TextUnit.Word, -1) == 0 && range.Compare(original), "failed move preserves unnormalized original range");
        range = At(provider, 12, 2);
        original = range.Clone();
        Check(range.Move(TextUnit.Word, 1) == 0 && range.Compare(original), "failed final move preserves original subrange");

        range = At(provider, 7, 2);
        Check(range.MoveEndpointByUnit(TextPatternRangeEndpoint.Start, TextUnit.Word, 1) == 1,
            "start endpoint from inside word reaches next boundary");
        Check(range.Compare(At(provider, 11)), "start crossing end collapses at moved start");
        range = At(provider, 7, 2);
        Check(range.MoveEndpointByUnit(TextPatternRangeEndpoint.End, TextUnit.Word, -1) == -1,
            "end endpoint from inside word reaches current start");
        Check(range.Compare(At(provider, 6)), "end crossing start collapses at moved end");

        core.SetText("你😀x");
        range = provider.DocumentRange;
        Check(range.Move(TextUnit.Character, int.MaxValue) == 2, "character movement counts Unicode scalars");
        Equal("x", range.GetText(-1), "character movement keeps last scalar");
        Check(range.Move(TextUnit.Character, -1) == -1, "reverse character movement count");
        Equal("😀", range.GetText(-1), "character range does not split emoji");
        Check(native.Length == Encoding.UTF8.GetByteCount("你😀x"), "navigation leaves native document unchanged");
    }

    private static void CheckFallbacks(TextEditorCore core, ITextProvider provider)
    {
        core.SetText("alpha beta\rfinal");
        var format = At(provider, 2);
        format.ExpandToEnclosingUnit(TextUnit.Format);
        Equal("alpha ", format.GetText(-1), "unsupported Format promotes to Word");
        Check(format.Move(TextUnit.Format, 1) == 1, "Format movement uses Word boundaries");
        Equal("beta\r", format.GetText(-1), "Format movement includes separators");
        foreach (var unit in new[] { TextUnit.Page, TextUnit.Document })
        {
            var range = At(provider, 2, 3);
            var original = range.Clone();
            Check(range.Move(unit, 1) == 0 && range.Compare(original), unit + " cannot move a nonempty document unit");
            range.ExpandToEnclosingUnit(unit);
            Check(range.Compare(provider.DocumentRange), unit + " expands to document");
            range = At(provider, 2);
            Check(range.MoveEndpointByUnit(TextPatternRangeEndpoint.End, unit, int.MaxValue) == 1,
                unit + " positive endpoint stops at document end");
            Equal("pha beta\rfinal", range.GetText(-1), unit + " endpoint keeps original start");
            Check(range.MoveEndpointByUnit(TextPatternRangeEndpoint.End, unit, int.MinValue) == -1,
                unit + " negative endpoint stops at document start");
            Check(range.Compare(At(provider, 0)), unit + " endpoint crossing collapses range");
        }
    }

    private static void CheckEmpty(TextEditorCore core, ITextProvider provider)
    {
        core.SetText(string.Empty);
        foreach (var unit in new[] { TextUnit.Character, TextUnit.Word, TextUnit.Format, TextUnit.Line, TextUnit.Paragraph, TextUnit.Page, TextUnit.Document })
        {
            var range = provider.DocumentRange;
            range.ExpandToEnclosingUnit(unit);
            Check(range.GetText(-1) == "" && range.Move(unit, int.MaxValue) == 0 && range.Move(unit, int.MinValue) == 0,
                unit + " empty document range");
            Check(range.MoveEndpointByUnit(TextPatternRangeEndpoint.Start, unit, int.MaxValue) == 0 &&
                range.MoveEndpointByUnit(TextPatternRangeEndpoint.End, unit, int.MinValue) == 0, unit + " empty endpoints");
        }
    }

    private static void CheckStaleOffset(TextEditorCore core, ITextProvider provider)
    {
        core.SetText("abcd");
        var stale = At(provider, 2);
        var staleCharacter = stale.Clone();
        core.SetText("😀 beta");
        stale.ExpandToEnclosingUnit(TextUnit.Word);
        Equal("😀 ", stale.GetText(-1), "word expansion aligns an obsolete byte offset to a Unicode boundary");
        staleCharacter.ExpandToEnclosingUnit(TextUnit.Character);
        Equal("😀", staleCharacter.GetText(-1), "character expansion aligns an obsolete byte offset to a Unicode boundary");
    }

    private static void CheckRawLineEndings(TextEditorCore core, WinUIEditor.Editor native, ITextProvider provider)
    {
        foreach (var ending in new[] { "\r\n", "\n" })
        {
            string[] units = ["alpha" + ending, ending, "你好 😀" + ending, "end" + ending];
            // Bypass the application's intentional CR normalization.
            native.SetText(string.Concat(units));
            Equal(string.Concat(units), provider.DocumentRange.GetText(-1), "raw line endings reach native storage unchanged");
            core.UpdateLayout();
            CheckPartition(native, provider, TextUnit.Line, units);
            CheckPartition(native, provider, TextUnit.Paragraph, units);
        }

        native.SetText("a\r\n");
        var character = At(provider, 1);
        character.ExpandToEnclosingUnit(TextUnit.Character);
        Equal("\r", character.GetText(-1), "character expansion uses scalar boundaries within CRLF");
        var range = At(provider, 1, 2);
        Check(range.Move(TextUnit.Character, int.MaxValue) == 1, "CRLF EOF movement reports the actual scalar destination");
        Equal("\n", range.GetText(-1), "CRLF EOF movement reaches the final scalar");
        var original = range.Clone();
        Check(range.Move(TextUnit.Character, int.MaxValue) == 0 && range.Compare(original),
            "CRLF final scalar reports no movement when endpoints stay unchanged");
        Check(range.Move(TextUnit.Character, int.MinValue) == -2, "CRLF reverse movement counts CR and LF consistently");
        Equal("a", range.GetText(-1), "CRLF reverse movement reaches document start");
    }

    private static async Task CheckImmediateWrappingAsync(TextEditorCore core, WinUIEditor.Editor native, ITextProvider provider)
    {
        var text = string.Concat(System.Linq.Enumerable.Repeat("alpha 你好 😀 beta, ", 15));
        await AfterPaintAsync(native, () =>
        {
            core.Width = 420;
            core.TextWrapping = TextWrapping.NoWrap;
            core.SetText(text);
            core.UpdateLayout();
        });

        string FirstLine()
        {
            var range = At(provider, 0);
            range.ExpandToEnclosingUnit(TextUnit.Line);
            return range.GetText(-1);
        }

        string wide = null;
        await AfterPaintAsync(native, () =>
        {
            core.TextWrapping = TextWrapping.Wrap;
            wide = FirstLine(); // Deliberately no yield/paint after changing mode.
            Check(wide.Length < text.Length, "wrap enable applies to the first synchronous UIA query");
        });
        VerifyPaintedLine(wide);

        string narrow = null;
        await AfterPaintAsync(native, () =>
        {
            core.Width = 220;
            core.UpdateLayout(); // Gives the native control its new size.
            narrow = FirstLine();
            Check(narrow.Length < wide.Length, "resize applies before the next native paint");
        });
        VerifyPaintedLine(narrow);

        await AfterPaintAsync(native, () =>
        {
            core.TextWrapping = TextWrapping.NoWrap;
            Equal(text, FirstLine(), "wrap disable applies before the next native paint");
        });
        await AfterPaintAsync(native, () =>
        {
            core.TextWrapping = TextWrapping.Wrap;
            Equal(narrow, FirstLine(), "wrap re-enable immediately restores current-width boundaries");
        });
        VerifyPaintedLine(narrow);

        await AfterPaintAsync(native, () =>
        {
            core.Width = 420;
            core.UpdateLayout();
            Equal(wide, FirstLine(), "widening immediately restores wider line boundaries");
        });
        VerifyPaintedLine(wide);

        void VerifyPaintedLine(string immediate)
        {
            var y = native.PointYFromPosition(0);
            long end = 0;
            while (end < native.Length && native.PointYFromPosition(end) == y)
                end = native.PositionAfter(end);
            var bytes = Encoding.UTF8.GetBytes(text);
            Equal(Encoding.UTF8.GetString(bytes, 0, (int)end), immediate,
                "synchronous UIA boundaries agree with subsequent rendered character positions");
        }
    }

    private static async Task CheckPendingSyntaxWrappingAsync(TextEditorCore core, WinUIEditor.Editor native, ITextProvider provider)
    {
        var profile = SyntaxLanguageProfile.Create("csharp", "navigation.cs");
        try
        {
            await AfterPaintAsync(native, () =>
            {
                core.Width = 220;
                core.TextWrapping = TextWrapping.Wrap;
                core.SetText(string.Empty);
                native.SetLexerLanguage(profile.Lexer, profile.Keywords, profile.PropertyNames, profile.PropertyValues);
                core.UpdateLayout();
            });

            var text = string.Concat(System.Linq.Enumerable.Repeat("abc+123*functionName(\"text\");", 20));
            for (var edit = 0; edit < 2; ++edit)
            {
                string immediate = null;
                string pending = null;
                if (edit != 0) text = "999-" + text;
                await AfterPaintAsync(native, () =>
                {
                    if (edit == 0) native.SetText(text);
                    else native.InsertText(0, "999-");
                    // Query directly after the edit, before any paint/yield or
                    // lexer reset. TSF may already have styled this line while
                    // processing the edit; both notification orders are valid.
                    immediate = FirstLine();

                    // Separately exercise explicitly pending tokenization.
                    native.SetLexerLanguage(profile.Lexer, profile.Keywords, profile.PropertyNames, profile.PropertyValues);
                    pending = FirstLine();
                });

                var y = native.PointYFromPosition(0);
                long end = 0;
                while (end < native.Length && native.PointYFromPosition(end) == y)
                    end = native.PositionAfter(end);
                var bytes = Encoding.UTF8.GetBytes(text);
                var rendered = Encoding.UTF8.GetString(bytes, 0, (int)end);
                Equal(rendered, immediate,
                    "synchronous lexer-enabled line query agrees with subsequent painting after load/edit");
                Equal(rendered, pending,
                    "synchronous line query with pending tokenization agrees with subsequent painting");
                Check(native.GetStyleAt(0) != native.GetStyleAt(3), "syntax fixture contains different token styles");
            }

            string FirstLine()
            {
                var range = At(provider, 0);
                range.ExpandToEnclosingUnit(TextUnit.Line);
                return range.GetText(-1);
            }
        }
        finally
        {
            // The surrounding navigation fixtures use the plain-text profile;
            // direct lexer configuration does not change their managed state.
            native.SetLexerLanguage(string.Empty, [], [], []);
        }
    }

    private static ITextRangeProvider At(ITextProvider provider, int characters, int length = 0)
    {
        var range = provider.DocumentRange;
        range.MoveEndpointByRange(TextPatternRangeEndpoint.End, range, TextPatternRangeEndpoint.Start);
        if (characters != 0)
            Check(range.Move(TextUnit.Character, characters) == characters, "fixture character offset");
        if (length != 0)
            Check(range.MoveEndpointByUnit(TextPatternRangeEndpoint.End, TextUnit.Character, length) == length, "fixture character length");
        return range;
    }

    private static int CharacterCount(string text)
    {
        var count = 0;
        foreach (var _ in text.EnumerateRunes()) ++count;
        return count;
    }

    private static void Equal(string expected, string actual, string message) => Check(expected == actual,
        message + $"; expected [{expected.Replace("\r", "\\r", StringComparison.Ordinal)}], actual [{actual.Replace("\r", "\\r", StringComparison.Ordinal)}]");

    private static void Check(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }
}
