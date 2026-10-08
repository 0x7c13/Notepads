// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using Notepads.Features.Documents.FileTypes;
using Notepads.Features.Documents.Text;
using Notepads.Features.Preferences;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Presentation.Theming;
using Windows.UI.Core;
using WinUIEditor;

namespace Notepads.Presentation.Controls.TextEditor;

// Indentation follows the file, then the language's convention, then the Tab
// setting. Rules read lexer styles on the current line, so brackets and colons
// in comments or strings never trigger them.
public sealed partial class TextEditorCore
{
    // The native syntax long-line limit (NativeSyntax.h).
    private const int StyleCatchUpBytes = 64 * 1024;
    private const int PythonReferenceLines = 1000;
    private DocumentLanguage _language = DocumentLanguages.PlainText; // set in SetSyntaxLanguage
    private int _detectedIndentation; // 0 = unknown
    private long _indentationCheckedVersion = -1;
    private bool _typedIndentationQueued;
    private long _typedIndentationVersion;
    private long _typedIndentationPos;

    private bool HasBracketRules => _language.Id is not ("plaintext" or "markdown" or "html" or "xml" or "yaml");
    private bool HasColonRules => _language.Id is "python" or "yaml";

    // Common conventions where a language has one; 0 defers to the Tab setting.
    private static int GetConventionalIndentation(string languageId) => languageId switch
    {
        "csharp" or "java" or "powershell" or "python" or "rust" => 4,
        "css" or "javascript" or "json" or "typescript" or "yaml" => 2,
        _ => 0
    };

    private int GetEffectiveIndentation()
    {
        // A detected result stays until the next load; an unknown one is retried after edits.
        if (_detectedIndentation == 0 && _indentationCheckedVersion != ContentVersion)
        {
            _detectedIndentation = IndentationDetector.Detect(ReadIndentationSample());
            _indentationCheckedVersion = ContentVersion;
        }
        var indentation = _detectedIndentation != 0 ? _detectedIndentation : GetConventionalIndentation(_language.Id);
        if (indentation == 0) indentation = ApplicationPreferences.EditorDefaultTabIndents;
        // YAML forbids tabs for indentation.
        return _language.Id == "yaml" && indentation < 0 ? 2 : indentation;
    }

    private string ReadIndentationSample()
    {
        var end = Math.Min(Native.LineCount > IndentationDetector.SampleLines
            ? Native.PositionFromLine(IndentationDetector.SampleLines) : Native.Length, IndentationDetector.SampleBytes);
        while (end > 0 && end < Native.Length && (Native.GetCharAt(end) & 0xc0) == 0x80) end--;
        return ReadRange(0, end);
    }

    private void ResetDetectedIndentation()
    {
        _detectedIndentation = 0;
        _indentationCheckedVersion = -1;
    }

    private int GetUnitColumns(int indentation) => indentation <= 0 ? Native.TabWidth : indentation;

    private string CreateIndentation(int columns, int indentation) => indentation < 0
        ? new string('\t', columns / Native.TabWidth) + new string(' ', columns % Native.TabWidth)
        : new string(' ', columns);

    private void ChangeIndentation(bool remove)
    {
        if (!CanEdit) return;
        var startLine = Native.LineFromPosition(Native.SelectionStart);
        var endLine = Native.LineFromPosition(Native.SelectionEnd);
        var indentation = GetEffectiveIndentation();
        Native.UseTabs = indentation < 0;
        Native.Indent = GetUnitColumns(indentation);
        var singleRangeOnOneLine = startLine == endLine && Native.Selections == 1 && !Native.SelectionIsRectangle;
        if (!remove && singleRangeOnOneLine)
        {
            // Insert one indentation unit rather than moving to the next
            // visual tab stop.
            TypeText(CreateIndentation(GetUnitColumns(indentation), indentation));
            return;
        }
        if (remove && singleRangeOnOneLine)
        {
            // BackTab outside the leading whitespace moves the caret.
            // Notepads' Shift+Tab command always dedents the current line.
            DedentLine(startLine, Math.Max(0, Native.GetLineIndentation(startLine) - Native.Indent));
            return;
        }
        // Native commands preserve selection direction, handle rectangular
        // ranges, exclude an unselected final line, and group undo.
        if (remove) Native.BackTab();
        else Native.Tab();
    }

    // Backspace at the end of a line's indentation removes back to the previous
    // indentation unit stop; anywhere else it deletes as usual.
    private void DeleteBackWithUnindent()
    {
        if (!CanEdit) return;
        var caret = Native.CurrentPos;
        var line = Native.LineFromPosition(caret);
        var column = Native.GetLineIndentation(line);
        if (column > 0 && Native.Selections == 1 && !Native.SelectionIsRectangle
            && Native.SelectionEmpty && Native.GetLineIndentPosition(line) == caret)
        {
            var unit = GetUnitColumns(GetEffectiveIndentation());
            DedentLine(line, (column - 1) / unit * unit);
            return;
        }
        Native.DeleteBack();
    }

