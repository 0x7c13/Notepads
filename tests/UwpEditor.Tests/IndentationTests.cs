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
            CheckZeroIndentationSetting(core, native);
            await CheckReviewRegressionsAsync(core, native);
            CheckEnterRules(core, native);
            await CheckEnterWithoutRulesAsync(core, native);
            await CheckTypedClosersAsync(core, native);
            await CheckPythonColonsAsync(core, native);
            CheckReadOnly(core, native);
            CheckSingleLineDedentSelection(core, native);
            CheckBackspace(core, native);
        }
        finally
        {
            ApplicationPreferences.EditorDefaultTabIndents = previousIndent;
            core.SetSyntaxLanguage(DocumentLanguages.PlainText, "x.txt");
        }
        log.AppendLine("PASS: indentation precedence (file, language, Tab setting), Enter rules for brackets, colons, YAML and Python block exits, " +
            "typed closer and Python keyword dedents, stale and disposed callbacks, read-only Tab, selection-keeping Shift+Tab, and Backspace unindent.");
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

    private static async Task CheckReviewRegressionsAsync(TextEditorCore core, WinUIEditor.Editor native)
    {
        ApplicationPreferences.EditorDefaultTabIndents = 4;

        // Whether an open multiline string continues the statement is read from
        // the lexer's style on the break just inserted, which no leading-text scan
        // can reproduce: prefixes, escapes and the other delimiter all defeat one.
        Enter(core, native, "python", "def f():\r    return \"\"\"text ^", "def f():\r    return \"\"\"text \r    ^",
            "Python return in an unfinished string at EOF");
        Enter(core, native, "python", "def f():\r    return '''text ^", "def f():\r    return '''text \r    ^",
            "Python return in an unfinished single-quote string at EOF");
        Enter(core, native, "python", "def f():\r    return f\"\"\"text ^", "def f():\r    return f\"\"\"text \r    ^",
            "Python return in an unfinished f-string at EOF");
        Enter(core, native, "python", "def f():\r    return \"\"\"^", "def f():\r    return \"\"\"\r    ^",
            "Python return on a bare opening triple quote stays open");
        // The prefix is part of the delimiter, not string content.
        Enter(core, native, "python", "def f():\r    return f\"\"\"^", "def f():\r    return f\"\"\"\r    ^",
            "Python return in an unfinished prefixed string at EOF");
        Enter(core, native, "python", "def f():\r    return f\"\"\"\"\"\"^", "def f():\r    return f\"\"\"\"\"\"\r^",
            "Python return in a closed prefixed empty string at EOF");
        // An escaped quote is content, so the string stays open.
        Enter(core, native, "python", "def f():\r    return \"\"\"abc\\\"\"\"^", "def f():\r    return \"\"\"abc\\\"\"\"\r    ^",
            "Python return past an escaped quote stays open");
        Enter(core, native, "python", "def f():\r    return \"\"\"abc\\\"\"\"\"^", "def f():\r    return \"\"\"abc\\\"\"\"\"\r^",
            "Python return after an escaped quote and a closing triple");
        // A non-quote or the other delimiter inside the string is content.
        Enter(core, native, "python", "def f():\r    return \"\"\"abc\"\"x^", "def f():\r    return \"\"\"abc\"\"x\r    ^",
            "Python return after two content quotes and a character stays open");
        Enter(core, native, "python", "def f():\r    return '''abc\"\"\"^", "def f():\r    return '''abc\"\"\"\r    ^",
            "Python return after the other delimiter inside a string stays open");
        // Closed strings end the statement and dedent.
        Enter(core, native, "python", "def f():\r    return \"\"\"\"\"\"^", "def f():\r    return \"\"\"\"\"\"\r^",
            "Python return on an empty closed string at EOF");
        Enter(core, native, "python", "def f():\r    return \"\"\"abc\"\"\"^", "def f():\r    return \"\"\"abc\"\"\"\r^",
            "Python return in a closed triple-quoted string at EOF");
        Enter(core, native, "python", "def f():\r    return '''abc'''^", "def f():\r    return '''abc'''\r^",
            "Python return in a closed single-quote triple string at EOF");
        Enter(core, native, "python", "def f():\r    return f\"\"\"abc\"\"\"^", "def f():\r    return f\"\"\"abc\"\"\"\r^",
            "Python return in a closed triple-quoted f-string at EOF");
        Enter(core, native, "python", "def f():\r    return \"abc\"^", "def f():\r    return \"abc\"\r^",
            "Python return in a closed single-quote string at EOF");
        Enter(core, native, "python", "def f():\r    return \"\"\"text ^\r", "def f():\r    return \"\"\"text \r    ^\r",
            "Python return in an unfinished string before the last line");
        // A definition provides the indentation the open string continues.
        Enter(core, native, "python", "def f():\r    x = 1\r    return \"\"\"text ^", "def f():\r    x = 1\r    return \"\"\"text \r    ^",
            "Python open string continues the enclosing block's indentation");
        // The break and the indentation are one undo step.
        Load(core, "python", "def f():\r    return \"\"\"text ^");
        core.TestEnter();
        core.Undo();
        Expect(core, native, "def f():\r    return \"\"\"text ^", "one undo removes the whole Python block exit");

        // Splitting a bracket pair must leave Up/Down on the middle line's column.
        Load(core, "csharp", "void F() {^}");
        core.TestEnter();
        Expect(core, native, "void F() {\r    ^\r}", "bracket split: Enter between braces");
        native.LineUp();
        Expect(core, native, "void^ F() {\r    \r}", "bracket split: Up keeps the middle line's column");
        native.LineDown();
        Expect(core, native, "void F() {\r    ^\r}", "bracket split: Down keeps the middle line's column");

        // else after an if that contains a definition the caret has left: at the
        // definition's own indentation the outer if is reachable again.
        await Type(core, native, "python", "if x:\r    def f():\r        g()\r    else^", ":",
            "if x:\r    def f():\r        g()\relse:^", "Python else at the def's indentation aligns to the outer if");
        await Type(core, native, "python", "if x:\r    class C:\r        g()\r    else^", ":",
            "if x:\r    class C:\r        g()\relse:^", "Python else at the class's indentation aligns to the outer if");
        await Type(core, native, "python", "if x:\r    def f():\r        g()\r    h()\r        else^", ":",
            "if x:\r    def f():\r        g()\r    h()\relse:^", "Python else after a closed nested def body");
        await Type(core, native, "python", "if x:\r    def f():\r        g()\r        else^", ":",
            "if x:\r    def f():\r        g()\r        else:^", "Python else still inside a nested def");
        await Type(core, native, "python", "def f():\r    if x:\r        g()\r        else^", ":",
            "def f():\r    if x:\r        g()\r    else:^", "Python else inside a def matches its if");
        // A closed multiline string is not a statement: the walk must step over it
        // and still reach the enclosing header above it, whatever its indentation.
        await Type(core, native, "python", "if ready:\r    text = \"\"\"hello\rworld\r\"\"\"\r    else^", ":",
            "if ready:\r    text = \"\"\"hello\rworld\r\"\"\"\relse:^",
            "Python else after a closed multiline string aligns to the outer if");
        await Type(core, native, "python", "try:\r    text = \"\"\"a\rb\r\"\"\"\r    finally^", ":",
            "try:\r    text = \"\"\"a\rb\r\"\"\"\rfinally:^",
            "Python finally after a closed multiline string aligns to the outer try");
        // A standalone string statement closes deeper suites like any statement:
        // the walk must use its line's indentation, not step past the bookkeeping.
        await Type(core, native, "python", "if outer:\r    if inner:\r        work()\r    \"\"\"done\"\"\"\r    else^", ":",
            "if outer:\r    if inner:\r        work()\r    \"\"\"done\"\"\"\relse:^",
            "Python else after a standalone single-line string aligns to the outer if");
        await Type(core, native, "python", "if outer:\r    if inner:\r        work()\r    \"\"\"done\rmore\"\"\"\r    else^", ":",
            "if outer:\r    if inner:\r        work()\r    \"\"\"done\rmore\"\"\"\relse:^",
            "Python else after a standalone multiline string aligns to the outer if");
        await Type(core, native, "python", "if a:\r    if b:\r        c()\r        \"\"\"doc\"\"\"\r        else^", ":",
            "if a:\r    if b:\r        c()\r        \"\"\"doc\"\"\"\r    else:^",
            "Python else after a deeper standalone string aligns to its own if");
        await Type(core, native, "python", "try:\r    work()\r    \"\"\"doc\"\"\"\r    finally^", ":",
            "try:\r    work()\r    \"\"\"doc\"\"\"\rfinally:^",
            "Python finally after a standalone string aligns to the outer try");
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

    private static void CheckZeroIndentationSetting(TextEditorCore core, WinUIEditor.Editor native)
    {
        ApplicationPreferences.EditorDefaultTabIndents = 0;
        Load(core, "plaintext", "        ^");
        core.TestDeleteBack();
        Expect(core, native, "    ^", "Backspace with a zero Tab setting uses the tab width");
        core.TestIndentation(true);
        Expect(core, native, "^", "Shift+Tab with a zero Tab setting uses the tab width");
        core.TestIndentation(false);
        Expect(core, native, "    ^", "Tab with a zero Tab setting inserts a full unit");
        ApplicationPreferences.EditorDefaultTabIndents = 4;
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
        await Type(core, native, "csharp", "{\r    ^", "};", "{\r};^", "closer followed by a semicolon in one TSF update");
        await Type(core, native, "csharp", "{\r    ^", "}; // 你好", "{\r}; // 你好^", "batched closer with a UTF-8 suffix");
        await Type(core, native, "csharp", "你好\r{\r    ^", "};", "你好\r{\r};^", "batched closer after a UTF-8 prefix");
        await Type(core, native, "csharp", "F(\r    ^", ");", "F(\r);^", "batched parenthesis");
        await Type(core, native, "json", "[\r    ^", "],", "[\r],^", "batched bracket");
        await Type(core, native, "csharp", "{\r    }^", ";", "{\r    };^", "typing after an existing closer does not dedent it");
        await Type(core, native, "csharp", "{\r    /*\r    ^", "};", "{\r    /*\r    };^", "batched closer in a comment");
        await Type(core, native, "csharp", "{\r    s = @\"\r    ^", "};", "{\r    s = @\"\r    };^", "batched closer in a string");

        Load(core, "csharp", "{\r    ^");
        foreach (var ch in "}; ")
        {
            native.AddText(1, ch.ToString());
            core.TestCharAdded();
        }
        await RoundTripAsync(core);
        Expect(core, native, "{\r}; ^", "adjacent typing before the callback preserves the closer adjustment");
        core.Undo();
        Equal("{\r    }; ", core.GetText(), "one undo restores indentation without removing the burst");

        Load(core, "csharp", "{\r    ^");
        native.AddText(1, "}");
        core.TestCharAdded();
        core.TestFlushTypedIndentation(); // PreviewKeyDown runs before TSF observes the next key.
        Expect(core, native, "{\r}^", "pending closer is applied before the next key");
        native.AddText(1, ";");
        core.TestCharAdded();
        await RoundTripAsync(core);
        Expect(core, native, "{\r};^", "the posted callback cannot replay an already flushed adjustment");

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
        Expect(core, native, "{\r    };^", "an edit without CharAdded cancels the pending adjustment");

        Load(core, "csharp", "{\r    ^");
        native.AddText(1, "}");
        core.TestCharAdded();
        native.SetEmptySelection(0);
        native.AddText(1, ";");
        core.TestCharAdded();
        await RoundTripAsync(core);
        Expect(core, native, ";^{\r    }", "typing elsewhere cancels the pending adjustment");

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
        foreach (var (header, burst) in new[]
        {
            ("elif items[1", ":2]:"),
            ("elif items[\"a", ":b\"]:"),
            ("else", ": # note:")
        })
        {
            Load(core, "python", "if x:\r    f()\r    " + header + "^");
            foreach (var ch in burst)
            {
                native.AddText(1, ch.ToString());
                core.TestCharAdded();
            }
            await RoundTripAsync(core);
            Expect(core, native, "if x:\r    f()\r" + header + burst + "^", "Python burst uses the suite colon: " + header + burst);
        }
        await Type(core, native, "python", "if x:\r    if y:\r        a = 1\r# note\r\r        else^", ":",
            "if x:\r    if y:\r        a = 1\r# note\r\r    else:^", "Python else: after a comment line");
        await Type(core, native, "python", "if x:\r    a = 1\relse^", ":", "if x:\r    a = 1\relse:^", "Python else: already in place");
        await Type(core, native, "python", "if x:\r    a = 1\r    elsewhere^", ":", "if x:\r    a = 1\r    elsewhere:^", "Python elsewhere:");
        await Type(core, native, "python", "def f():\r    if x: pass\r    else^", ":",
            "def f():\r    if x: pass\r    else:^", "Python inline suite keeps an already aligned else inside its function");
        await Type(core, native, "python", "if (\r    x\r):\r    f()\r    else^", ":",
            "if (\r    x\r):\r    f()\relse:^", "Python else matches a continued header");
        await Type(core, native, "python", "def f():\r    try:\r        if x:\r            f()\r            except E^", ":",
            "def f():\r    try:\r        if x:\r            f()\r    except E:^", "Python except finds try across a nested suite");
        await Type(core, native, "python", "try:\r    f()\rexcept E:\r    g()\relse:\r    h()\r    finally^", ":",
            "try:\r    f()\rexcept E:\r    g()\relse:\r    h()\rfinally:^", "Python finally after try/except/else");
        await Type(core, native, "python", "if outer:\r    if inner:\r        f()\r    g()\r    else^", ":",
            "if outer:\r    if inner:\r        f()\r    g()\relse:^", "Python else skips a closed sibling suite");
        await Type(core, native, "python", "if x:\r    def f():\r        g()\r        else^", ":",
            "if x:\r    def f():\r        g()\r        else:^", "Python else never searches outside its function");
        await Type(core, native, "python", "if x:\r    f()\r    except E^", ":",
            "if x:\r    f()\r    except E:^", "Python except without a matching try is left alone");
        await Type(core, native, "python", "if x:\r    async  def f():\r        g()\r        else^", ":",
            "if x:\r    async  def f():\r        g()\r        else:^", "Python else never searches outside an async function");
        await Type(core, native, "python", "async def f():\r    async for x in xs:\r        g()\r        else^", ":",
            "async def f():\r    async for x in xs:\r        g()\r    else:^", "Python else matches an async loop");
    }

    private static void CheckReadOnly(TextEditorCore core, WinUIEditor.Editor native)
    {
        foreach (var disable in new[] { false, true })
        {
            foreach (var text in new[] { "^  a\r  b^", "  a^", "  ^a" })
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
                    core.TestDeleteBack();
                }
                finally
                {
                    core.IsEnabled = true;
                    native.ReadOnly = false;
                }
                Check(core.GetText() == before && native.Anchor == anchor && native.CurrentPos == caret && !core.CanUndo,
                    "Tab, Shift+Tab and Backspace leave a read-only editor unchanged: " + Escape(text) + (disable ? " (disabled)" : " (read-only)"));
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

    private static void CheckBackspace(TextEditorCore core, WinUIEditor.Editor native)
    {
        ApplicationPreferences.EditorDefaultTabIndents = 4;
        foreach (var (language, text, expected) in new[]
        {
            // At the end of the indentation: back to the previous unit stop.
            ("plaintext", "        ^abc", "    ^abc"),
            ("plaintext", "      ^abc", "    ^abc"),
            ("plaintext", "        ^", "    ^"),
            ("plaintext", "\t\t^abc", "\t^abc"),
            ("plaintext", "\t  ^abc", "\t^abc"),
            ("javascript", "  ^abc", "^abc"),
            ("python", "if x:\r  y\r  ^z", "if x:\r  y\r^z"),
            // Elsewhere: one character or the selection.
            ("plaintext", "    a^bc", "    ^bc"),
            ("plaintext", "  ^  abc", " ^  abc"),
            ("plaintext", "a\r^    b", "a^    b"),
            ("plaintext", "    ^abc^", "    ^"),
        })
        {
            Load(core, language, text);
            core.TestDeleteBack();
            Expect(core, native, expected, "Backspace: " + Escape(text));
        }
        // A unit wider than the tab width still removes the whole indentation.
        // A whitespace-only line is ignored by detection, so the setting wins.
        ApplicationPreferences.EditorDefaultTabIndents = 8;
        Load(core, "plaintext", "\t\t^");
        core.TestDeleteBack();
        Expect(core, native, "^", "Backspace with a unit wider than the tab width");
        ApplicationPreferences.EditorDefaultTabIndents = 4;
        Load(core, "plaintext", "        ^abc\r        def");
        core.TestDeleteBack();
        native.LineDown();
        Expect(core, native, "    abc\r    ^    def", "Down after a Backspace unindent keeps the new column");
        Load(core, "plaintext", "        ^abc");
        core.TestDeleteBack();
        core.Undo();
        Equal("        abc", core.GetText(), "a Backspace unindent is one undo step");
        // A zero-width rectangle is empty but is not a plain caret.
        Load(core, "plaintext", "        ^abc");
        native.SelectionMode = WinUIEditor.SelectionMode.Rectangle;
        native.RectangularSelectionAnchor = 8;
        native.RectangularSelectionCaret = 8;
        Check(native.SelectionEmpty, "zero-width rectangle fixture is empty");
        core.TestDeleteBack();
        native.SelectionMode = WinUIEditor.SelectionMode.Stream;
        Expect(core, native, "       ^abc", "Backspace deletes one character with a zero-width rectangle");
        // Ordinary deletion next to an unindent: three separate undo steps.
        Load(core, "plaintext", "\t x^");
        core.TestDeleteBack();
        Expect(core, native, "\t ^", "Backspace deletes a character after a tab-indented line");
        core.TestDeleteBack();
        Expect(core, native, "\t^", "Backspace removes the remaining space");
        core.TestDeleteBack();
        Expect(core, native, "^", "Backspace removes the tab");
        core.Undo();
        Expect(core, native, "\t^", "one undo restores the tab only");
        core.Undo();
        Expect(core, native, "\t ^", "another undo restores the space only");
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
        native.AddText(Encoding.UTF8.GetByteCount(typed), typed);
        core.TestCharAdded();
        await RoundTripAsync(core);
        Expect(core, native, expected, name);
    }

    private static async Task RoundTripAsync(TextEditorCore core)
    {
        // A low-priority sentinel may run before an idle adjustment that yielded
        // to input. Wait for completion instead of asserting an intermediate state.
        await core.Dispatcher.RunIdleAsync(_ => { });
        var deadline = DateTime.UtcNow.AddSeconds(5);
        while (core.TestTypedIndentationPending)
        {
            if (DateTime.UtcNow >= deadline) throw new TimeoutException("Typed indentation did not reach idle.");
            await Task.Delay(10);
        }
    }

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
