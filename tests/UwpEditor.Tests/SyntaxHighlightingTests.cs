// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Diagnostics;
using System.IO;
using System.Text;
using System.Threading.Tasks;
using Notepads.Features.Documents.FileTypes;
using Notepads.Features.Documents.Storage;
using Notepads.Presentation.Controls.TextEditor;
using Notepads.Presentation.Theming;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;

namespace NotepadsEditorTests;

internal static class SyntaxHighlightingTests
{
    private static readonly (string Id, string Text, string Token, SyntaxColorRole Role)[] Samples =
    [
        ("cpp", "// note\rint count = 42;\rconst char* message = \"hi\";\r", "int", SyntaxColorRole.Keyword),
        ("c", "// note\rint count = 42;\r", "int", SyntaxColorRole.Keyword),
        ("csharp", "// note\rpublic class Example { string name = \"hi\"; }\r", "class", SyntaxColorRole.Keyword),
        ("java", "// note\rpublic class Example { int count = 42; }\r", "class", SyntaxColorRole.Keyword),
        ("javascript", "// note\rconst message = `hi`;\r", "const", SyntaxColorRole.Keyword),
        ("typescript", "// note\rinterface Example { name: string; }\r", "interface", SyntaxColorRole.Keyword),
        ("python", "# note\rdef greet():\r    return \"hi\"\r", "def", SyntaxColorRole.Keyword),
        ("json", "{\r  \"count\": 42,\r  \"ok\": true\r}\r", "count", SyntaxColorRole.Attribute),
        ("html", "<!-- note -->\r<div title=\"hi\">text</div>\r", "div", SyntaxColorRole.Markup),
        ("xml", "<!-- note -->\r<item title=\"hi\">text</item>\r", "item", SyntaxColorRole.Markup),
        ("css", "/* note */\rbody { color: red; content: \"hi\"; }\r", "hi", SyntaxColorRole.String),
        ("powershell", "# note\rfunction Test { Write-Host \"hi\" }\r", "function", SyntaxColorRole.Keyword),
        ("sql", "-- note\rSELECT 'hi' FROM items;\r", "SELECT", SyntaxColorRole.Keyword),
        ("yaml", "# note\rname: \"hi\"\rcount: 42\r", "name", SyntaxColorRole.Attribute),
        ("markdown", "# Title\r\rparagraph\r\r---\r\r`code`\r", "Title", SyntaxColorRole.Markup),
        ("rust", "// note\rfn main() { let n = 42; }\r", "fn", SyntaxColorRole.Keyword),
        ("bash", "# note\rif true; then\r  echo \"hi\"\rfi\r", "if", SyntaxColorRole.Keyword),
        ("lua", "-- note\rfunction greet()\r return \"hi\"\rend\r", "function", SyntaxColorRole.Keyword),
        ("toml", "# note\rname = \"hi\"\rcount = 42\r", "hi", SyntaxColorRole.String)
    ];