    private void EnterWithAutoIndentation()
    {
        var start = Native.SelectionStart;
        var line = Native.LineFromPosition(start);
        var lineStart = Native.PositionFromLine(line);
        // Read only the indentation prefix, even for an exceptionally long line.
        var codeStart = lineStart;
        while (codeStart < start && IsSpaceOrTab(Native.GetCharAt(codeStart))) codeStart++;
        var indentation = ReadRange(lineStart, codeStart);
        if (CanUseIndentationRules())
        {
            var end = Native.SelectionEnd;
            var lineEnd = Native.GetLineEndPosition(line);
            // The last code character before the caret, after a trailing comment.
            var prev = start - 1;
            while (prev >= codeStart && (IsSpaceOrTab(Native.GetCharAt(prev)) || GetRole(prev) == SyntaxColorRole.Comment)) prev--;
            var ch = prev >= codeStart ? Native.GetCharAt(prev) : 0;
            if (ch is '{' or '[' or '(' && HasBracketRules && GetRole(prev) == SyntaxColorRole.Operator)
            {
                var indent = GetEffectiveIndentation();
                var unit = CreateIndentation(GetUnitColumns(indent), indent);
                var closer = end;
                while (closer < lineEnd && IsSpaceOrTab(Native.GetCharAt(closer))) closer++;
                if (closer < lineEnd && Native.GetCharAt(closer) == (ch == '{' ? '}' : ch == '[' ? ']' : ')')
                    && GetRole(closer) == SyntaxColorRole.Operator)
                {
                    // Split the pair and leave the caret on the indented middle line.
                    Native.SetSel(start, closer);
                    TypeText("\r" + indentation + unit + "\r" + indentation);
                    Native.SetEmptySelection(start + 1 + indentation.Length + unit.Length);
                    // The paste recorded the closing line's column; Up/Down must
                    // use the middle line's instead.
                    Native.ChooseCaretX();
                    return;
                }
                indentation += unit;
            }
            else if (ch == ':' && HasColonRules && GetRole(prev) == SyntaxColorRole.Operator
                || _language.Id == "yaml" && IsYamlBlockScalar(prev, codeStart))
            {
                var indent = GetEffectiveIndentation();
                if (_language.Id == "yaml")
                {
                    // A sequence item's "- " counts as indentation; YAML indents with spaces.
                    var item = Native.GetCharAt(codeStart) == '-' && Native.GetCharAt(codeStart + 1) == ' ' ? codeStart + 2 : codeStart;
                    indentation = new string(' ', (int)Native.GetColumn(item) + GetUnitColumns(indent));
                }
                else indentation += CreateIndentation(GetUnitColumns(indent), indent);
            }
            else if (_language.Id == "python" && prev >= codeStart && IsPythonBlockExit(codeStart, prev, end, lineEnd))
            {
                var indent = GetEffectiveIndentation();
                var dedented = CreateIndentation(Math.Max(0, Native.GetLineIndentation(line) - GetUnitColumns(indent)), indent);
                if (dedented != indentation)
                {
                    // A statement continuing an open multiline string keeps the
                    // block. Only the lexer knows that, and it needs the break to
                    // style: insert it for real, then read the style it was given.
                    Native.BeginUndoAction();
                    try
                    {
                        TypeText("\r");
                        // A failed catch-up keeps the statement's indentation:
                        // the block's own indentation is the safe choice.
                        var styled = TryEnsureStyled(start + 1);
                        var next = styled && GetRole(start) != SyntaxColorRole.String ? dedented : indentation;
                        if (next.Length > 0) TypeText(next);
                    }
                    finally { Native.EndUndoAction(); }
                    return;
                }
            }
        }
        TypeText("\r" + indentation);
    }

    // Rules need current lexer styles and one stream selection within a line.
    private bool CanUseIndentationRules()
    {
        if (!CanEdit || _view.IsComposing || _view.HasTextStoreLock
            || !HasBracketRules && !HasColonRules || _syntaxProfile.Lexer.Length == 0
            || SyntaxPauseReason != EditorSyntaxPauseReason.None || Native.Selections != 1 || Native.SelectionIsRectangle)
        {
            return false;
        }
        var line = Native.LineFromPosition(Native.SelectionStart);
        // Include the line's EOL, whose style shows an open multiline string.
        return Native.LineFromPosition(Native.SelectionEnd) == line && TryEnsureStyled(Native.PositionFromLine(line + 1));
    }

