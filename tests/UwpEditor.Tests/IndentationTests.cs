// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Text;
using System.Threading.Tasks;
using Notepads.Features.Documents.FileTypes;
using Notepads.Features.Preferences;
using Notepads.Presentation.Controls.TextEditor;
using Windows.UI.Core;
using Windows.UI.Xaml.Controls;

namespace NotepadsEditorTests;

// '^' marks the caret in fixtures; a second '^' makes the first one the anchor.
internal static class IndentationTests
{
    public static async Task RunAsync(StringBuilder log, TextEditorCore core)
    {
        var native = GetNative(core);
        var previousIndent = ApplicationPreferences.EditorDefaultTabIndents;
        try
        {
            // Refresh the cached syntax status left by earlier tests.
            await core.LoadTextAsync(string.Empty);
            CheckPrecedence(core, native);
            CheckEnterRules(core, native);
            await CheckEnterWithoutRulesAsync(core, native);
            await CheckTypedClosersAsync(core, native);
            await CheckPythonColonsAsync(core, native);
            CheckReadOnly(core, native);
            CheckSingleLineDedentSelection(core, native);
        }
        finally
        {
            ApplicationPreferences.EditorDefaultTabIndents = previousIndent;
            core.SetSyntaxLanguage(DocumentLanguages.PlainText, "x.txt");
        }
        log.AppendLine("PASS: indentation precedence (file, language, Tab setting), Enter rules for brackets, colons, YAML and Python block exits, " +
            "typed closer and Python keyword dedents, stale and disposed callbacks, read-only Tab, and selection-keeping Shift+Tab.");
    }

    private static void CheckPrecedence(TextEditorCore core, WinUIEditor.Editor native)
    {
        ApplicationPreferences.EditorDefaultTabIndents = -1;
        Enter(core, native, "csharp", "class A {\r  int x;\r  void F() {^\r    y();\r  }\r}",
            "class A {\r  int x;\r  void F() {\r    ^\r    y();\r  }\r}", "Enter in a detected 2-space file with the tab setting");
        Load(core, "csharp", "class A {\r  int x;\r^\r}");
        core.TestIndentation(false);
        Expect(core, native, "class A {\r  int x;\r  ^\r}", "Tab in a detected 2-space file with the tab setting");
        Enter(core, native, "csharp", "void F() {^", "void F() {\r    ^", "Enter in a new C# file with the tab setting");
        core.Undo();
        Check(core.GetText() == "void F() {" && !core.CanUndo, "an indenting Enter is one undo step");
        Enter(core, native, "json", "{^", "{\r  ^", "Enter in a new JSON file");
        Enter(core, native, "yaml", "key:^", "key:\r  ^", "Enter in a new YAML file with the tab setting");
        Enter(core, native, "yaml", "a:\r\tb: 1\r\tc: 2\rd:^", "a:\r\tb: 1\r\tc: 2\rd:\r  ^", "Enter in a tab-indented YAML file");
        core.TestIndentation(false);
        Expect(core, native, "a:\r\tb: 1\r\tc: 2\rd:\r    ^", "Tab in a tab-indented YAML file inserts spaces");

        ApplicationPreferences.EditorDefaultTabIndents = 4;
        Enter(core, native, "csharp", "class A\r{\r\tvoid F()\r\t{^\r\t}\r}", "class A\r{\r\tvoid F()\r\t{\r\t\t^\r\t}\r}",
            "Enter in a detected tab file with the 4-space setting");
        core.TestIndentation(false);
        Expect(core, native, "class A\r{\r\tvoid F()\r\t{\r\t\t\t^\r\t}\r}", "Tab in a detected tab file with the 4-space setting");

        ApplicationPreferences.EditorDefaultTabIndents = 8;
        Load(core, "plaintext", "x^");
        core.TestIndentation(false);
        Expect(core, native, "x        ^", "Tab in plain text uses the Tab setting");
        core.SelectAll();
        core.TypeText("a\r  b\r    c\r");
        core.TestIndentation(false);
        Expect(core, native, "a\r  b\r    c\r  ^", "pasted indentation is detected on the next Tab");

        foreach (var (language, text) in new[] { ("plaintext", "  {^"), ("markdown", "  {^"), ("csharp", "  x;^") })
        {
            Enter(core, native, language, text, text.Replace("^", "\r  ^", StringComparison.Ordinal), language + " continuing Enter");
            Check(core.TestIndentationCheckedVersion == -1, language + " continuing Enter does not read the indentation sample");
        }
    }