    public static async Task RunAsync(StringBuilder log, TextEditorCore core)
    {
        var view = (WinUIEditor.EditorBaseControl)((Grid)core.Content).Children[0];
        var native = view.Editor;
        native.WrapMode = WinUIEditor.Wrap.None;
        foreach (var sample in Samples)
        {
            Checkpoint("Language/newlines: " + sample.Id);
            var profile = SyntaxLanguageProfile.Create(sample.Id, "sample." + sample.Id);
            Configure(native, profile);
            native.SetText(sample.Text);
            native.Colourise(0, -1);
            var token = sample.Text.IndexOf(sample.Token, StringComparison.Ordinal);
            Check(profile.Tokens[native.GetStyleAt(token)] == sample.Role, sample.Id + " representative token role");
            var styles = ReadStyles(native);
            foreach (var ending in new[] { "\n", "\r\n" })
            {
                var variant = sample.Text.Replace("\r", ending, StringComparison.Ordinal);
                native.SetText(variant);
                native.Colourise(0, -1);
                var position = 0;
                for (var original = 0; original < sample.Text.Length; original++)
                {
                    if (sample.Text[original] != '\r')
                        Check(native.GetStyleAt(position) == styles[original], sample.Id + " CR/LF/CRLF token equivalence");
                    position += sample.Text[original] == '\r' ? ending.Length : 1;
                }
            }
        }

        var csharp = SyntaxLanguageProfile.Create("csharp", "sample.cs");
        Configure(native, csharp);
        const string rawString = "class Example { string json = \"\"\"\r{\"ok\":true}\r\"\"\"; }\r";
        native.SetText(rawString);
        native.Colourise(0, -1);
        Check(csharp.Tokens[native.GetStyleAt(rawString.IndexOf("ok", StringComparison.Ordinal))] == SyntaxColorRole.String,
            "C# multiline raw strings retain their string color");
        var html = SyntaxLanguageProfile.Create("html", "sample.html");
        Configure(native, html);
        const string script = "<script>const value = `hi`;</script>\r";
        native.SetText(script);
        native.Colourise(0, -1);
        Check(html.Tokens[native.GetStyleAt(script.IndexOf("const", StringComparison.Ordinal))] == SyntaxColorRole.Keyword &&
            html.Tokens[native.GetStyleAt(script.IndexOf("hi", StringComparison.Ordinal))] == SyntaxColorRole.String,
            "HTML embedded JavaScript keywords and template literals use mapped roles");
        Check(html.Tokens[127] == SyntaxColorRole.Operator, "HTML embedded PHP operators do not inherit string colors");

        // A blank CR line must separate a horizontal rule from its paragraph.
        Configure(native, SyntaxLanguageProfile.Create("markdown", "sample.md"));
        native.SetText("paragraph\r\r---\r");
        native.Colourise(0, -1);
        Check(native.GetStyleAt(11) == 17, "Markdown CR horizontal rule after blank line");

        // Incremental multiline state must converge to a fresh lex of the same text.
        Configure(native, SyntaxLanguageProfile.Create("cpp", "sample.cpp"));
        native.SetText("int before;\r/* open\rcomment\r*/ int after;\r");
        native.Colourise(0, -1);
        native.DeleteRange(28, 2);
        native.Colourise(28, -1);
        var incremental = ReadStyles(native);
        var edited = native.GetText(native.Length + 1);
        native.SetText(edited);
        native.Colourise(0, -1);
        EqualStyles(incremental, ReadStyles(native), "multiline incremental versus fresh coloring");

        core.SetSyntaxLanguage(DocumentLanguages.Find("csharp"), "sample.cs");
        await core.LoadTextAsync("public class Example { string name = \"hi\"; }\r");
        native.Colourise(0, -1);
        Check(native.GetStyleAt(7) == 5, "native document replacement reinstalls the language profile; actual=" + native.GetStyleAt(7));
        native.EmptyUndoBuffer();
        native.SetSavePoint();
        var version = core.ContentVersion;
        var before = core.GetText();
        core.SetSyntaxLanguage(DocumentLanguages.Find("python"), "sample.py");
        native.Colourise(0, -1);
        Check(core.ContentVersion == version && !native.Modify && !native.CanUndo() && core.GetText() == before,
            "language change keeps document bytes, version, dirtiness and undo");
        core.SetSyntaxLanguage(DocumentLanguages.PlainText, "sample.txt");
        native.Colourise(0, -1);
        foreach (var style in ReadStyles(native)) Check(style == 0, "Plain Text clears stale lexical styles");

        core.SetSyntaxLanguage(DocumentLanguages.Find("csharp"), "sample.cs");
        native.Colourise(0, -1);
        ThemeSettingsService.SetTheme(ElementTheme.Light);
        core.RequestedTheme = ElementTheme.Light;
        await Task.Delay(100);
        var light = native.StyleGetFore(5);
        Check(light == ToNativeColor(SyntaxColorPalette.ForTheme(ElementTheme.Light)[SyntaxColorRole.Keyword]),
            "light syntax color comes from theme resources");
        ThemeSettingsService.SetTheme(ElementTheme.Dark);
        core.RequestedTheme = ElementTheme.Dark;
        await Task.Delay(100);
        Check(native.StyleGetFore(5) != light, "theme changes reapply syntax colors");
        var dark = native.StyleGetFore(5);
        var otherThreadColor = await Task.Run(() => SyntaxColorPalette.ForTheme(ElementTheme.Light)[SyntaxColorRole.Keyword]);
        Check(ToNativeColor(otherThreadColor) == light, "cached palette contains values usable across window apartments");
        var styledBeforeZoom = native.EndStyled;
        core.SetFontZoomFactor(110);
        core.SetFontZoomFactor(100);
        Check(native.StyleGetFore(5) == dark && native.EndStyled == styledBeforeZoom, "zoom keeps palette and lexer progress");

        CheckAdmission(native);
        await CheckWrappedLanguageChangeAsync(core, native);
        Checkpoint("Streamed replacement");
        await CheckStreamedReplacementAsync(core, native);
#if DEBUG
        Checkpoint("Optional allocation failures");
        await CheckStyleAllocationFailureAsync(core, native);
        await CheckLexerAllocationFailureAsync(core, native);
        await CheckJournalStyleFailureAsync(core, native);
#endif
        await MeasureVisibleStylingAsync(log, core, native);
        core.SetSyntaxLanguage(DocumentLanguages.Find("python"), "sample.py");
        native.SetText("def greet():\r    return 'hi'\r");
        native.Colourise(0, 0);
        await Task.Delay(250);
        Check(native.EndStyled > 0 && native.GetStyleAt(0) == 5, "native idle styling makes visible text ready without explicit Colourise");
        log.AppendLine("PASS: native syntax: 19 language profiles; CR/LF/CRLF; incremental/reloaded styling; metadata-only selection; theme resources/zoom; bounded admission, edits and undo/redo; visible idle completion.");
    }