    // Typing moves EndStyled back and paint styles later. Catch up from the
    // start of EndStyled's line, as Document::EnsureStyledTo does.
    private bool TryEnsureStyled(long end)
    {
        var styled = Native.EndStyled;
        if (styled >= end) return true;
        var from = Native.PositionFromLine(Native.LineFromPosition(styled));
        if (end - from > StyleCatchUpBytes) return false;
        Native.Colourise(from, end);
        return true;
    }

    private SyntaxColorRole GetRole(long position) => _syntaxProfile.Tokens[Native.GetStyleIndexAt(position)];

    private static bool IsSpaceOrTab(int ch) => ch is ' ' or '\t';

    private bool IsBlankOrComment(long start, long end)
    {
        for (var position = start; position < end; position++)
            if (!IsSpaceOrTab(Native.GetCharAt(position)) && GetRole(position) != SyntaxColorRole.Comment) return false;
        return true;
    }

    // `key: |` or `key: >`, with an optional + or - chomping indicator.
    private bool IsYamlBlockScalar(long position, long codeStart)
    {
        if (Native.GetCharAt(position) is '+' or '-') position--;
        if (position < codeStart || Native.GetCharAt(position) is not ('|' or '>')) return false;
        do position--; while (position >= codeStart && IsSpaceOrTab(Native.GetCharAt(position)));
        return position >= codeStart && Native.GetCharAt(position) == ':' && GetRole(position) == SyntaxColorRole.Operator;
    }

    // `return x` and similar statements end the block, unless the statement
    // continues on the next line. Whether an open multiline string continues it
    // is decided from the lexer after the break exists, in the caller.
    private bool IsPythonBlockExit(long codeStart, long prev, long end, long lineEnd)
    {
        var wordEnd = Native.WordEndPosition(codeStart, true);
        if (wordEnd - codeStart > 8 || GetRole(codeStart) != SyntaxColorRole.Keyword
            || ReadRange(codeStart, wordEnd) is not ("pass" or "break" or "continue" or "return" or "raise")
            || Native.GetCharAt(prev) == '\\' || !IsBlankOrComment(end, lineEnd))
        {
            return false;
        }
        var depth = 0;
        for (var position = wordEnd; position <= prev; position++)
        {
            var ch = Native.GetCharAt(position);
            if (ch is not ('(' or '[' or '{' or ')' or ']' or '}') || GetRole(position) != SyntaxColorRole.Operator) continue;
            depth = ch is '(' or '[' or '{' ? depth + 1 : Math.Max(0, depth - 1);
        }
        return depth == 0;
    }

    private bool IsTypedIndentationTrigger(int ch) => ch is '}' or ']' or ')' ? HasBracketRules : ch == ':' && _language.Id == "python";

    // CharAdded runs while TSF still holds its lock, so edit after the update.
    private void OnNativeCharAdded(Editor sender, CharAddedEventArgs args)
    {
        // A TSF update can insert several characters; Ch is only the first.
        var p = Native.CurrentPos - 1;
        var ch = Native.GetCharAt(p);
        // A closer only moves when it starts its line.
        if (!CanEdit || _view.IsComposing || !IsTypedIndentationTrigger(ch)
            || ch != ':' && Native.GetLineIndentPosition(Native.LineFromPosition(p)) != p)
        {
            return;
        }
        _typedIndentationVersion = ContentVersion;
        _typedIndentationPos = Native.CurrentPos;
        QueueTypedIndentation();
    }

    private async void QueueTypedIndentation()
    {
        if (_typedIndentationQueued) return;
        _typedIndentationQueued = true;
        try
        {
            await Dispatcher.RunAsync(CoreDispatcherPriority.Normal, () =>
            {
                _typedIndentationQueued = false;
                ApplyTypedIndentation(_typedIndentationVersion, _typedIndentationPos);
            });
        }
        catch (Exception exception)
        {
            _typedIndentationQueued = false;
            if (!_disposed) LoggingService.LogException(exception);
        }
    }

    // Dedents a typed closer to its opener's line, or a Python else/elif/except/
    // finally to its block. The adjustment is its own undo step.
    private void ApplyTypedIndentation(long version, long pos)
    {
        if (_disposed || _settingText || ContentVersion != version || Native.CurrentPos != pos
            || !Native.SelectionEmpty || !CanUseIndentationRules())
        {
            return;
        }
        var p = pos - 1;
        var ch = Native.GetCharAt(p);
        if (!IsTypedIndentationTrigger(ch) || GetRole(p) != SyntaxColorRole.Operator) return;
        var line = Native.LineFromPosition(p);
        int target;
        if (ch == ':')
        {
            var codeStart = Native.GetLineIndentPosition(line);
            if (!IsPythonDedentKeyword(codeStart, pos, Native.GetLineEndPosition(line))) return;
            target = FindPythonDedentTarget(line, ReadRange(codeStart, Native.WordEndPosition(codeStart, true)));
            if (target < 0) return;
        }
        else
        {
            var match = Native.BraceMatch(p, 0);
            if (match < 0) return;
            target = Native.GetLineIndentation(Native.LineFromPosition(match));
        }
        // Only ever dedent, never moving text right against what was typed.
        if (target >= Native.GetLineIndentation(line)) return;
        DedentLine(line, target);
    }

