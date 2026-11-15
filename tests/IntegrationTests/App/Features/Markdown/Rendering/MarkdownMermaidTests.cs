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
using Windows.UI.ViewManagement;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Automation.Provider;
using Windows.UI.Xaml.Automation.Text;
using static NotepadsWorkspaceTests.DocumentFeatureAssertions;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal static class MarkdownMermaidTests
{
    internal const string SingleSource = "# Tiny flowchart\r\r```mermaid\rflowchart TD\rA-->B\r```\r";

    // Fixed authored CASE 07-A input; no user path or runtime file dependency.
    internal const string DiagramSource = "# Mermaid families\r\r```mermaid\r" +
        "flowchart LR\r" +
        "    A[\"打开 Markdown\"] --> B{\"预览已就绪？\"}\r" +
        "    B -->|是| C[\"显示已恢复的视口\"]\r" +
        "    B -->|否| D[\"保留加载状态\"]\r" +
        "    D --> B\r" +
        "    subgraph Pair[\"双栏联动\"]\r" +
        "        C --> E[\"源码\"]\r" +
        "        E <--> F[\"预览\"]\r" +
        "    end\r```\r\r```mermaid\r" +
        "sequenceDiagram\r" +
        "    participant E as Editor\r" +
        "    participant P as Preview\r" +
        "    E->>P: Publish revision 1\r" +
        "    P-->>E: Frame ready\r" +
        "    Note over E,P: Keep the viewport anchor\r```\r\r```mermaid\r" +
        "stateDiagram-v2\r" +
        "    [*] --> Loading\r" +
        "    Loading --> Ready\r" +
        "    Ready --> [*]\r```\r\r```mermaid\r" +
        "pie showData\r" +
        "    title 预览资源\r" +
        "    \"文本\" : 62\r" +
        "    \"图片\" : 25\r" +
        "    \"公式\" : 13\r```\r\r```mermaid\r" +
        "flowchart LR\r" +
        "    A[\"unclosed node\r```\r";

    internal static string MixedSource => "# Shared Math and Mermaid admission\r\r" +
        string.Join(" ", Enumerable.Range(0, 70).Select(index => "$x_{" + index + "}+1$")) +
        "\r\r" + DiagramSource;

    internal static string OverflowSource => "# Bounded Math and Mermaid admission\r\r" +
        string.Join(" ", Enumerable.Range(0, 140).Select(index => "$x_{" + index + "}+1$")) +
        "\r\r" + DiagramSource;

    private const string RelationshipSource = "# Class and entity relationship diagrams\r\r```mermaid\r" +
        "classDiagram\r" +
        "    direction LR\r" +
        "    class Document[\"Markdown 文档\"] {\r" +
        "        +String source\r" +
        "        +save()\r" +
        "    }\r" +
        "    class Preview {\r" +
        "        +render()\r" +
        "    }\r" +
        "    Document \"1\" *-- \"1\" Preview : owns\r" +
        "    Preview ..|> Renderer : implements\r" +
        "    Renderer <|-- NativeRenderer\r" +
        "```\r\r```mermaid\r" +
        "erDiagram\r" +
        "    direction LR\r" +
        "    DOCUMENT {\r" +
        "        string id PK\r" +
        "        string title\r" +
        "    }\r" +
        "    SECTION {\r" +
        "        string documentId FK\r" +
        "        int ordinal\r" +
        "    }\r" +
        "    DOCUMENT ||--|{ SECTION : contains\r" +
        "    SECTION }o..o| PARENT : references\r" +
        "```\r";

    private static string DiagramBoundarySource => "# Diagram resident boundary\r\r" +
        string.Join("\r", Enumerable.Range(0, 65).Select(index =>
            "```mermaid\rflowchart TD\rA" + index + "-->B" + index + "\r```\r"));

    internal sealed class DerivedObject
    {
        internal string Family;
        internal string Name;
        internal string Help;
        internal string State;
        internal Rect Bounds;
    }

    internal static async Task RunAsync(StringBuilder log)
    {
        await RunCaseAsync("single", SingleSource, 0, 1, log);
        // This is intentionally first after the sanity case: the original 64
        // resident run admitted 59 Math objects and all five diagram requests.
        // Keep that original failure evidence; the current 128-resident policy
        // admits the complete authored mixed input.
        await RunCaseAsync("mixed", MixedSource, 70, 5, log);
        await RunCaseAsync("overflow", OverflowSource, 140, 5, log);
        await RunCaseAsync("diagrams", DiagramSource, 0, 5, log, relayout: true);
        await RunCaseAsync("relationships", RelationshipSource, 0, 2, log);
        await MetadataAsync(log);
        await InspectionAsync(log);
        await RunCaseAsync("diagram-boundary-65", DiagramBoundarySource, 0, 65, log);
    }

    private static async Task MetadataAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        const string source = "# Accessible diagrams\r\r```mermaid\rclassDiagram\r" +
            "accTitle: 编辑器类图\raccDescr {\r第一行；描述\rSecond line.\r}\r" +
            "Editor --> Preview\r```\r\r```mermaid\rflowchart TD\r" +
            "accTitle: 带语法错误的图\raccDescr: The description is independent of rendering.\r" +
            "A[\"unclosed\r```\r";
        var editor = await OpenAsync(host, "mermaid-accessibility.md", Encoding.UTF8.GetBytes(source));
        editor.ShowHideContentPreview();
        var view = (await MarkdownFeatureTests.CurrentAsync(editor)).PreviewView;
        await UntilAsync(() =>
        {
            var diagrams = ReadObjects(view).Where(item => item.Family == "Diagram").ToArray();
            return diagrams.Length == 2 &&
                diagrams[0].Name == "Diagram: 编辑器类图" && diagrams[0].State == "Ready" &&
                diagrams[0].Help.EndsWith("\n第一行；描述\nSecond line.", StringComparison.Ordinal) &&
                diagrams[1].Name == "Diagram: 带语法错误的图" && diagrams[1].State == "Failed" &&
                diagrams[1].Help.EndsWith("\nThe description is independent of rendering.", StringComparison.Ordinal);
        }, "Mermaid metadata is available through native UIA for a rendered and a failed diagram.", 25000);
        Check(editor.GetText() == source &&
            view.DisplayedText.Count(character => character == '\uFFFC') == 2 &&
            !view.DisplayedText.Contains("accTitle", StringComparison.Ordinal),
            "Authored Mermaid metadata leaves editor text and the two preview text objects intact.");
        log.AppendLine("PASS: Markdown.Mermaid accessibility preserves native names, descriptions and source on Ready and Failed diagrams.");
    }

    private static async Task InspectionAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        const string source = "# Inspect Mermaid\r\r```mermaid\rflowchart TB\r" +
            "accTitle: Wide diagram\r" +
            "A[Read the original Markdown document and its metadata]\r" +
            "A --> B[Lay out headings and body text]\r" +
            "A --> C[Look up local images and formulas]\r" +
            "A --> D[Verify all accessible names and descriptions]\r" +
            "B --> E[Publish preview]; C --> E; D --> E\r```\r\rAfter the diagram.";
        var editor = await OpenAsync(host, "mermaid-inspection.md", Encoding.UTF8.GetBytes(source));
        editor.ShowHideContentPreview();
        var view = (await MarkdownFeatureTests.CurrentAsync(editor)).PreviewView;
        var previewPeer = FrameworkElementAutomationPeer.FromElement(view) ??
            FrameworkElementAutomationPeer.CreatePeerForElement(view);
        AutomationPeer diagram = null;
        await UntilAsync(() =>
        {
            diagram = previewPeer.GetChildren()?.SingleOrDefault(child => child.GetName() == "Diagram: Wide diagram");
            return diagram?.GetPattern(PatternInterface.Invoke) is IInvokeProvider;
        }, "A ready native diagram exposes its inspection action.", 25000);

        var invoke = (IInvokeProvider)diagram.GetPattern(PatternInterface.Invoke);
        var normal = diagram.GetBoundingRectangle();
        var initialSelection = view.GetSelection();
        invoke.Invoke();
        log.AppendLine($"OBS: Mermaid inspection first Invoke focus={view.FocusState}/{diagram.HasKeyboardFocus()}, bounds={diagram.GetBoundingRectangle()}, Scroll={diagram.GetPattern(PatternInterface.Scroll) != null}, Transform2={diagram.GetPattern(PatternInterface.Transform2) != null}.");
        await UntilAsync(() => diagram.GetPattern(PatternInterface.Transform2) is ITransformProvider2 &&
            diagram.GetPattern(PatternInterface.Scroll) is IScrollProvider,
            "The real UIA peer publishes both inspection patterns.");
        var transform = (ITransformProvider2)diagram.GetPattern(PatternInterface.Transform2);
        var scroll = (IScrollProvider)diagram.GetPattern(PatternInterface.Scroll);
        Check(transform.CanZoom && transform.ZoomLevel == 100 && transform.MinZoom == 100 && transform.MaxZoom == 800 &&
            !scroll.HorizontallyScrollable && !scroll.VerticallyScrollable && diagram.HasKeyboardFocus(),
            "Inspection opens at fit and gives the diagram the keyboard focus.");

        transform.Zoom(800);
        Check(transform.ZoomLevel == 800 && scroll.HorizontallyScrollable && scroll.VerticallyScrollable &&
            scroll.HorizontalViewSize < 100 && scroll.VerticalViewSize < 100,
            "Zoom uses both axes of the same native diagram.");
        scroll.SetScrollPercent(0, 100);
        Check(scroll.HorizontalScrollPercent == 0 && scroll.VerticalScrollPercent == 100,
            "The UIA scroll pattern reaches both diagram edges.");
        scroll.Scroll(ScrollAmount.SmallIncrement, ScrollAmount.SmallDecrement);
        Check(scroll.HorizontalScrollPercent > 0 && scroll.VerticalScrollPercent < 100,
            "The UIA scroll pattern pans both axes within the clipped viewport.");

        var zoomed = diagram.GetBoundingRectangle();
        var text = (ITextProvider)previewPeer.GetPattern(PatternInterface.Text);
        var objectRange = ((ITextChildProvider)diagram.GetPattern(PatternInterface.TextChild)).TextRange;
        var visible = text.GetVisibleRanges();
        var point = text.RangeFromPoint(new Point(zoomed.X + zoomed.Width / 4, zoomed.Y + zoomed.Height / 2));
        objectRange.GetBoundingRectangles(out var objectBounds);
        Check(zoomed.Height > normal.Height && visible.Length == 1 &&
            visible[0].Compare(objectRange) && point.CompareEndpoints(TextPatternRangeEndpoint.Start, objectRange,
                TextPatternRangeEndpoint.Start) == 0 && objectBounds?.Length == 4,
            "Inspection UIA geometry exposes the painted diagram as one atomic object.");
        transform.Zoom(100);
        Check(transform.ZoomLevel == 100 && !scroll.HorizontallyScrollable && !scroll.VerticallyScrollable,
            "Fit restores the original scroll ranges.");
        invoke.Invoke();
        var restored = diagram.GetBoundingRectangle();
        Check(diagram.GetPattern(PatternInterface.Scroll) == null &&
            diagram.GetPattern(PatternInterface.Transform2) == null &&
            Math.Abs(restored.Height - normal.Height) < 1.0 && diagram.HasKeyboardFocus(),
            "Closing the inspector restores document geometry and keeps the diagram focused.");
        var selection = view.GetSelection();
        Check(editor.GetText() == source && selection.Start == initialSelection.Start &&
            selection.Length == initialSelection.Length,
            "Inspection preserves source bytes and preview selection.");
        log.AppendLine("PASS: Markdown.Mermaid inspection verifies native UIA Invoke, pan, zoom, text geometry and source.");
    }

    private static async Task RunCaseAsync(string name, string source, int formulas, int diagrams,
        StringBuilder log, bool relayout = false)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var editor = await OpenAsync(host, "mermaid-" + name + ".md", Encoding.UTF8.GetBytes(source));
        var native = editor.TestNativeCore.TestFeatureNative;
        native.SetEmptySelection(0);
        var original = editor.GetText();
        var sequence = native.DocumentSequence;
        var undo = native.UndoSequence;
        var canUndo = native.CanUndo();
        var canRedo = native.CanRedo();
        var modified = native.Modify;
        var editorModified = editor.IsModified;
        var window = Window.Current.CoreWindow.Bounds;
        var theme = ThemeSettingsService.ThemeMode;
        var sourceSelection = (native.Anchor, native.CurrentPos, native.SelectionRevision);
        void AssertSourceSelection(string transition)
        {
            var actual = (native.Anchor, native.CurrentPos, native.SelectionRevision);
            log.AppendLine($"OBS: Mermaid {transition} source selection={actual.Anchor}:{actual.CurrentPos}@{actual.SelectionRevision}, expected={sourceSelection.Anchor}:{sourceSelection.CurrentPos}@{sourceSelection.SelectionRevision}.");
            Check(sourceSelection == actual,
                "Mermaid " + transition + " preserves the nonempty source selection, direction and revision.");
        }
        try
        {
            editor.ShowHideContentPreview();
            var extension = await MarkdownFeatureTests.CurrentAsync(editor);
            extension.FollowSource = false;
            var preview = extension.PreviewView;
            preview.TrySetVerticalScrollProgress(0);
            // Establish a source selection after preview initialization, which may
            // intentionally focus or map the initial caret. Every authored title
            // begins with enough ASCII text for these nonempty UTF8 positions.
            editor.Focus();
            native.SetSel(2, 9);
            sourceSelection = (native.Anchor, native.CurrentPos, native.SelectionRevision);
            Check(native.Anchor == 2 && native.CurrentPos == 9 && preview.FocusState == FocusState.Unfocused,
                "The Mermaid fixture establishes a focused, nonempty source selection away from zero.");
            await AssertDiagramsAsync(preview, formulas, diagrams, name, log);
            AssertSourceSelection(name + "-render");
            if (relayout)
            {
                var generation = preview.ThemeGeneration;
                ThemeSettingsService.SetTheme(theme == ElementTheme.Light ? ElementTheme.Dark : ElementTheme.Light);
                await UntilAsync(() => preview.ThemeGeneration != generation,
                    "Mermaid theme change reaches the real native preview.", 25000);
                await MarkdownFeatureTests.CurrentAsync(editor);
                await AssertDiagramsAsync(preview, formulas, diagrams, name + "-theme", log);
                AssertSourceSelection(name + "-theme");
                Check(ApplicationView.GetForCurrentView().TryResizeView(new Size(900, 760)),
                    "The Mermaid fixture admits its narrower window.");
                await UntilAsync(() => Math.Abs(Window.Current.CoreWindow.Bounds.Width - 900) < 0.6,
                    "The Mermaid window settles at its narrower width.");
                await MarkdownFeatureTests.CurrentAsync(editor);
                await AssertDiagramsAsync(preview, formulas, diagrams, name + "-width", log);
                AssertSourceSelection(name + "-width");
                editor.ShowHideContentPreview();
                await extension.DisposalCompletion.WaitAsync(TimeSpan.FromSeconds(20));
                editor.ShowHideContentPreview();
                extension = await MarkdownFeatureTests.CurrentAsync(editor);
                extension.FollowSource = false;
                await AssertDiagramsAsync(extension.PreviewView, formulas, diagrams, name + "-reopen", log);
                AssertSourceSelection(name + "-reopen");
            }
            Check(editor.GetText() == original && native.DocumentSequence == sequence &&
                native.UndoSequence == undo && native.CanUndo() == canUndo && native.CanRedo() == canRedo &&
                native.Modify == modified && editor.IsModified == editorModified &&
                sourceSelection == (native.Anchor, native.CurrentPos, native.SelectionRevision),
                "Mermaid rendering, diagnostic states and relayout preserve exact source, source selection, document sequence, Undo/Redo and save point.");
            log.AppendLine("PASS: Markdown.Mermaid " + name + " preserves all source-backed objects and expected diagram outcomes.");
        }
        finally
        {
            try { ThemeSettingsService.SetTheme(theme); }
            finally { ApplicationView.GetForCurrentView().TryResizeView(new Size(window.Width, window.Height)); }
        }
    }

    private static async Task AssertDiagramsAsync(MarkdownView preview, int formulas, int diagrams,
        string label, StringBuilder log)
    {
        DerivedObject[] objects = Array.Empty<DerivedObject>();
        try
        {
            await UntilAsync(() =>
            {
                objects = ReadObjects(preview);
                return objects.Count(item => item.Family == "Formula") == formulas &&
                    objects.Count(item => item.Family == "Diagram") == diagrams &&
                    objects.All(item => item.State != "Pending" && item.State != "Unavailable");
            }, "All authored Mermaid/Math objects publish a terminal UIA state: " + label, 25000);
            if (formulas != 0)
            {
                var actualFormulas = objects.Where(item => item.Family == "Formula").ToArray();
                var ready = formulas == 70 ? 70 : 123;
                var limited = formulas == 70 ? 0 : 17;
                Check((formulas == 70 || formulas == 140) && diagrams == 5 &&
                    actualFormulas.Count(item => item.State == "Ready") == ready &&
                    actualFormulas.Count(item => item.State == "Limited") == limited &&
                    actualFormulas.Select(item => item.Name).SequenceEqual(
                        Enumerable.Range(0, formulas).Select(index => "Formula: x_{" + index + "}+1")) &&
                    actualFormulas.Take(ready).All(item => item.State == "Ready") &&
                    actualFormulas.Skip(ready).All(item => item.State == "Limited"),
                    $"The shared 128-resident fixture admits exactly {ready} valid formulas and limits {limited}, retaining every exact source-backed object and five diagram requests: " + label);
            }
            var actual = objects.Where(item => item.Family == "Diagram").ToArray();
            Check(actual.Take(diagrams == 5 ? 4 : diagrams).All(item => item.State == "Ready") &&
                (diagrams != 5 || actual[4].State == "Failed"),
                diagrams == 65 ? "All 65 authored tiny flowcharts render: " + label :
                    "The valid flowchart/sequence/state/pie diagrams render and the malformed flowchart fails: " + label);
            if (label == "relationships")
                Check(actual[0].Name.StartsWith("Diagram: classDiagram\n", StringComparison.Ordinal) &&
                    actual[1].Name.StartsWith("Diagram: erDiagram\n", StringComparison.Ordinal) &&
                    actual.All(item => item.Bounds.Width > 0 && item.Bounds.Height > 0),
                    "Class and ER fences each publish one usable native diagram object.");
            if (diagrams == 65)
                Check(actual.Select((item, index) => item.Name.Contains("A" + index + "-->B" + index, StringComparison.Ordinal)).All(matches => matches) &&
                    actual[64].State == "Ready",
                    "All 65 uniquely authored tiny flowcharts render, including the actual public UIA object at ordinal 64: " + label);
        }
        finally
        {
            log.AppendLine($"OBS: Mermaid {label} formulas={objects.Count(item => item.Family == "Formula")}, diagrams={objects.Count(item => item.Family == "Diagram")}; " +
                string.Join(", ", objects.GroupBy(item => item.Family + "." + item.State).Select(group => group.Key + "=" + group.Count())));
            foreach (var item in objects)
                log.AppendLine("OBS: Mermaid " + label + " '" + Escape(item.Name) + "': " + item.Help +
                    $" bounds={item.Bounds.X},{item.Bounds.Y},{item.Bounds.Width},{item.Bounds.Height}.");
        }
    }

    internal static DerivedObject[] ReadObjects(MarkdownView preview)
    {
        if (preview == null) return Array.Empty<DerivedObject>();
        var peer = FrameworkElementAutomationPeer.FromElement(preview) ?? FrameworkElementAutomationPeer.CreatePeerForElement(preview);
        return (peer?.GetChildren() ?? Array.Empty<AutomationPeer>())
            .Where(child => child.GetName().StartsWith("Formula:", StringComparison.Ordinal) ||
                child.GetName().StartsWith("Diagram:", StringComparison.Ordinal))
            .Select(child => new DerivedObject
            {
                Family = child.GetName().StartsWith("Formula:", StringComparison.Ordinal) ? "Formula" : "Diagram",
                Name = child.GetName(), Help = child.GetHelpText(), State = State(child.GetHelpText()),
                Bounds = child.GetBoundingRectangle()
            }).ToArray();
    }

    private static string State(string help)
    {
        if (help.Contains(" rendered.", StringComparison.Ordinal)) return "Ready";
        if (help.Contains(" loading.", StringComparison.Ordinal)) return "Pending";
        if (help.Contains("resource or placement limit", StringComparison.Ordinal)) return "Limited";
        if (help.Contains("unsupported", StringComparison.Ordinal) || help.Contains("not supported", StringComparison.Ordinal)) return "Unsupported";
        if (help.Contains("syntax error", StringComparison.Ordinal) || help.Contains("rendering failed", StringComparison.Ordinal)) return "Failed";
        if (help.Contains("font", StringComparison.Ordinal)) return "FontUnavailable";
        if (help.Contains("cancelled", StringComparison.Ordinal)) return "Cancelled";
        return "Unavailable";
    }

    private static string Escape(string value) => value.Replace("\r", "\\r", StringComparison.Ordinal).Replace("\n", "\\n", StringComparison.Ordinal);
}