    private static async Task CheckStreamedReplacementAsync(TextEditorCore core, WinUIEditor.Editor native)
    {
        core.SetSyntaxLanguage(DocumentLanguages.Find("json"), "sample.json");
        var bytes = Encoding.UTF8.GetBytes("{\"ok\":true}\r");
        using var baseline = await DocumentBaseline.CreateAsync(stream => stream.WriteAsync(bytes).AsTask());
        await core.LoadBaselineAsync(baseline, false);
        native.Colourise(0, -1);
        Check(native.GetStyleAt(6) == 11, "streamed baseline attaches optional style storage");
        native.EmptyUndoBuffer();
        native.SetSel(native.Length, native.Length);
        native.PasteText(" ");
        await core.LoadBaselineAsync(baseline, true);
        native.Colourise(0, -1);
        Check(native.GetStyleAt(6) == 11 && native.CanUndo(), "preserve-undo replacement reinstalls optional style storage");
        native.Undo();
        Check(core.GetText().EndsWith(" ", StringComparison.Ordinal), "styled replacement preserves whole-document undo");
        native.Redo();
        Check(core.GetText() == "{\"ok\":true}\r", "styled replacement preserves whole-document redo");
    }

#if DEBUG
    private static async Task CheckStyleAllocationFailureAsync(TextEditorCore core, WinUIEditor.Editor native)
    {
        const int faultOperation = 0x4E500001;
        core.SetSyntaxLanguage(DocumentLanguages.Find("json"), "sample.json");
        var bytes = Encoding.UTF8.GetBytes("{\"ok\":true}\r");
        using var baseline = await DocumentBaseline.CreateAsync(stream => stream.WriteAsync(bytes).AsTask());
        native.PrivateLexerCall(faultOperation, 1);
        await core.LoadBaselineAsync(baseline, false);
        Check(native.SyntaxHighlightingPauseReason == 4 && core.GetText() == "{\"ok\":true}\r",
            "optional allocation failure after streamed publication preserves a successful load");
        native.SetSel(native.Length, native.Length);
        native.PasteText(" ");
        native.Colourise(0, -1);
        Check(native.SyntaxHighlightingPauseReason == 4, "memory pause is latched through ordinary editing/painting");
        core.RetrySyntaxHighlighting();
        Check(native.SyntaxHighlightingPauseReason == 0, "explicit language retry resumes after memory pressure");

        native.BeginUndoAction();
        native.PrivateLexerCall(faultOperation, 2);
        native.PasteText(" ");
        native.EndUndoAction();
        Check(native.SyntaxHighlightingPauseReason == 4 && core.GetText().EndsWith("  ", StringComparison.Ordinal),
            "optional style growth failure does not fail text insertion");
        native.Undo();
        Check(core.GetText().EndsWith(" ", StringComparison.Ordinal) && native.SyntaxHighlightingPauseReason == 4,
            "memory fallback retains undo and avoids allocation retries");
        native.Redo();
        Check(core.GetText().EndsWith("  ", StringComparison.Ordinal), "memory fallback retains redo");

        await core.LoadBaselineAsync(baseline, false);
        Check(native.SyntaxHighlightingPauseReason == 0, "complete reload retries optional syntax memory");
        native.PrivateLexerCall(faultOperation, 1);
        await core.LoadBaselineAsync(baseline, true);
        Check(native.SyntaxHighlightingPauseReason == 4 && native.CanUndo() && core.GetText() == "{\"ok\":true}\r",
            "post-publication style allocation failure preserves a successful replacement and undo");
        core.RetrySyntaxHighlighting();
    }

