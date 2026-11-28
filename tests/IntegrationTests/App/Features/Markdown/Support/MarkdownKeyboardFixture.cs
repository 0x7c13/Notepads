// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Text;
using System.Text.Json;
using System.Threading.Tasks;
using Notepads.Features.Preferences;
using Notepads.Presentation.Controls.TextEditor;
using Notepads.Presentation.Theming;
using Windows.Storage;
using Windows.UI.ViewManagement;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Input;
using Windows.UI.Xaml.Media;
using static NotepadsWorkspaceTests.DocumentFeatureAssertions;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

// Real input comes exclusively from the separate desktop driver. This fixture
// supplies an owned Markdown file and reports the product/native observations.
internal sealed partial class MarkdownKeyboardFixture : IAsyncDisposable
{
    private readonly FeatureTestHost _host;
    private readonly UIElement _previous;
    private readonly ElementTheme _previousTheme;
    private readonly TextEditor _editor;
    private readonly WinUIEditor.EditorBaseControl _source;

    private MarkdownKeyboardFixture(FeatureTestHost host, UIElement previous, ElementTheme theme, TextEditor editor)
    {
        _host = host;
        _previous = previous;
        _previousTheme = theme;
        _editor = editor;
        _source = (WinUIEditor.EditorBaseControl)((Grid)editor.TestNativeCore.Content).Children[0];
    }

    internal static async Task<MarkdownKeyboardFixture> CreateAsync()
    {
        var previous = Window.Current.Content;
        var theme = ThemeSettingsService.ThemeMode;
        FeatureTestHost host = null;
        try
        {
            host = await FeatureTestHost.CreateAsync();
            var png = await host.Folder.CreateFileAsync("keyboard.png", CreationCollisionOption.FailIfExists);
            await FileIO.WriteBytesAsync(png, Convert.FromBase64String("iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAEklEQVR4nGP4z8DwHwyBNBgAAEnICff5q7YNAAAAAElFTkSuQmCC"));
            const string markdown = "# Markdown keyboard fixture\r\rAlpha Ω 😀 selection starts here.\r\r" +
                "![keyboard image alt](keyboard.png)\r\rSelection ends here after the image.\r\rEditable tail.";
            var editor = await OpenAsync(host, "keyboard.md", Encoding.UTF8.GetBytes(markdown));
            var fixture = new MarkdownKeyboardFixture(host, previous, theme, editor);
            AutomationProperties.SetAutomationId(host.Page, "MarkdownKeyboardPage");
            AutomationProperties.SetAutomationId(fixture._source, "MarkdownKeyboardSource");
            ApplicationView.GetForCurrentView().TryResizeView(new Windows.Foundation.Size(1100, 760));
            editor.TestNativeCore.TextWrapping = TextWrapping.Wrap;
            editor.TestNativeCore.TestFeatureNative.SetEmptySelection(editor.TestNativeCore.DocumentLength);
            TestClipboard.UseSystemClipboard = true;
            await UntilAsync(() => fixture._source.IsLoaded && fixture._source.ActualWidth > 100,
                "The external Markdown keyboard fixture presents a real native source editor.");
            return fixture;
        }
        catch
        {
            TestClipboard.UseSystemClipboard = false;
            try { if (host != null) await host.DisposeAsync(); }
            finally { Window.Current.Content = previous; }
            throw;
        }
    }

    internal void SetTheme(string value)
    {
        if (value is not ("Light" or "Dark")) throw new ArgumentException("Only declared Light/Dark screenshot fixtures are supported.");
        ThemeSettingsService.SetTheme(value == "Light" ? ElementTheme.Light : ElementTheme.Dark);
    }

    internal async Task ShowScrollSyncMenuAsync()
    {
        if (!_editor.TestMarkdownPanelOpen) _editor.ShowHideContentPreview();
        await MarkdownFeatureTests.CurrentAsync(_editor);
        _editor.GetContextFlyout().ShowAt(_source);
    }

    internal string Resize(double width, double height)
    {
        if (!double.IsFinite(width) || !double.IsFinite(height) || width < 640 || width > 2400 || height < 360 || height > 1600)
            throw new ArgumentOutOfRangeException(nameof(width), "Markdown viewport preparation requires 640–2400 by 360–1600 DIPs.");
        var accepted = ApplicationView.GetForCurrentView().TryResizeView(new Windows.Foundation.Size(width, height));
        return "markdownResizeAccepted=" + accepted + "\n" + Snapshot();
    }