    private static void CheckEnterRules(TextEditorCore core, WinUIEditor.Editor native)
    {
        ApplicationPreferences.EditorDefaultTabIndents = 4;
        Enter(core, native, "json", "{\"a\": [^", "{\"a\": [\r  ^", "JSON [");
        Enter(core, native, "python", "if x:  # c^", "if x:  # c\r    ^", "Python colon before a comment");
        Enter(core, native, "yaml", "  - name:^", "  - name:\r      ^", "YAML sequence item key");
        Enter(core, native, "yaml", "run: |^", "run: |\r  ^", "YAML block scalar");
        Enter(core, native, "yaml", "  - run: |^", "  - run: |\r      ^", "YAML sequence item block scalar");

        Load(core, "csharp", "void F() ^");
        native.Colourise(0, -1);
        native.AddText(1, "{");
        Check(native.EndStyled < native.Length, "typing leaves the brace unstyled");
        core.TestEnter();
        Expect(core, native, "void F() {\r    ^", "Enter right after typing styles the line first");

        Enter(core, native, "csharp", "// {^", "// {\r^", "C# brace in a comment");
        Enter(core, native, "csharp", "s = \"{^\";", "s = \"{\r^\";", "C# brace in a string");
        Enter(core, native, "python", "x = \"a:^\"", "x = \"a:\r^\"", "Python colon in a string");
        Enter(core, native, "python", "x = 1  # :^", "x = 1  # :\r^", "Python colon in a comment");
        Enter(core, native, "json", "{\"a{^\": 1}", "{\"a{\r^\": 1}", "JSON brace in a property name");
        Enter(core, native, "yaml", "items: [^", "items: [\r^", "YAML flow sequence");

        Enter(core, native, "csharp", "void F() {^}", "void F() {\r    ^\r}", "Enter between braces");
        core.Undo();
        Check(core.GetText() == "void F() {}" && !core.CanUndo, "Enter between braces is one undo step");
        Enter(core, native, "csharp", "s = \"é\"; F() {^}", "s = \"é\"; F() {\r    ^\r}", "Enter between braces after non-ASCII text");
        Enter(core, native, "json", "[ ^ ]", "[ \r  ^\r]", "Enter between spaced brackets");
        core.Undo();
        Check(core.GetText() == "[  ]" && !core.CanUndo, "Enter between brackets is one undo step");

        Enter(core, native, "python", "def f():\r    return x^", "def f():\r    return x\r^", "Python return");
        Enter(core, native, "python", "if x:\r    pass^", "if x:\r    pass\r^", "Python pass");
        Enter(core, native, "python", "if x:\r  \t    return x^", "if x:\r  \t    return x\r\t^", "Python return after mixed indentation");
        foreach (var text in new[] {
            "if x:\r    returned = 1^", "def f():\r    return call(arg,^", "def f():\r    \"\"\"\r    pass^\r    \"\"\"",
            "def f():\r    return a + \\^", "def f():\r    return \"\"\"text^\r" })
        {
            Enter(core, native, "python", text, text.Replace("^", "\r    ^", StringComparison.Ordinal), "Python non-exit " + Escape(text));
        }
        Enter(core, native, "python", "def f():\r    return^ x", "def f():\r    return\r    ^ x", "Python return before code");
    }

    private static async Task CheckEnterWithoutRulesAsync(TextEditorCore core, WinUIEditor.Editor native)
    {
        var multipleSelection = native.MultipleSelection;
        native.MultipleSelection = true;
        try
        {
            Load(core, "csharp", "  a {^\r  b {");
            native.AddSelection(native.Length, native.Length);
            core.TestEnter();
            Equal("  a {\r  \r  b {\r  ", core.GetText(), "Enter with multiple selections continues indentation");
        }
        finally { native.MultipleSelection = multipleSelection; }
        Enter(core, native, "csharp", "  a {^\r  b^ {", "  a {\r  ^ {", "Enter over a selection that spans lines");

        core.SetSyntaxLanguage(DocumentLanguages.Find("csharp"), "x.cs");
        await core.LoadTextAsync(new string('x', 65537) + "\r  void F() {");
        try
        {
            Check(core.SyntaxPauseReason == WinUIEditor.EditorSyntaxPauseReason.LongLine, "a long line pauses syntax");
            native.SetEmptySelection(native.Length);
            core.TestEnter();
            Check(core.GetText().EndsWith("  void F() {\r  ", StringComparison.Ordinal), "Enter with paused syntax continues indentation");
        }
        finally { await core.LoadTextAsync(string.Empty); }
    }