    private static async Task CheckJournalStyleFailureAsync(TextEditorCore core, WinUIEditor.Editor native)
    {
        core.SetSyntaxLanguage(DocumentLanguages.Find("json"), "sample.json");
        var bytes = Encoding.UTF8.GetBytes("{\"ok\":true}\r");
        using var baseline = await DocumentBaseline.CreateAsync(stream => stream.WriteAsync(bytes).AsTask());
        await core.LoadBaselineAsync(baseline, false);
        var folder = await Windows.Storage.ApplicationData.Current.LocalFolder.CreateFolderAsync(
            "SyntaxJournal-" + Guid.NewGuid().ToString("N"));
        try
        {
            var file = await folder.CreateFileAsync("active.npj");
            native.StartJournal(file.Path, 0);
            native.PrivateLexerCall(0x4E500001, 2);
            native.SetSel(native.Length, native.Length);
            native.PasteText(" ");
            Check(native.SyntaxHighlightingPauseReason == 4 && native.DocumentSequence == 1,
                "optional growth failure does not interrupt journal commit");
            native.Undo();
            Check(native.DocumentSequence == 2 && core.GetText() == "{\"ok\":true}\r",
                "journal undo remains consistent after optional allocation failure");
            native.PrivateLexerCall(0x4E500001, 1);
            await core.LoadBaselineAsync(baseline, true);
            Check(native.SyntaxHighlightingPauseReason == 4 && native.DocumentSequence == 3,
                "optional post-publication failure does not interrupt replacement journal commit");
            for (var point = 1; point <= 5; point++)
            {
                native.PrivateLexerCall(0x4E500002, (ulong)point);
                await core.LoadBaselineAsync(baseline, true);
                Check(native.SyntaxHighlightingPauseReason == 4 && native.DocumentSequence == (ulong)(3 + point),
                    "complete optional configuration failure preserves journal commit at point " + point);
            }
            using var checkpoint = native.AcquireJournalCheckpoint();
            await checkpoint.FlushAsync();
            using var verified = await WinUIEditor.EditorJournalCheckpoint.OpenAsync(checkpoint.FilePath,
                checkpoint.BaseSequence, checkpoint.CommittedSequence, checkpoint.CommittedByteLength, checkpoint.PrefixSha256);
            Check(verified.CommittedSequence == 8 && verified.CommittedDocumentByteLength == (ulong)bytes.Length,
                "journal prefix remains valid after optional syntax allocation failures");
        }
        finally
        {
            await native.StopJournalAsync();
            await folder.DeleteAsync(Windows.Storage.StorageDeleteOption.PermanentDelete);
        }
        core.RetrySyntaxHighlighting();
    }

