// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Notepads.Markdown;
using Notepads.Presentation.Controls.TextEditor;
using Notepads.Presentation.Theming;
using Windows.Foundation;
using Windows.Storage.Streams;
using Windows.UI.ViewManagement;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Automation.Provider;
using static NotepadsWorkspaceTests.DocumentFeatureAssertions;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal static class MarkdownMathCasesTests
{
    // Raw TeX keeps both row-ending backslashes visible. This repairs the lab
    // transcription, including its leading empty cell and final empty column.
    private static readonly string Aligned = """
        \begin{aligned}
        x   &= 0 \\
            &+ y \\
        z   &
        \end{aligned}
        """.Replace("\r\n", "\r", StringComparison.Ordinal).Replace('\n', '\r');

    private const string BareHan = "汉+x";
    private const string JoinedHan = "\\text{汉\u200D字}";
    private const string ChineseMean = @"\text{平均值}=\frac{1}{n}\sum_{i=1}^{n} x_i";
    private const string ChineseStyled = @"\frac{\textbf{\textnormal{繁體中文}}}{\text{样本（n=3）}}+x_{\text{\underline{测试}}}";
    private const string QuoteNorm = @"\left\lVert \vec v \right\rVert_2=\sqrt{\sum_{i=1}^d v_i^2}";
    private const string RealMember = @"x\in\mathbb{R}";
    private const string ListLimit = @"\lim_{x\to 0}\frac{\sin x}{x}=1";
    private const string Unknown = @"\thisCommandDoesNotExist{1}";
    private const string Continuation = "1+1=2";
    private const string UnrelatedControl = "\\text{汉\u202E字}";
    // Preserve the original B3 failure, rather than silently repairing invalid
    // input in the engine: U+0008, single row slashes and missing end slash.
    private const string CorruptedAligned = "\u0008egin{aligned}\rx   &= 0 \\\r    &+ y \\\rz   &\rend{aligned}";

    private sealed class FormulaExpectation
    {
        internal string Input;
        internal string State;
    }

    internal static async Task RunAsync(StringBuilder log)
    {
        var source = "# Math regression cases\r\r" +
            Display(Aligned) + "\r\r" +
            "Bare $" + BareHan + "$ and joined $" + JoinedHan + "$.\r\r" +
            "Mean $" + ChineseMean + "$.\r\r" + Display(ChineseStyled) + "\r\r" +
            "> 引用内的块公式\r>\r> $$\r> " + QuoteNorm + "\r> $$\r\r" +
            "1. 列表内的 $" + RealMember + "$。\r\r" +
            "   $$\r   " + ListLimit + "\r   $$\r\r" +
            "2. 公式后的第二项。\r\r" +
            "故意使用未知命令 $" + Unknown + "$。错误样例后应继续显示正文和正常公式 $" + Continuation + "$。\r\r" +
            "Unrelated control $" + UnrelatedControl + "$.\r\r" +
            Display(CorruptedAligned) + "\r";
        FormulaExpectation[] expected =
        [
            Formula(Aligned, display: true),
            Formula(BareHan),
            Formula(JoinedHan),
            Formula(ChineseMean),
            Formula(ChineseStyled, display: true),
            Formula(QuoteNorm, display: true),
            Formula(RealMember),
            Formula(ListLimit, display: true),
            Formula(Unknown, state: "Failed"),
            Formula(Continuation),
            Formula(UnrelatedControl, state: "Unsupported"),
            Formula(CorruptedAligned, display: true, state: "Failed")
        ];
        await RunCaseAsync("authored", source, expected, log, relayout: true);

        // A mixed overflow fixture reserves five of 128 residents for diagrams,
        // so separately prove successful native completion at ordinals 64/127.
        var boundaryInputs = Enumerable.Range(0, 128).Select(index => "x_{" + index + "}+1").ToArray();
        await RunCaseAsync("resident-boundary-128",
            "# Math resident boundary\r\r" + string.Join(" ", boundaryInputs.Select(input => "$" + input + "$")) + "\r",
            boundaryInputs.Select(input => Formula(input)).ToArray(), log);

        var overflowInputs = Enumerable.Range(0, 129).Select(index => "x_{" + index + "}+1").ToArray();
        await RunCaseAsync("resident-overflow-129",
            "# Math resident overflow\r\r" + string.Join(" ", overflowInputs.Select(input => "$" + input + "$")) + "\r",
            overflowInputs.Select((input, index) => Formula(input, state: index < 128 ? "Ready" : "Limited")).ToArray(), log);
    }

    private static string Display(string input) => "$$\r" + input + "\r$$";

    private static FormulaExpectation Formula(string input, bool display = false, string state = "Ready") =>
        new FormulaExpectation
        {
            Input = display ? "\n" + input.Replace('\r', '\n') + "\n" : input,
            State = state
        };

    private static async Task RunCaseAsync(string name, string source, FormulaExpectation[] expected,
        StringBuilder log, bool relayout = false)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var sourceBytes = Encoding.UTF8.GetBytes(source);
        var editor = await OpenAsync(host, "math-" + name + ".md", sourceBytes);
        var native = editor.TestNativeCore.TestFeatureNative;
        native.SetEmptySelection(0);
        var original = editor.GetText();
        Check(original == source, "The Math fixture opens the exact authored source, including control characters and TeX slashes.");
        AssertNativeSourceBytes(editor.TestNativeCore, sourceBytes, name + "-open");
        var sequence = native.DocumentSequence;
        var undo = native.UndoSequence;
        var canUndo = native.CanUndo();
        var canRedo = native.CanRedo();
        var modified = native.Modify;
        var editorModified = editor.IsModified;
        var window = Window.Current.CoreWindow.Bounds;
        var theme = ThemeSettingsService.ThemeMode;
        var sourceSelection = (native.Anchor, native.CurrentPos, native.SelectionRevision);
        var selectionEstablished = false;
        void AssertSource(string transition)
        {
            var selection = (native.Anchor, native.CurrentPos, native.SelectionRevision);
            AssertNativeSourceBytes(editor.TestNativeCore, sourceBytes, transition);
            Check(editor.GetText() == original && native.DocumentSequence == sequence &&
                native.UndoSequence == undo && native.CanUndo() == canUndo && native.CanRedo() == canRedo &&
                native.Modify == modified && editor.IsModified == editorModified &&
                (!selectionEstablished || sourceSelection == selection),
                "Math " + transition + " preserves exact source, the nonempty selection and direction, document sequence, Undo/Redo and save point.");
            log.AppendLine($"OBS: Math {transition} source selection={selection.Anchor}:{selection.CurrentPos}@{selection.SelectionRevision}, expected={sourceSelection.Anchor}:{sourceSelection.CurrentPos}@{sourceSelection.SelectionRevision}; sequence={sequence}, undo={undo}, modified={modified}.");
        }
        try
        {
            editor.ShowHideContentPreview();
            var extension = await MarkdownFeatureTests.CurrentAsync(editor);
            extension.FollowSource = false;
            var preview = extension.PreviewView;
            preview.TrySetVerticalScrollProgress(0);
            editor.Focus();
            native.SetSel(9, 2);
            sourceSelection = (native.Anchor, native.CurrentPos, native.SelectionRevision);
            selectionEstablished = true;
            Check(native.Anchor == 9 && native.CurrentPos == 2 && preview.FocusState == FocusState.Unfocused,
                "The Math fixture establishes a focused, backward nonempty source selection away from zero.");
            await AssertFormulasAsync(preview, expected, name, log);
            AssertSource(name + "-render");
            if (relayout)
            {
                var generation = preview.ThemeGeneration;
                ThemeSettingsService.SetTheme(theme == ElementTheme.Light ? ElementTheme.Dark : ElementTheme.Light);
                await UntilAsync(() => preview.ThemeGeneration != generation,
                    "The Math theme change reaches the real native preview.", 25000);
                await MarkdownFeatureTests.CurrentAsync(editor);
                await AssertFormulasAsync(preview, expected, name + "-theme", log);
                AssertSource(name + "-theme");
                Check(ApplicationView.GetForCurrentView().TryResizeView(new Size(900, 760)),
                    "The Math fixture admits its narrower window.");
                await UntilAsync(() => Math.Abs(Window.Current.CoreWindow.Bounds.Width - 900) < 0.6,
                    "The Math window settles at its narrower width.");
                await MarkdownFeatureTests.CurrentAsync(editor);
                await AssertFormulasAsync(preview, expected, name + "-width", log);
                AssertSource(name + "-width");
                editor.ShowHideContentPreview();
                await extension.DisposalCompletion.WaitAsync(TimeSpan.FromSeconds(20));
                editor.ShowHideContentPreview();
                extension = await MarkdownFeatureTests.CurrentAsync(editor);
                extension.FollowSource = false;
                await AssertFormulasAsync(extension.PreviewView, expected, name + "-reopen", log);
                AssertSource(name + "-reopen");
            }
            log.AppendLine("PASS: Markdown.MathCases " + name + " preserves every exact source-backed formula and expected native UIA outcome.");
        }
        finally
        {
            try { AssertSource(name + "-finally"); }
            finally
            {
                ThemeSettingsService.SetTheme(theme);
                ApplicationView.GetForCurrentView().TryResizeView(new Size(window.Width, window.Height));
            }
        }
    }

    private static async Task AssertFormulasAsync(MarkdownView preview, FormulaExpectation[] expected,
        string label, StringBuilder log)
    {
        MarkdownMermaidTests.DerivedObject[] objects = Array.Empty<MarkdownMermaidTests.DerivedObject>();
        try
        {
            await UntilAsync(() =>
            {
                objects = MarkdownMermaidTests.ReadObjects(preview);
                return objects.Length == expected.Length && objects.All(item => item.Family == "Formula" && item.State != "Pending");
            }, "Every authored Math object publishes a terminal native UIA state: " + label, 25000);
            Check(objects.Select(item => item.Name).SequenceEqual(expected.Select(item => "Formula: " + item.Input)) &&
                objects.Select(item => item.State).SequenceEqual(expected.Select(item => item.State)),
                "Every exact Math input has its independently expected Ready, Failed, Unsupported or Limited UIA state, with no missing or extra formulas: " + label);
            var peer = FrameworkElementAutomationPeer.FromElement(preview) ??
                FrameworkElementAutomationPeer.CreatePeerForElement(preview);
            var children = peer.GetChildren().Where(child => child.GetName().StartsWith("Formula:", StringComparison.Ordinal)).ToArray();
            for (var index = 0; index < expected.Length; index++)
                AssertFormulaReading(children[index], expected[index].Input, label + "-" + index, log);
            if (expected.Length >= 128)
            {
                Check(objects[64].State == "Ready" && objects[127].State == "Ready",
                    "The Math capacity fixture completes actual public UIA objects at ordinals 64 and 127: " + label);
                if (expected.Length == 129)
                    Check(objects.Count(item => item.State == "Ready") == 128 &&
                        objects.Count(item => item.State == "Limited") == 1 && objects[128].State == "Limited",
                        "The standalone 129-formula fixture renders the first 128 and limits only ordinal 128 while retaining its exact input and atomic UIA range: " + label);
            }
            else
                Check(preview.DisplayedText.Contains("错误样例后应继续显示正文和正常公式 \uFFFC。", StringComparison.Ordinal) &&
                    preview.DisplayedText.Contains("公式后的第二项。", StringComparison.Ordinal),
                    "Normal text, a valid formula after the unknown command, and the following list item remain visible: " + label);
        }
        finally
        {
            log.AppendLine($"OBS: Math {label} formulas={objects.Length}; " +
                string.Join(", ", objects.GroupBy(item => item.State).Select(group => group.Key + "=" + group.Count())));
            foreach (var item in objects)
                log.AppendLine("OBS: Math " + label + " '" + Escape(item.Name) + "': " + item.Help +
                    $" bounds={item.Bounds.X},{item.Bounds.Y},{item.Bounds.Width},{item.Bounds.Height}.");
        }
    }

    internal static void AssertFormulaReading(AutomationPeer peer, string expectedInput, string label, StringBuilder log)
    {
        var child = peer.GetPattern(PatternInterface.TextChild) as ITextChildProvider;
        // UIA text uses Main UTF16 coordinates, where each formula is one atomic
        // object. Semantic clipboard export is a separate native descriptor path.
        const string expectedText = "\uFFFC";
        var actualText = child?.TextRange?.GetText(-1);
        var expectedName = "Formula: " + expectedInput;
        var actualName = peer.GetName();
        var detail = $"Math UIA {label} expectedName='{Escape(expectedName)}', actualName='{Escape(actualName)}', expectedText='{Escape(expectedText)}', actualText='{Escape(actualText)}'.";
        log.AppendLine("OBS: " + detail);
        Check(child != null && actualText == expectedText && actualName == expectedName,
            "The actual Math UIA text-child range retains one atomic Main object and its exact original formula input. " + detail);
    }

    internal static void AssertNativeSourceBytes(TextEditorCore core, byte[] expected, string label)
    {
        Check(core.DocumentLength == expected.LongLength, "Math source preserves the exact native UTF8 byte length: " + label);
        for (long offset = 0; offset < core.DocumentLength;)
        {
            var chunk = core.ReadCurrentUtf8Range(offset,
                Math.Max(4, checked((int)Math.Min(65536, core.DocumentLength - offset))));
            Check(chunk.Length != 0, "The Math native source-byte check makes scalar-aligned progress: " + label);
            var actual = new byte[chunk.Length];
            using (var reader = DataReader.FromBuffer(chunk)) reader.ReadBytes(actual);
            Check(actual.AsSpan().SequenceEqual(expected.AsSpan(checked((int)offset), actual.Length)),
                $"Math source preserves every native UTF8 byte, including TeX delimiters, slashes and Unicode controls: {label} at byte {offset}.");
            offset += actual.Length;
        }
    }

    private static string Escape(string value) => value == null ? "<null>" : value.Replace("\r", "\\r", StringComparison.Ordinal)
        .Replace("\n", "\\n", StringComparison.Ordinal).Replace("\u0008", "\\u0008", StringComparison.Ordinal)
        .Replace("\u200D", "\\u200D", StringComparison.Ordinal).Replace("\u202E", "\\u202E", StringComparison.Ordinal)
        .Replace("\uFFFC", "\\uFFFC", StringComparison.Ordinal);
}