    // Deleting the end of the indentation keeps carets in place relative to
    // the text, and the deletion tells TSF where the caret is. A tab across
    // the target column is replaced by the spaces before it.
    private void DedentLine(long line, int column)
    {
        var start = Native.FindColumn(line, column);
        var end = Native.GetLineIndentPosition(line);
        if (start >= end) return;
        var spaces = column - (int)Native.GetColumn(start);
        Native.BeginUndoAction();
        try
        {
            // Unlike SCI_INSERTTEXT, replacing an empty target keeps the selection.
            if (spaces > 0) ReplaceRange(start, start, new string(' ', spaces));
            Native.DeleteRange(start + spaces, end - start);
        }
        finally { Native.EndUndoAction(); }
        // As native Backspace and BackTab do: Up/Down keep the new column.
        Native.ChooseCaretX();
        Native.ScrollCaret();
    }

    // The line is `else:` or `finally:`, or starts with `elif` or `except`, and ends at this colon.
    private bool IsPythonDedentKeyword(long codeStart, long pos, long lineEnd)
    {
        var wordEnd = Native.WordEndPosition(codeStart, true);
        if (wordEnd - codeStart > 7 || GetRole(codeStart) != SyntaxColorRole.Keyword) return false;
        var word = ReadRange(codeStart, wordEnd);
        return (word is "else" or "finally" ? wordEnd == pos - 1 : word is "elif" or "except" && Native.GetCharAt(wordEnd) is ' ' or ':')
            && IsBlankOrComment(pos, lineEnd);
    }

    // Walk enclosing suites rather than subtracting a unit from the last
    // statement: it may be nested several levels deeper, or an inline suite.
    // A statement at a smaller indent closes any earlier suites at its level.
    private int FindPythonDedentTarget(long line, string keyword)
    {
        var currentIndent = Native.GetLineIndentation(line);
        var enclosingIndent = int.MaxValue;
        var limit = Math.Max(0, line - PythonReferenceLines);
        for (var reference = line - 1; reference >= limit; reference--)
        {
            var start = Native.GetLineIndentPosition(reference);
            if (start == Native.GetLineEndPosition(reference) || GetRole(start) == SyntaxColorRole.Comment) continue;
            // A continued header can end with a closing bracket on its own
            // line. Its opener's line carries the block keyword and indent.
            if (Native.GetCharAt(start) is ')' or ']' or '}' && GetRole(start) == SyntaxColorRole.Operator)
            {
                var match = Native.BraceMatch(start, 0);
                if (match < 0) return -1;
                reference = Native.LineFromPosition(match);
                if (reference < limit) return -1;
                start = Native.GetLineIndentPosition(reference);
            }
            var indent = Native.GetLineIndentation(reference);
            if (indent > enclosingIndent) continue;
            // A line whose text sits inside a string already open above is
            // content, not a statement: step over it without moving the
            // boundary. A line that opens a string is a statement like any
            // other and must still close the suites at its indentation.
            if (GetRole(start) == SyntaxColorRole.String && reference > 0
                && GetRole(Native.GetLineEndPosition(reference - 1)) == SyntaxColorRole.String)
            {
                continue;
            }
            var end = Native.WordEndPosition(start, true);
            var word = end - start <= 8 && GetRole(start) == SyntaxColorRole.Keyword ? ReadRange(start, end) : string.Empty;
            if (word == "async")
            {
                start = end;
                while (IsSpaceOrTab(Native.GetCharAt(start))) start++;
                end = Native.WordEndPosition(start, true);
                word = end - start <= 4 ? ReadRange(start, end) : string.Empty;
            }
            // A definition fences the caret in only while the caret sits inside
            // its body; at the definition's own indentation the caret has left.
            if (word is "def" or "class" && indent < currentIndent) return -1;
            var matches = keyword switch
            {
                "elif" => word is "if" or "elif",
                "else" => word is "if" or "elif" or "for" or "while" or "except",
                "except" or "finally" => word is "try" or "except",
                _ => false
            };
            if (matches && indent <= currentIndent) return indent;
            // A try/except's else can precede finally; verify its earlier
            // try/except at this same level before changing the indentation.
            enclosingIndent = keyword == "finally" && word == "else" ? indent : indent - 1;
        }
        return -1;
    }
}