    private static async Task CheckLexerAllocationFailureAsync(TextEditorCore core, WinUIEditor.Editor native)
    {
        const int faultOperation = 0x4E500002;
        core.SetSyntaxLanguage(DocumentLanguages.Find("json"), "sample.json");
        var bytes = Encoding.UTF8.GetBytes("{\"ok\":true}\r");
        using var baseline = await DocumentBaseline.CreateAsync(stream => stream.WriteAsync(bytes).AsTask());
        for (var point = 1; point <= 5; point++)
        {
            // Shared state, factory, lex interface, keywords and properties are optional.
            var version = core.ContentVersion;
            var textChanged = 0;
            void OnTextChanged(object sender, RoutedEventArgs args) => textChanged++;
            core.TextChanged += OnTextChanged;
            try
            {
                native.PrivateLexerCall(faultOperation, (ulong)point);
                await core.LoadBaselineAsync(baseline, false);
                Check(native.SyntaxHighlightingPauseReason == 4 && core.GetText() == "{\"ok\":true}\r" &&
                    core.ContentVersion > version && textChanged == 1,
                    "post-publication configuration failure preserves successful load and notification at point " + point);
                native.SetSel(native.Length, native.Length);
                native.PasteText(" ");
                Check(native.SyntaxHighlightingPauseReason == 4, "configuration failure is latched through edits");
                core.RetrySyntaxHighlighting();
                native.Colourise(0, -1);
                Check(native.SyntaxHighlightingPauseReason == 0 && native.GetStyleAt(6) == 11,
                    "explicit retry restores a complete configuration after point " + point);
            }
            finally { core.TextChanged -= OnTextChanged; }
        }

        core.SetSyntaxLanguage(DocumentLanguages.Find("cpp"), "sample.cpp");
        await core.LoadTextAsync("int before;\r/* note */\rint after;\r");
        for (var point = 6; point <= 7; point++)
        {
            native.PrivateLexerCall(faultOperation, (ulong)point);
            native.Colourise(0, -1);
            Check(native.SyntaxHighlightingPauseReason == 4 && native.EndStyled == native.Length,
                "Lex/Fold allocation failure pauses with complete plain styling at point " + point);
            foreach (var style in ReadStyles(native)) Check(style == 0, "OOM clears partially colored styles");
            await Task.Delay(50);
            core.RetrySyntaxHighlighting();
            native.Colourise(0, -1);
            Check(native.SyntaxHighlightingPauseReason == 0 && native.GetStyleAt(0) == 5,
                "Lex/Fold failure leaves styling reentrancy state reusable at point " + point);
        }

        native.PrivateLexerCall(faultOperation, 6);
        native.InsertText(native.PositionFromLine(1), " ");
        var timer = Stopwatch.StartNew();
        while (native.SyntaxHighlightingPauseReason != 4 && timer.ElapsedMilliseconds < 10000) await Task.Delay(16);
        Check(native.SyntaxHighlightingPauseReason == 4 && native.EndStyled == native.Length,
            "idle Lex allocation failure is contained before returning through the timer ABI");
        core.RetrySyntaxHighlighting();
        native.Colourise(0, -1);
        Check(native.GetStyleAt(0) == 5, "idle failure can recover through an explicit retry");

        // Previously colored, nonvisible regions must also lose stale styles.
        native.WrapMode = WinUIEditor.Wrap.Word;
        var source = new StringBuilder();
        for (var line = 0; line < 200; line++) source.Append("int value = 42; /* " + new string('x', 1000) + " */\r");
        await core.LoadTextAsync(source.ToString());
        native.Colourise(0, -1);
        await Task.Delay(100);
        var wrappedLines = native.WrapCount(0);
        Check(wrappedLines > 1, "OOM wrap fixture establishes multiline layout");
        Check(native.GetStyleAt(native.PositionFromLine(199)) == 5, "distant fixture starts fully colored");
        for (var point = 6; point <= 7; point++)
        {
            native.PrivateLexerCall(faultOperation, (ulong)point);
            native.InsertText(0, " ");
            var wait = Stopwatch.StartNew();
            while (native.SyntaxHighlightingPauseReason != 4 && wait.ElapsedMilliseconds < 10000) await Task.Delay(16);
            Check(native.SyntaxHighlightingPauseReason == 4 && native.EndStyled == native.Length && native.Lexer == 0,
                "paint/idle failure retires the failed lexer at point " + point);
            foreach (var style in ReadStyles(native)) Check(style == 0, "idle OOM clears all previous regions");
            Check(native.WrapCount(0) == wrappedLines, "OOM cleanup preserves word-wrap heights and scroll mapping");
            core.RetrySyntaxHighlighting();
            native.Colourise(0, -1);
            Check(native.GetStyleAt(native.PositionFromLine(199)) == 5, "distant styles recover after retry");
        }
        native.WrapMode = WinUIEditor.Wrap.None;
    }
#endif

