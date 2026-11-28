// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Text;
using System.Threading.Tasks;
using Notepads.Markdown;
using Notepads.Presentation.Controls.Markdown;
using Notepads.Presentation.Controls.TextEditor;
using NotepadsWorkspaceTests;
using Windows.Storage;
using Windows.Storage.Streams;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Automation.Provider;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Input;
using Windows.UI.Xaml.Markup;
using Windows.UI.Xaml.Media;
using Windows.UI.ViewManagement;
using static NotepadsIntegrationTests.FeatureUI;
using static NotepadsWorkspaceTests.DocumentFeatureAssertions;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal static partial class MarkdownFeatureTests
{
    private static async Task MathEscapeLinkAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        const string label = "转义 [a] 完整";
        var markdown = "[转义 \\[a\\] 完整](#target)\r\r\\[\ra\r\\]\r\r" +
            string.Join("\r\r", Enumerable.Repeat("Viewport separation.", 48)) + "\r\r## Target";
        var editor = await OpenAsync(host, "math-escape-link.md", Encoding.UTF8.GetBytes(markdown));
        editor.ShowHideContentPreview();
        var preview = (await CurrentAsync(editor)).PreviewView;
        Check(editor.GetText() == markdown && preview.DisplayedText.StartsWith(label, StringComparison.Ordinal) &&
            preview.DisplayedText.Count(character => character == '\uFFFC') == 1,
            "The link retains escaped brackets and only the standalone display creates a formula object.");

        var peer = FrameworkElementAutomationPeer.FromElement(preview) ?? FrameworkElementAutomationPeer.CreatePeerForElement(preview);
        var link = peer.GetChildren().Single(child => child.GetAutomationControlType() == AutomationControlType.Hyperlink);
        Check(link.GetName().Contains(label, StringComparison.Ordinal), "The preview exposes one complete link label.");
        string destination = null;
        preview.LinkRequested += (_, args) => destination = args.Destination;
        ((IInvokeProvider)link.GetPattern(PatternInterface.Invoke)).Invoke();
        Check(destination == "#target", "Invoking the link retains its fragment destination.");
        log.AppendLine("PASS: Markdown.MathEscapeLink preserves one visible hyperlink and one standalone display formula.");
    }

    private static async Task MathAliasesAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        const string inline = @"\sqrt{1+x^2}";
        const string display = @"\frac{1}{n}\sum_{k=1}^{n}k";
        var backticks = "$`" + inline + "`$";
        var fenced = "```math\r" + display + "\r```\r";
        var markdown = "前 " + backticks + " 后。\r\r" + fenced +
            "\r```mathematics\rordinary code\r```";
        var editor = await OpenAsync(host, "math-aliases.md", Encoding.UTF8.GetBytes(markdown));
        editor.ShowHideContentPreview();
        var preview = (await CurrentAsync(editor)).PreviewView;
        var peer = FrameworkElementAutomationPeer.FromElement(preview) ??
            FrameworkElementAutomationPeer.CreatePeerForElement(preview);
        const string rendered = "Formula rendered. Copy exports the complete original source.";
        await UntilAsync(() =>
        {
            var objects = peer.GetChildren();
            return objects != null &&
                objects.Any(child => child.GetName() == "Formula: " + inline && child.GetHelpText() == rendered) &&
                objects.Any(child => child.GetName() == "Formula: " + display + "\n" && child.GetHelpText() == rendered);
        }, "The inline and fenced formulas both reach native rendered state.");

        var children = peer.GetChildren();
        var compact = children.Single(child => child.GetName() == "Formula: " + inline).GetBoundingRectangle();
        var block = children.Single(child => child.GetName() == "Formula: " + display + "\n").GetBoundingRectangle();
        Check(compact.Width > 0 && block.Width > 0 && block.Height > compact.Height && block.Y > compact.Y,
            "The fenced formula reserves its own display row and the inline formula remains in the surrounding text.");
        var plain = preview.DisplayedText;
        Check(plain.StartsWith("前 \uFFFC 后。\n\uFFFC\n", StringComparison.Ordinal) &&
            plain.Contains("ordinary code", StringComparison.Ordinal) &&
            plain.Count(character => character == '\uFFFC') == 2 && editor.GetText() == markdown,
            "The two formula objects preserve the exact editor text and unrelated code fence.");

        var state = preview.GetSelection();
        var position = (ulong)plain.LastIndexOf('\uFFFC');
        // Both seams also map adjacent syntax. Select the inward affinity at each end.
        var opening = await preview.MapPreviewSpanAsync(state.DocumentId, state.Revision, state.TextGeneration,
            position, position, MarkdownPositionAffinity.After);
        var closing = await preview.MapPreviewSpanAsync(state.DocumentId, state.Revision, state.TextGeneration,
            position + 1, position + 1, MarkdownPositionAffinity.Before);
        var start = (ulong)Encoding.UTF8.GetByteCount(markdown.AsSpan(0, markdown.IndexOf(fenced, StringComparison.Ordinal)));
        Check(opening.Start.SourceByteOffset == start &&
            closing.End.SourceByteOffset == start + (ulong)Encoding.UTF8.GetByteCount(fenced),
            "The display object maps to the entire authored math fence.");
        log.AppendLine("PASS: Markdown.MathAliases verifies native inline and fenced formulas, display geometry and exact source mapping.");
    }

    private static async Task MathRenderingAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        const string inline = @"a\quad b";
        const string display = @"\int_0^1 f(x)\,dx\qquad\sum_{n=1}^{\infty}\frac{1}{n^2}";
        const string operatorName = @"\operatorname{Cov}(X,Y)";
        const string operatorLimits = @"\operatorname*{argmax}_{x} f(x)";
        const string operatorSymbols = @"\operatorname{A\&\Gamma\alpha}";
        const string matrix = @"\begin{matrix}1&22\\33&4\end{matrix}";
        const string inlineCases = @"\begin{cases}x&x>0\\0&x\le 0\end{cases}";
        const string displayCases = @"f(x)=\begin{cases}x^2&\text{if }x<0\\0&\text{if }x=0\\x&\text{if }x>0\end{cases}";
        const string inlineAligned = @"\begin{aligned}a&=b\\aa&=cc\end{aligned}";
        const string displayAligned = @"\begin{aligned}a+b&=c&d+e&=f\\a&=b&d&=-e\\x&=\frac{1}{2}\end{aligned}";
        const string inlineVector = @"\vec{v}_i";
        const string displayVectors = @"\vec{AB}+\vec{\frac{1}{2}}+\vec{\vec{x}}";
        const string inlineBrace = @"\overbrace{x+y}^{\text{sum}}_i";
        const string unicodeBrace = @"\mathbf{\overbrace{𝑥}}";
        const string standaloneBrace = @"\underbrace{x_1+x_2+\cdots+x_n}_{n\;\text{terms}}";
        const string displayBrace = @"\begin{aligned}" + "\n" +
            @"(a+b)^2 &= (a+b)(a+b) \\" + "\n" +
            @"        &= a^2 + 2ab + b^2" + "\n" +
            @"\end{aligned}" + "\n" +
            @"\quad" + "\n" +
            standaloneBrace;
        var delimitedMatrices = string.Join(@"\quad ", new[] { "pmatrix", "bmatrix", "Bmatrix", "vmatrix", "Vmatrix" }
            .Select(name => $@"\begin{{{name}}}a&b\\c&d\\e&f\end{{{name}}}"));
        var markdown = $"Inline ${inline}$ and ${operatorName}$.\r\rSymbols ${operatorSymbols}$." +
            $"\r\r$$\r{display}\r$$\r\r$$\r{operatorLimits}\r$$\r\r$$\r{matrix}\r$$" +
            $"\r\r$$\r{delimitedMatrices}\r$$\r\rPiecewise ${inlineCases}$.\r\r$$\r{displayCases}\r$$" +
            $"\r\rAligned ${inlineAligned}$.\r\r$$\r{displayAligned}\r$$" +
            $"\r\rVector ${inlineVector}$.\r\r$$\r{displayVectors}\r$$" +
            $"\r\rBrace ${inlineBrace}$ and ${unicodeBrace}$.\r\r$$\r{standaloneBrace}\r$$\r\r$$\r{displayBrace.Replace('\n', '\r')}\r$$";
        var editor = await OpenAsync(host, "math-rendering.md", Encoding.UTF8.GetBytes(markdown));
        editor.ShowHideContentPreview();
        var preview = (await CurrentAsync(editor)).PreviewView;
        var peer = FrameworkElementAutomationPeer.FromElement(preview) ?? FrameworkElementAutomationPeer.CreatePeerForElement(preview);
        try
        {
            await UntilAsync(() =>
            {
                var ready = peer.GetChildren()?.Where(child =>
                        child.GetHelpText() == "Formula rendered. Copy exports the complete original source.")
                    .Select(child => child.GetName()).ToArray();
                return ready != null && ready.Contains($"Formula: {inline}") &&
                    ready.Contains($"Formula: \n{display}\n") &&
                    ready.Contains($"Formula: {operatorName}") &&
                    ready.Contains($"Formula: {operatorSymbols}") &&
                    ready.Contains($"Formula: \n{operatorLimits}\n") &&
                    ready.Contains($"Formula: \n{matrix}\n") &&
                    ready.Contains($"Formula: \n{delimitedMatrices}\n") &&
                    ready.Contains($"Formula: {inlineCases}") &&
                    ready.Contains($"Formula: \n{displayCases}\n") &&
                    ready.Contains($"Formula: {inlineAligned}") &&
                    ready.Contains($"Formula: \n{displayAligned}\n") &&
                    ready.Contains($"Formula: {inlineVector}") &&
                    ready.Contains($"Formula: \n{displayVectors}\n") &&
                    ready.Contains($"Formula: {inlineBrace}") &&
                    ready.Contains($"Formula: {unicodeBrace}") &&
                    ready.Contains($"Formula: \n{standaloneBrace}\n") &&
                    ready.Contains($"Formula: \n{displayBrace}\n");
            }, "Fixed spacing, operators, matrices, piecewise formulas, alignment, vectors, and braces reach the rendered math state.");
        }
        catch (TimeoutException)
        {
            log.AppendLine($"MathRendering displayed {preview.DisplayedText.Count(character => character == '\uFFFC')} formula objects.");
            foreach (var child in peer.GetChildren() ?? Array.Empty<AutomationPeer>())
                log.AppendLine($"MathFixedSpacing observed '{child.GetName()}': {child.GetHelpText()}");
            throw;
        }
        log.AppendLine("PASS: Markdown.MathFixedSpacing renders quad and qquad in the native preview.");
        log.AppendLine("PASS: Markdown.MathOperatorNames renders a Roman inline operator and a starred display operator.");
        log.AppendLine("PASS: Markdown.MathPlainMatrix renders a plain matrix with the exact TeX input.");
        log.AppendLine("PASS: Markdown.MathDelimitedMatrices renders five tall delimited matrices with exact TeX input.");
        log.AppendLine("PASS: Markdown.MathCases renders inline and display piecewise formulas with exact TeX input.");
        log.AppendLine("PASS: Markdown.MathAligned renders inline and display aligned formulas with exact TeX input.");
        log.AppendLine("PASS: Markdown.MathVector renders inline and display vector formulas with exact TeX input.");
        log.AppendLine("PASS: Markdown.MathBraceAnnotations renders inline and display braces with exact TeX input.");
    }

    private static async Task ChineseMathTextAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        const string inline = @"\text{平均值}=\frac{1}{n}\sum_{i=1}^{n} x_i";
        const string display = @"\frac{\textbf{\textnormal{繁體中文}}}{\text{样本（n=3）}}+x_{\text{\underline{测试}}}";
        const string joined = "\\text{汉\u200D字}";
        const string unsupported = "\\text{汉\u202E字}";
        const string invalid = @"\frac{1}{";
        var markdown = $"Mean ${inline}$.\r\r$$\r{display}\r$$\r\rJoined ${joined}$.\r\rControl ${unsupported}$.\r\rBroken ${invalid}$.";
        var editor = await OpenAsync(host, "chinese-math-text.md", Encoding.UTF8.GetBytes(markdown));
        editor.ShowHideContentPreview();
        var preview = (await CurrentAsync(editor)).PreviewView;
        var peer = FrameworkElementAutomationPeer.FromElement(preview) ?? FrameworkElementAutomationPeer.CreatePeerForElement(preview);
        const string ready = "Formula rendered. Copy exports the complete original source.";
        const string fallback = "Formula is not supported. Copy exports the complete original source.";
        const string syntaxError = "Formula syntax error. Copy exports the complete original source.";
        try
        {
            await UntilAsync(() =>
            {
                var objects = peer.GetChildren();
                return objects != null &&
                    objects.Count(child => child.GetName().StartsWith("Formula:", StringComparison.Ordinal)) == 5 &&
                    objects.Any(child => child.GetName() == $"Formula: {inline}" && child.GetHelpText() == ready) &&
                    objects.Any(child => child.GetName() == $"Formula: \n{display}\n" && child.GetHelpText() == ready) &&
                    objects.Any(child => child.GetName() == $"Formula: {joined}" && child.GetHelpText() == ready) &&
                    objects.Any(child => child.GetName() == $"Formula: {unsupported}" && child.GetHelpText() == fallback) &&
                    objects.Any(child => child.GetName() == $"Formula: {invalid}" && child.GetHelpText() == syntaxError);
            }, "Chinese formula text and a Han-base joiner render while an unrelated control remains unsupported and malformed TeX fails.");
            MarkdownMathCasesTests.AssertFormulaReading(
                peer.GetChildren().Single(child => child.GetName() == $"Formula: {joined}"), joined, "ChineseMathText-HanZWJ", log);
            MarkdownMathCasesTests.AssertNativeSourceBytes(editor.TestNativeCore, Encoding.UTF8.GetBytes(markdown), "ChineseMathText-HanZWJ");
            Check(editor.GetText() == markdown, "Chinese Math rendering and its atomic UIA text range preserve the exact authored source.");
        }
        catch (TimeoutException)
        {
            foreach (var child in peer.GetChildren() ?? Array.Empty<AutomationPeer>())
                log.AppendLine($"ChineseMathText observed '{child.GetName()}': {child.GetHelpText()}");
            throw;
        }
        log.AppendLine("PASS: Markdown.MathChineseText renders the lab formula, mixed styled text and Han-ZWJ-Han with exact input, atomic UIA text and native UTF8 source; unrelated control and malformed TeX remain distinct Unsupported and Failed states.");
    }

}