    private static async Task CheckTypedClosersAsync(TextEditorCore core, WinUIEditor.Editor native)
    {
        await Type(core, native, "csharp", "void F()\r{\r    a();\r    ^", "}", "void F()\r{\r    a();\r}^", "typed brace");
        core.Undo();
        Equal("void F()\r{\r    a();\r    }", core.GetText(), "the first undo restores the indentation");
        core.Undo();
        Check(core.GetText() == "void F()\r{\r    a();\r    " && !core.CanUndo, "the next undo removes the typing");
        await Type(core, native, "csharp", "F(\r    a,\r    ^", ")", "F(\r    a,\r)^", "typed parenthesis");
        await Type(core, native, "json", "[\r  1,\r  ^", "]", "[\r  1,\r]^", "typed bracket");

        await Type(core, native, "csharp", "{\r    /*\r    ^", "}", "{\r    /*\r    }^", "typed brace in a comment");
        await Type(core, native, "csharp", "{\r    s = @\"\r    ^", "}", "{\r    s = @\"\r    }^", "typed brace in a verbatim string");
        await Type(core, native, "csharp", "{\r    a^", "}", "{\r    a}^", "typed brace after code");
        await Type(core, native, "csharp", "    {\r  ^", "}", "    {\r  }^", "typed brace with a deeper opener");
        await Type(core, native, "csharp", "a\r    ^", "}", "a\r    }^", "typed brace without an opener");
        await Type(core, native, "csharp", "  {\r\t^", "}", "  {\r  }^", "typed brace across a tab stop");

        Load(core, "csharp", "{\r    ^");
        native.AddText(1, "}");
        core.TestCharAdded();
        native.AddText(1, ";");
        await RoundTripAsync(core);
        Expect(core, native, "{\r    };^", "typing again before the callback cancels the adjustment");

        Load(core, "csharp", "{\r    ^");
        native.AddText(1, "}");
        var version = core.ContentVersion;
        var pos = native.CurrentPos;
        native.SetEmptySelection(0);
        core.TestApplyTypedIndentation(version, pos);
        Equal("{\r    }", core.GetText(), "a moved caret cancels the adjustment");
        native.SetEmptySelection(pos);
        core.TestApplyTypedIndentation(version, pos);
        Expect(core, native, "{\r}^", "an unchanged snapshot applies the adjustment");

        var closed = new TextEditorCore();
        var closedNative = GetNative(closed);
        closed.SetSyntaxLanguage(DocumentLanguages.Find("csharp"), "x.cs");
        closed.SetText("{\r    ");
        closedNative.SetEmptySelection(closedNative.Length);
        closedNative.AddText(1, "}");
        closed.TestCharAdded();
        var closedVersion = closed.ContentVersion;
        var closedPos = closedNative.CurrentPos;
        closed.Dispose();
        // Neither the queued callback nor a late direct call may throw after disposal.
        await RoundTripAsync(core);
        closed.TestApplyTypedIndentation(closedVersion, closedPos);
    }

    private static async Task CheckPythonColonsAsync(TextEditorCore core, WinUIEditor.Editor native)
    {
        ApplicationPreferences.EditorDefaultTabIndents = 4;
        foreach (var (block, keyword) in new[] { ("if x:", "else"), ("if x:", "elif y"), ("try:", "except E"), ("try:", "finally") })
        {
            await Type(core, native, "python", block + "\r    a = 1\r    " + keyword + "^", ":",
                block + "\r    a = 1\r" + keyword + ":^", "Python " + keyword + ":");
        }
        await Type(core, native, "python", "if x:\r    if y:\r        a = 1\r# note\r\r        else^", ":",
            "if x:\r    if y:\r        a = 1\r# note\r\r    else:^", "Python else: after a comment line");
        await Type(core, native, "python", "if x:\r    a = 1\relse^", ":", "if x:\r    a = 1\relse:^", "Python else: already in place");
        await Type(core, native, "python", "if x:\r    a = 1\r    elsewhere^", ":", "if x:\r    a = 1\r    elsewhere:^", "Python elsewhere:");
    }