    private static async Task CheckWrappedLanguageChangeAsync(TextEditorCore core, WinUIEditor.Editor native)
    {
        core.SetSyntaxLanguage(DocumentLanguages.Find("cpp"), "sample.cpp");
        native.WrapMode = WinUIEditor.Wrap.Word;
        await core.LoadTextAsync("int value = 42; /* " + new string('x', 2000) + " */\rint tail;\r");
        native.Colourise(0, -1);
        await Task.Delay(100);
        var wrappedLines = native.WrapCount(0);
        Check(wrappedLines > 1, "language change fixture establishes multiline layout");
        Configure(native, SyntaxLanguageProfile.Create("json", "sample.json"));
        Check(native.WrapCount(0) == wrappedLines, "native profile switching preserves existing wrap heights");
        Configure(native, SyntaxLanguageProfile.Create("plaintext", "sample.txt"));
        Check(native.WrapCount(0) == wrappedLines, "Plain Text switching preserves existing wrap heights");
        native.WrapMode = WinUIEditor.Wrap.None;
    }

    private static async Task MeasureVisibleStylingAsync(StringBuilder log, TextEditorCore core, WinUIEditor.Editor native)
    {
        Checkpoint("Visible styling measurement");
        core.SetSyntaxLanguage(DocumentLanguages.Find("cpp"), "sample.cpp");
        var source = new StringBuilder();
        for (var line = 0; line < 25000; line++) source.Append("int value = 42; /* note */\r");
        var bytes = Encoding.UTF8.GetBytes(source.ToString());
        using var baseline = await DocumentBaseline.CreateAsync(stream => stream.WriteAsync(bytes).AsTask());
        var memory = Windows.System.MemoryManager.AppMemoryUsage;
        var timer = Stopwatch.StartNew();
        await core.LoadBaselineAsync(baseline, false);
        while (native.GetStyleAt(0) != 5 && timer.ElapsedMilliseconds < 10000) await Task.Delay(16);
        Check(native.GetStyleAt(0) == 5, "moderate source becomes styled through native idle scheduling");
        var initial = timer.ElapsedMilliseconds;
        var initialStyled = native.EndStyled;
        Check(initialStyled < native.Length, "initial styling does not unconditionally color the complete document");
        timer.Restart();
        var distant = native.PositionFromLine(24900);
        native.GotoPos(distant);
        while (native.GetStyleAt(distant) != 5 && timer.ElapsedMilliseconds < 10000) await Task.Delay(16);
        Check(native.GetStyleAt(distant) == 5, "distant scrolling converges through native idle scheduling");
        var scroll = timer.ElapsedMilliseconds;
        timer.Restart();
        native.SetSel(distant, distant + 3);
        native.PasteText("float");
        while (native.GetStyleAt(distant) != 5 && timer.ElapsedMilliseconds < 10000) await Task.Delay(16);
        Check(native.GetStyleAt(distant) == 5, "visible edit converges to keyword coloring");
        log.AppendLine($"MEASURE: syntax {bytes.Length} bytes / 25,000 lines; load+visible={initial} ms; initially styled={initialStyled} bytes; distant scroll={scroll} ms; edit={timer.ElapsedMilliseconds} ms; app-memory delta={(long)Windows.System.MemoryManager.AppMemoryUsage - (long)memory} bytes. No frame-rate/size guarantee.");
    }