    // Read existing visuals only: no UpdateLayout, invalidation, or product hook.
    internal string GeometrySnapshot()
    {
        var root = Window.Current.Content as UIElement ?? throw new InvalidOperationException("The test window has no visual root.");
        using var bytes = new MemoryStream();
        using (var json = new Utf8JsonWriter(bytes))
        {
            json.WriteStartObject();
            json.WriteNumber("schema", 1);
            json.WriteString("utc", DateTime.UtcNow.ToString("O", CultureInfo.InvariantCulture));
            json.WriteBoolean("panelOpen", _editor.TestMarkdownPanelOpen);
            json.WriteNumber("nativeRevision", _source.Editor.DocumentRevision);
            json.WriteNumber("selectionRevision", _source.Editor.SelectionRevision);
            json.WriteBoolean("sourceViewportRestorePending", _source.IsViewportRestorePending);
            var bounds = Window.Current.CoreWindow.Bounds;
            json.WriteStartObject("coreWindowBounds");
            Number(json, "x", bounds.X); Number(json, "y", bounds.Y);
            Number(json, "width", bounds.Width); Number(json, "height", bounds.Height);
            json.WriteEndObject();
            json.WriteStartArray("source");
            WriteVisualTree(json, _source, root, "source", 0, new int[2]);
            WriteAncestors(json, _source, root, "source");
            json.WriteEndArray();
            json.WriteStartArray("preview");
            var preview = _editor.TestMarkdownPreview?.PreviewView;
            if (preview != null)
            {
                WriteVisualTree(json, preview, root, "preview", 0, new int[2]);
                WriteAncestors(json, preview, root, "preview");
            }
            json.WriteEndArray();
            json.WriteEndObject();
            json.Flush();
        }
        return "markdownGeometry64=" + Convert.ToBase64String(bytes.ToArray()) + "\n";
    }

    private static void WriteVisualTree(Utf8JsonWriter json, DependencyObject current, UIElement root,
        string path, int depth, int[] budget)
    {
        if (budget[0]++ >= 256 || depth > 20 || budget[1] >= 64) return;
        if (current is FrameworkElement element && (depth == 0 || element is Border or Canvas or ScrollViewer ||
            element.Name is "ImageTarget" or "SurfaceTarget"))
        {
            WriteGeometry(json, element, root, path, depth == 0 ? "control" :
                element.Name == "ImageTarget" ? "nativeImageTarget" : element.Name == "SurfaceTarget" ? "previewSurfaceTarget" : "descendant");
            budget[1]++;
            if (element.Name is "ImageTarget" or "SurfaceTarget") WriteAncestors(json, element, root, path);
        }
        var count = VisualTreeHelper.GetChildrenCount(current);
        for (var index = 0; index < count; index++)
            WriteVisualTree(json, VisualTreeHelper.GetChild(current, index), root,
                path + "/" + index.ToString(CultureInfo.InvariantCulture), depth + 1, budget);
        if (depth == 0)
        {
            json.WriteStartObject();
            json.WriteString("role", "traversal");
            json.WriteString("path", path);
            json.WriteNumber("visited", budget[0]);
            json.WriteNumber("records", budget[1]);
            json.WriteBoolean("bounded", budget[0] >= 256 || budget[1] >= 64);
            json.WriteEndObject();
        }
    }

    private static void WriteAncestors(Utf8JsonWriter json, DependencyObject current, UIElement root, string pane)
    {
        for (var depth = 0; depth < 16 && current != null; depth++)
        {
            current = VisualTreeHelper.GetParent(current);
            if (current is FrameworkElement element)
                WriteGeometry(json, element, root, pane + "/ancestor/" + depth.ToString(CultureInfo.InvariantCulture), "ancestor");
            if (ReferenceEquals(current, root)) break;
        }
    }