    private static void CheckReadOnly(TextEditorCore core, WinUIEditor.Editor native)
    {
        foreach (var disable in new[] { false, true })
        {
            foreach (var text in new[] { "^  a\r  b^", "  a^" })
            {
                Load(core, "csharp", text);
                var before = core.GetText();
                var anchor = native.Anchor;
                var caret = native.CurrentPos;
                if (disable) core.IsEnabled = false;
                else native.ReadOnly = true;
                try
                {
                    core.TestIndentation(false);
                    core.TestIndentation(true);
                }
                finally
                {
                    core.IsEnabled = true;
                    native.ReadOnly = false;
                }
                Check(core.GetText() == before && native.Anchor == anchor && native.CurrentPos == caret && !core.CanUndo,
                    "Tab and Shift+Tab leave a read-only editor unchanged: " + Escape(text) + (disable ? " (disabled)" : " (read-only)"));
            }
        }
    }

    private static void CheckSingleLineDedentSelection(TextEditorCore core, WinUIEditor.Editor native)
    {
        ApplicationPreferences.EditorDefaultTabIndents = 4;
        foreach (var (text, expected, start, shift) in new[] { ("        abc^", "    abc", 8, 4), ("\t  abc^", "  abc", 3, 1) })
            foreach (var (anchor, caret) in new[] { (start, start + 3), (start + 3, start) })
            {
                Load(core, "plaintext", text);
                core.SetTextSelectionPosition(anchor, caret);
                core.TestIndentation(true);
                Equal(expected, core.GetText(), "single-line Shift+Tab with a selection: " + Escape(text));
                Check(native.Anchor == anchor - shift && native.CurrentPos == caret - shift,
                    $"single-line Shift+Tab keeps the selection and its direction: {Escape(text)} anchor={native.Anchor} caret={native.CurrentPos}");
            }
        foreach (var (text, expected) in new[] { ("        ^abc", "    ^abc"), ("        ^", "    ^"), ("\t\t^abc", "\t^abc"), ("\t  ^abc", "  ^abc") })
        {
            Load(core, "plaintext", text);
            core.TestIndentation(true);
            Expect(core, native, expected, "single-line Shift+Tab keeps the caret after the indentation: " + Escape(text));
        }
        Load(core, "plaintext", "\t\t^abc");
        core.TestIndentation(true);
        core.TestIndentation(true);
        core.Undo();
        Equal("\tabc", core.GetText(), "each single-line Shift+Tab is its own undo step");
    }

    private static void Enter(TextEditorCore core, WinUIEditor.Editor native, string language, string text, string expected, string name)
    {
        Load(core, language, text);
        core.TestEnter();
        Expect(core, native, expected, name);
    }

    // Inserts text at the caret as a TSF update would, then lets the queued adjustment run.
    private static async Task Type(TextEditorCore core, WinUIEditor.Editor native, string language, string text, string typed, string expected, string name)
    {
        Load(core, language, text);
        native.AddText(typed.Length, typed);
        core.TestCharAdded();
        await RoundTripAsync(core);
        Expect(core, native, expected, name);
    }

    private static async Task RoundTripAsync(TextEditorCore core) =>
        await core.Dispatcher.RunAsync(CoreDispatcherPriority.Normal, () => { });

    private static void Load(TextEditorCore core, string language, string text)
    {
        core.SetSyntaxLanguage(DocumentLanguages.Find(language), "x." + language);
        var anchor = text.IndexOf('^');
        text = text.Remove(anchor, 1);
        var caret = text.IndexOf('^');
        if (caret < 0) caret = anchor;
        else text = text.Remove(caret, 1);
        core.SetText(text);
        core.SetTextSelectionPosition(anchor, caret);
    }

    private static void Expect(TextEditorCore core, WinUIEditor.Editor native, string expected, string name)
    {
        var caret = expected.IndexOf('^');
        Equal(expected.Remove(caret, 1), core.GetText(), name);
        // Native positions count UTF-8 bytes.
        Check(native.SelectionEmpty && native.CurrentPos == Encoding.UTF8.GetByteCount(expected.AsSpan(0, caret)),
            name + " caret; actual=" + native.CurrentPos);
    }

    private static WinUIEditor.Editor GetNative(TextEditorCore core) => ((WinUIEditor.EditorBaseControl)((Grid)core.Content).Children[0]).Editor;
    private static string Escape(string text) => text.Replace("\r", "\\r", StringComparison.Ordinal).Replace("\t", "\\t", StringComparison.Ordinal);

    private static void Equal(string expected, string actual, string name)
    {
        if (!string.Equals(expected, actual, StringComparison.Ordinal))
            throw new Exception($"{name}: expected \"{Escape(expected)}\", actual \"{Escape(actual)}\".");
    }

    private static void Check(bool condition, string name)
    {
        if (!condition) throw new Exception(name);
    }
}