    private static void CheckAdmission(WinUIEditor.Editor native)
    {
        Checkpoint("Admission: long lines/undo");
        Configure(native, SyntaxLanguageProfile.Create("json", "sample.json"));
        var longLine = new string('x', 65537);
        native.SetText("{}\r" + longLine + "\r{}\r" + longLine + "\r{}");
        Check(native.SyntaxHighlightingPauseReason == 3, "multiple long lines pause coloring");
        native.Colourise(0, -1);
        Check(native.GetStyleAt(0) == 0, "paused lexer supplies plain styles");
        native.InsertText(0, "{}\r");
        Check(native.SyntaxHighlightingPauseReason == 3, "insertion before long lines retains pause");
        var first = native.PositionFromLine(2);
        native.DeleteRange(first, 3);
        Check(native.SyntaxHighlightingPauseReason == 3, "repairing one of multiple long lines retains pause");
        var second = native.PositionFromLine(4);
        native.DeleteRange(second, 3);
        Check(native.SyntaxHighlightingPauseReason == 0, "repairing every long line resumes coloring");
        native.Undo();
        Check(native.SyntaxHighlightingPauseReason == 3, "undo restores admission pause");
        native.Redo();
        Check(native.SyntaxHighlightingPauseReason == 0, "redo resumes coloring");
        native.DeleteRange(0, 3);
        Check(native.SyntaxHighlightingPauseReason == 0, "line deletion before repaired offenders shifts admission indices");

        native.SetText(new string('x', 40000) + "\r" + new string('x', 40000));
        Check(native.SyntaxHighlightingPauseReason == 0, "individual eligible lines remain admitted");
        native.DeleteRange(40000, 1);
        Check(native.SyntaxHighlightingPauseReason == 3, "line merging pauses before lexing the merged long line");
        native.Undo();
        Check(native.SyntaxHighlightingPauseReason == 0, "undo split restores admission");

        native.UndoCollection = false;
        Checkpoint("Admission: line-count boundary");
        native.SetText(new string('\r', 200000));
        Check(native.SyntaxHighlightingPauseReason == 2, "native line-count budget pauses coloring");
        native.DeleteRange(0, 1);
        Check(native.SyntaxHighlightingPauseReason == 0, "native line-count budget resumes after repair");
        Checkpoint("Admission: byte-count boundary with bounded lines");
        var fixture = new StringBuilder(32 * 1024 * 1024 + 1);
        var line = new string('x', 1023) + "\r";
        for (var index = 0; index < 32768; index++) fixture.Append(line);
        fixture.Append('x');
        native.SetText(fixture.ToString());
        Check(native.SyntaxHighlightingPauseReason == 1, "canonical byte budget pauses before line scan");
        native.SetText("{\"ok\":true}");
        Check(native.SyntaxHighlightingPauseReason == 0, "small replacement resumes a paused lexer");
        native.Colourise(0, -1);
        Check(native.GetStyleAt(6) == 11, "resumed lexer retains keyword configuration");
        native.UndoCollection = true;
    }

    private static void Configure(WinUIEditor.Editor native, SyntaxLanguageProfile profile)
    {
        native.SetLexerLanguage(profile.Lexer, profile.Keywords, profile.PropertyNames, profile.PropertyValues);
    }

    private static int[] ReadStyles(WinUIEditor.Editor native)
    {
        var styles = new int[checked((int)native.Length)];
        for (var index = 0; index < styles.Length; index++) styles[index] = native.GetStyleAt(index);
        return styles;
    }

    private static void EqualStyles(int[] expected, int[] actual, string name)
    {
        Check(expected.Length == actual.Length, name + " length");
        for (var index = 0; index < expected.Length; index++) Check(expected[index] == actual[index], name + " at " + index);
    }

    private static int ToNativeColor(Windows.UI.Color color) => color.R | color.G << 8 | color.B << 16;
    private static void Checkpoint(string message) => File.WriteAllText(Path.Combine(
        Windows.Storage.ApplicationData.Current.LocalFolder.Path, "diagnostic.txt"), "Syntax: " + message);
    private static void Check(bool condition, string name)
    {
        if (!condition) throw new Exception(name);
    }
}