    private static void WriteGeometry(Utf8JsonWriter json, FrameworkElement element, UIElement root, string path, string role)
    {
        json.WriteStartObject();
        json.WriteString("path", path);
        json.WriteString("role", role);
        json.WriteString("type", element.GetType().Name);
        json.WriteString("name", element.Name);
        json.WriteString("automationId", AutomationProperties.GetAutomationId(element));
        json.WriteBoolean("loaded", element.IsLoaded);
        json.WriteString("visibility", element.Visibility.ToString());
        Number(json, "actualWidth", element.ActualWidth); Number(json, "actualHeight", element.ActualHeight);
        Number(json, "width", element.Width); Number(json, "height", element.Height);
        json.WriteBoolean("useLayoutRounding", element.UseLayoutRounding);
        var scale = element.XamlRoot?.RasterizationScale ?? 0;
        Number(json, "rasterizationScale", scale);
        Number(json, "actualPixelWidth", element.ActualWidth * scale);
        Number(json, "actualPixelHeight", element.ActualHeight * scale);
        json.WriteString("horizontalAlignment", element.HorizontalAlignment.ToString());
        json.WriteString("verticalAlignment", element.VerticalAlignment.ToString());
        json.WriteString("flowDirection", element.FlowDirection.ToString());
        json.WriteStartArray("margin");
        foreach (var value in new[] { element.Margin.Left, element.Margin.Top, element.Margin.Right, element.Margin.Bottom }) json.WriteNumberValue(value);
        json.WriteEndArray();
        try
        {
            var transform = element.TransformToVisual(root);
            var origin = transform.TransformPoint(default);
            var x = transform.TransformPoint(new Windows.Foundation.Point(1, 0));
            var y = transform.TransformPoint(new Windows.Foundation.Point(0, 1));
            Point(json, "rootOrigin", origin.X, origin.Y);
            Point(json, "rootUnitX", x.X - origin.X, x.Y - origin.Y);
            Point(json, "rootUnitY", y.X - origin.X, y.Y - origin.Y);
            Point(json, "pixelOrigin", origin.X * scale, origin.Y * scale);
            Point(json, "pixelOriginFraction", origin.X * scale - Math.Round(origin.X * scale),
                origin.Y * scale - Math.Round(origin.Y * scale));
        }
        catch (Exception error) { json.WriteString("transformError", error.GetType().Name + ": " + error.Message); }
        WriteTransform(json, "renderTransform", element.RenderTransform);
        Point(json, "renderTransformOrigin", element.RenderTransformOrigin.X, element.RenderTransformOrigin.Y);
        var brush = element switch { Border border => border.Background, Panel panel => panel.Background, Control control => control.Background, _ => null };
        json.WriteString("backgroundType", brush?.GetType().Name ?? "null");
        if (brush is ImageBrush image)
        {
            json.WriteStartObject("imageBrush");
            json.WriteString("stretch", image.Stretch.ToString());
            json.WriteString("alignmentX", image.AlignmentX.ToString());
            json.WriteString("alignmentY", image.AlignmentY.ToString());
            Number(json, "opacity", image.Opacity);
            json.WriteString("imageSourceType", image.ImageSource?.GetType().Name ?? "null");
            WriteTransform(json, "relativeTransform", image.RelativeTransform);
            WriteTransform(json, "transform", image.Transform);
            json.WriteEndObject();
        }
        if (element is ScrollViewer scroll)
        {
            json.WriteStartObject("scroll");
            Number(json, "horizontalOffset", scroll.HorizontalOffset); Number(json, "verticalOffset", scroll.VerticalOffset);
            Number(json, "viewportWidth", scroll.ViewportWidth); Number(json, "viewportHeight", scroll.ViewportHeight);
            Number(json, "extentWidth", scroll.ExtentWidth); Number(json, "extentHeight", scroll.ExtentHeight);
            Number(json, "zoomFactor", scroll.ZoomFactor);
            json.WriteEndObject();
        }
        json.WriteEndObject();
    }

    private static void WriteTransform(Utf8JsonWriter json, string name, Transform transform)
    {
        if (transform == null) { json.WriteNull(name); return; }
        json.WriteStartObject(name);
        json.WriteString("type", transform.GetType().Name);
        var origin = transform.TransformPoint(default);
        var x = transform.TransformPoint(new Windows.Foundation.Point(1, 0));
        var y = transform.TransformPoint(new Windows.Foundation.Point(0, 1));
        Point(json, "origin", origin.X, origin.Y);
        Point(json, "unitX", x.X - origin.X, x.Y - origin.Y);
        Point(json, "unitY", y.X - origin.X, y.Y - origin.Y);
        json.WriteEndObject();
    }

    private static void Point(Utf8JsonWriter json, string name, double x, double y)
    {
        json.WriteStartObject(name); Number(json, "x", x); Number(json, "y", y); json.WriteEndObject();
    }

    private static void Number(Utf8JsonWriter json, string name, double value)
    {
        if (double.IsFinite(value)) json.WriteNumber(name, value);
        else json.WriteString(name, value.ToString(CultureInfo.InvariantCulture));
    }

    internal string Snapshot()
    {
        var native = _source.Editor;
        var extension = _editor.TestMarkdownPreview;
        var session = extension?.SourceSession;
        var preview = extension?.PreviewView;
        var selection = preview?.GetSelection();
        var peer = preview == null ? null : FrameworkElementAutomationPeer.FromElement(preview) ??
            FrameworkElementAutomationPeer.CreatePeerForElement(preview);
        var imageLoaded = peer?.GetChildren()?.Any(child => child.GetAutomationControlType() == AutomationControlType.Image &&
            child.GetName().StartsWith("keyboard image alt, image loaded", StringComparison.Ordinal)) == true;
        var focused = FocusManager.GetFocusedElement() as DependencyObject;
        return "markdownSource64=" + Encode(_editor.GetText()) +
            "\nmarkdownPanelOpen=" + _editor.TestMarkdownPanelOpen +
            "\nmarkdownSourceFocused=" + (_source.FocusState != FocusState.Unfocused) +
            "\nmarkdownPreviewFocused=" + (preview?.FocusState != FocusState.Unfocused && preview != null) +
            "\nmarkdownFocusedId=" + (focused == null ? "" : AutomationProperties.GetAutomationId(focused)) +
            "\nmarkdownSourceCurrent=" + (session?.IsCurrent == true) +
            "\nmarkdownStatus=" + session?.Status +
            "\nmarkdownFull=" + (session?.FullSubmissions ?? 0) +
            "\nmarkdownDelta=" + (session?.DeltaSubmissions ?? 0) +
            "\nmarkdownCopied=" + (session?.CopiedBytes ?? 0) +
            "\nmarkdownNativeRevision=" + native.DocumentRevision +
            "\nmarkdownFrontMatter=" + ApplicationPreferences.MarkdownFrontMatterView +
            "\nmarkdownExtensions=" + session?.Extensions +
            "\nmarkdownAcceptedExtensions=" + session?.AcceptedExtensions +
            "\nmarkdownAcceptedRevision=" + (session?.AcceptedRevision ?? 0) +
            "\nmarkdownDisplayedRevision=" + (preview?.DisplayedRevision ?? 0) +
            "\nmarkdownDisplayedDocument=" + preview?.DisplayedDocumentId +
            "\nmarkdownDocument=" + session?.DocumentId +
            "\nmarkdownSourceAnchor=" + native.Anchor + "\nmarkdownSourceCaret=" + native.CurrentPos +
            "\nmarkdownSelectionRevision=" + native.SelectionRevision +
            "\nmarkdownPreview64=" + Encode(preview?.DisplayedText ?? "") +
            "\nmarkdownSelectionAvailable=" + (selection?.IsAvailable == true) +
            "\nmarkdownSelectionStart=" + (selection?.Start ?? 0) +
            "\nmarkdownSelectionLength=" + (selection?.Length ?? 0) +
            "\nmarkdownImageLoaded=" + imageLoaded +
            "\nmarkdownTheme=" + _host.Page.ActualTheme +
            "\nmarkdownThemeGeneration=" + (preview?.ThemeGeneration ?? 0) +
            "\nmarkdownSurface=" + (preview?.IsSurfaceReady == true) +
            "\nmarkdownRenderError64=" + Encode(preview?.RenderingError ?? "") + "\n";
    }

    private static string Encode(string value) => Convert.ToBase64String(Encoding.UTF8.GetBytes(value));

    public async ValueTask DisposeAsync()
    {
        var failures = new List<Exception>();
        void Clean(Action action)
        {
            try { action(); }
            catch (Exception failure) { failures.Add(failure); }
        }

        Clean(RestoreFollowContinuity);
        Clean(RestoreSmooth);
        Clean(StopMotionTrace);
        Clean(ReleaseHtmlControls);
        Clean(RestoreYamlPreference);
        TestClipboard.UseSystemClipboard = false;
        try { await _host.DisposeAsync(); }
        catch (Exception failure) { failures.Add(failure); }
        Clean(() => ThemeSettingsService.SetTheme(_previousTheme));
        Clean(() => Window.Current.Content = _previous);
        if (failures.Count != 0) throw new AggregateException("Markdown keyboard fixture cleanup failed.", failures);
    }
}
