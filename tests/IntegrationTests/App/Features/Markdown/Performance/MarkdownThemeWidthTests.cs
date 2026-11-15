// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Linq;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Text;
using System.Threading.Tasks;
using Notepads.Markdown;
using Notepads.Presentation.Controls.Markdown;
using Notepads.Presentation.Controls.TextEditor;
using Notepads.Presentation.Theming;
using Windows.Foundation;
using Windows.Storage.Streams;
using Windows.UI;
using Windows.UI.ViewManagement;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Automation.Provider;
using Windows.UI.Xaml.Automation.Text;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Media;
using static NotepadsWorkspaceTests.DocumentFeatureAssertions;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests
{
    // Fixed authored input, actual App host and public native reading APIs. Each
    // scenario owns a fresh document so one failed transition cannot hide another.
    internal static partial class MarkdownThemeWidthTests
    {
        private const string Png = "iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAEklEQVR4nGP4z8DwHwyBNBgAAEnICff5q7YNAAAAAElFTkSuQmCC";
        private const string Middle = "W1 middle target 58294.";
        private const string WrappedWord = "w1word075";
        private const MarkdownExtensions Extensions = MarkdownExtensions.Math | MarkdownExtensions.Mermaid;

        private static string Source => "# W1 reading position 汉字 Ω 😀\r\r" +
            "![W1 loaded image](data:image/png;base64," + Png + ")\r\r" +
            "$x^2$\r\r$$\\frac{1}{2}$$\r\r" +
            "```mermaid\rflowchart TD\rA-->B\r```\r\r" +
            string.Join("\r\r", Enumerable.Range(0, 12).Select(index => "W1 leading paragraph " + index.ToString("D3") + ".")) +
            "\r\r```mermaid\rstateDiagram-v2\r[*] --> S0\r" +
            string.Join("\r", Enumerable.Range(0, 11).Select(index => "S" + index + " --> S" + (index + 1))) +
            "\rS11 --> [*]\r```\r\r" +
            string.Join("\r\r", Enumerable.Range(0, 12).Select(index => "W1 bridge paragraph " + index.ToString("D3") + ".")) +
            "\r\r" + Middle + "\r\rW1 independent following probe.\r\r" +
            "W1 long paragraph " + string.Join(" ", Enumerable.Range(0, 190).Select(index => "w1word" + index.ToString("D3"))) +
            "\r\r" + string.Join("\r\r", Enumerable.Range(0, 45).Select(index => "W1 trailing paragraph " + index.ToString("D3") + "."));

        internal static async Task RunAsync(StringBuilder log)
        {
            var failures = new List<string>();
            foreach (var target in new[] { "middle", "wrapped", "atomic" })
            {
                await CaseAsync("host-" + target + "-theme", () => HostAsync(target, "theme", log));
                await CaseAsync("host-" + target + "-narrow", () => HostAsync(target, "narrow", log));
                if (target != "atomic")
                    await CaseAsync("host-" + target + "-wide", () => HostAsync(target, "wide", log));
            }
            await CaseAsync("host-newer-intent-theme", () => HostAsync("middle", "theme-intent", log));
            await CaseAsync("host-newer-intent-width", () => HostAsync("middle", "narrow-intent", log));
            await CaseAsync("host-rapid-latest-theme", () => HostAsync("middle", "rapid", log));
            await CaseAsync("native-theme", () => NativeAsync("theme", log));
            await CaseAsync("native-width", () => NativeAsync("narrow", log));
            await CaseAsync("native-cancel-replace", () => NativeAsync("cancel-replace", log));
            foreach (var api in new[] { "viewport", "follow", "restore", "stale-restore" })
                await CaseAsync("native-same-row-" + api, () => SameRowFollowAsync(api, log));
            if (failures.Count != 0) throw new InvalidOperationException(string.Join("\n", failures));

            async Task CaseAsync(string name, Func<Task> action)
            {
                try { await action(); log.AppendLine("PASS: Markdown.ThemeWidth " + name); }
                catch (Exception error)
                {
                    failures.Add(name + ": " + error);
                    log.AppendLine("FAIL: Markdown.ThemeWidth " + name + ": " + error);
                }
            }
        }

        private static async Task HostAsync(string target, string transition, StringBuilder log)
        {
            var bounds = Window.Current.CoreWindow.Bounds;
            var theme = ThemeSettingsService.ThemeMode;
            await using var host = await FeatureTestHost.CreateAsync();
            TextEditor editor = null;
            MarkdownExtensionView extension = null;
            string stamp = null;
            var label = "host-" + target + "-" + transition;
            try
            {
                ThemeSettingsService.SetTheme(ElementTheme.Dark);
                await ResizeAsync(transition == "wide" ? 640 : 1100);
                editor = await OpenAsync(host, label + ".md", Encoding.UTF8.GetBytes(Source));
                editor.ShowHideContentPreview();
                extension = await MarkdownFeatureTests.CurrentAsync(editor);
                extension.FollowSource = false;
                var view = extension.PreviewView;
                view.IsSmoothScrollingEnabled = false;
                await SettledAsync(view, extension, label + "-initial", log);
                editor.Focus();
                editor.TestNativeCore.TestFeatureNative.SetSel(2, 9);
                stamp = Stamp(editor);
                var previewSelection = Selection(view);
                await PositionAsync(view, extension, target, label, log);
                var before = Observe(view, target);
                var oldTheme = view.ThemeGeneration;
                log.AppendLine("OBS: ThemeWidth " + label + " before " + before);

                // The intentional scroll is in the same UI turn as invalidation.
                // No await, frame callback, mapping or corrective setter intervenes.
                if (transition.StartsWith("theme", StringComparison.Ordinal) || transition == "rapid")
                {
                    ThemeSettingsService.SetTheme(ElementTheme.Light);
                    if (transition == "rapid")
                    {
                        ThemeSettingsService.SetTheme(ElementTheme.Dark);
                        ThemeSettingsService.SetTheme(ElementTheme.Light);
                    }
                }
                else
                    Check(ApplicationView.GetForCurrentView().TryResizeView(new Size(transition == "wide" ? 1100 : 640, 760)),
                        "The owned window admits the requested width transition.");
                var newerIntent = transition.EndsWith("-intent", StringComparison.Ordinal);
                if (newerIntent) Check(view.TrySetVerticalScrollProgress(0), "A newer explicit public viewport request is admitted before yielding.");

                await UntilAsync(() => view.GetViewportAnchor().IsAvailable && view.GetViewportAnchor().LayoutGeneration > before.Layout &&
                    (transition.StartsWith("theme", StringComparison.Ordinal) || transition == "rapid"
                        ? view.ThemeGeneration > oldTheme
                        : Math.Abs(Window.Current.CoreWindow.Bounds.Width - (transition == "wide" ? 1100 : 640)) < 0.6),
                    "The transition must publish a newer actual reading layout.", 25000);
                await SettledAsync(view, extension, label + "-after", log);
                var after = Observe(view, target);
                log.AppendLine("OBS: ThemeWidth " + label + " after " + after);
                Check(after.Layout > before.Layout && after.Document == before.Document && after.Revision == before.Revision && after.Text == before.Text,
                    "The newer presented layout retains the exact source/text identity.");
                if (newerIntent)
                    Check(after.Y <= PixelTolerance(view) && after.Offset == 0 && view.VerticalScrollProgress == 0,
                        "The newer explicit top request wins over the obsolete reading-position restoration.");
                else
                    AssertPreserved(view, target, before, after);
                Check(!extension.FollowSource && Stamp(editor) == stamp && Selection(view) == previewSelection,
                    "Theme/width changes and read-only observations preserve FollowSource=false, exact source, source selection/revision, Undo/Redo, save point and preview selection.");
            }
            finally
            {
                var unchanged = editor == null || stamp == null || Stamp(editor) == stamp;
                log.AppendLine("OBS: ThemeWidth " + label + " source/selection/Undo/revision unchanged=" + unchanged + ".");
                try
                {
                    if (editor?.TestMarkdownPanelOpen == true) editor.ShowHideContentPreview();
                    if (extension != null) await extension.DisposalCompletion.WaitAsync(TimeSpan.FromSeconds(20));
                }
                finally
                {
                    ThemeSettingsService.SetTheme(theme);
                    ApplicationView.GetForCurrentView().TryResizeView(new Size(bounds.Width, bounds.Height));
                }
                Check(unchanged, "Every regression exit preserves the actual source document and selection.");
            }
        }

        private static async Task NativeAsync(string transition, StringBuilder log)
        {
            var previous = Window.Current.Content;
            var root = new Grid();
            var view = new MarkdownView
            {
                Width = 760, Height = 380, IsSmoothScrollingEnabled = false,
                HorizontalAlignment = HorizontalAlignment.Left, VerticalAlignment = VerticalAlignment.Top
            };
            root.Children.Add(view);
            try
            {
                Window.Current.Content = root;
                Window.Current.Activate();
                await UntilAsync(() => view.IsLoaded && view.ActualWidth == 760, "The bare native consumer is arranged.");
                var scroll = Scroll(view);
                bool highContrast = new AccessibilitySettings().HighContrast;
                var rail = highContrast ? new UISettings().UIElementColor(UIElementType.Window) : Colors.DarkSlateGray;
                view.ApplyTheme(Colors.White, Colors.DarkSlateGray, "Segoe UI", 16, 24, 16, view.ThemeGeneration + 1);
                Check(scroll.Background is SolidColorBrush brush && brush.Color.Equals(rail),
                    "Applying a new theme after template attachment updates the scrollbar backdrop.");
                view.SetImageProvider(new Images(), 1);
                SetNativeTheme(view, dark: true);
                var document = Guid.NewGuid();
                var receipt = await view.SetDocumentWithExtensionsAsync(document, 1, Encoding.UTF8.GetBytes(Source), Extensions)
                    .AsTask().WaitAsync(TimeSpan.FromSeconds(25));
                Check(receipt.Status == DocumentUpdateStatus.Ready && receipt.Published, "The bare native multiresource source publishes.");
                await SettledAsync(view, null, "native-initial", log);
                await PositionAsync(view, null, "middle", "native-" + transition, log);
                var before = Observe(view, "middle");
                var selection = Selection(view);
                log.AppendLine("OBS: ThemeWidth native-" + transition + " before " + before);
                if (transition != "narrow") SetNativeTheme(view, dark: false);
                if (transition == "cancel-replace")
                {
                    // Cancel the in-flight old geometry, then replace its source in
                    // this same turn. Once published, the caller explicitly requests
                    // the new source's origin before allowing resource work to settle.
                    view.Cancel();
                    var replacementDocument = Guid.NewGuid();
                    var replacement = Source.Replace(Middle, "W1 current replacement target.", StringComparison.Ordinal);
                    receipt = await view.SetDocumentWithExtensionsAsync(replacementDocument, 2, Encoding.UTF8.GetBytes(replacement), Extensions)
                        .AsTask().WaitAsync(TimeSpan.FromSeconds(25));
                    Check(receipt.Status == DocumentUpdateStatus.Ready && receipt.Published, "The replacement after cancellation publishes.");
                    var replacementSelection = Selection(view);
                    Check(view.TrySetVerticalScrollProgress(0), "The current replacement admits an explicit top request before yielding again.");
                    await SettledAsync(view, null, "native-cancel-replace", log);
                    var current = view.GetViewportAnchor();
                    log.AppendLine($"OBS: ThemeWidth native-cancel-replace current={current.DocumentId}/{current.Revision}/{current.TextGeneration}/{current.LayoutGeneration}, offset={current.PreviewUtf16Offset}:{current.OffsetWithinLine:R}.");
                    Check(current.IsAvailable && current.DocumentId == replacementDocument && current.Revision == 2 &&
                        view.DisplayedRevision == 2 && view.DisplayedText.Contains("W1 current replacement target.", StringComparison.Ordinal) &&
                        !view.DisplayedText.Contains(Middle, StringComparison.Ordinal) && current.PreviewUtf16Offset == 0 &&
                        Scroll(view).VerticalOffset <= PixelTolerance(view) && view.VerticalScrollProgress == 0 && Selection(view) == replacementSelection,
                        "Cancelled old theme/resource restoration cannot override the newest source's explicit viewport intent or selection.");
                    return;
                }
                if (transition == "narrow") view.Width = 420;
                await UntilAsync(() => view.GetViewportAnchor().IsAvailable && view.GetViewportAnchor().LayoutGeneration > before.Layout &&
                    (transition != "narrow" || view.ActualWidth == 420), "The bare consumer publishes its newer layout.", 25000);
                await SettledAsync(view, null, "native-" + transition + "-after", log);
                var after = Observe(view, "middle");
                log.AppendLine("OBS: ThemeWidth native-" + transition + " after " + after);
                AssertPreserved(view, "middle", before, after);
                Check(Selection(view) == selection && view.GetSourceState().Revision == 1,
                    "The bare native consumer preserves source revision and selection through theme/width relayout.");
            }
            finally
            {
                view.Cancel();
                root.Children.Clear();
                Window.Current.Content = previous;
            }
        }

        private static async Task SameRowFollowAsync(string api, StringBuilder log)
        {
            var previous = Window.Current.Content;
            var root = new Grid();
            var view = new MarkdownView
            {
                Width = 760, Height = 380, IsSmoothScrollingEnabled = false,
                HorizontalAlignment = HorizontalAlignment.Left, VerticalAlignment = VerticalAlignment.Top
            };
            root.Children.Add(view);
            var label = "native-same-row-" + api;
            try
            {
                Window.Current.Content = root;
                Window.Current.Activate();
                await UntilAsync(() => view.IsLoaded && view.ActualWidth == 760, "The same-row native fixture is arranged.");
                view.SetImageProvider(new Images(), 1);
                SetNativeTheme(view, dark: true);
                var document = Guid.NewGuid();
                var receipt = await view.SetDocumentWithExtensionsAsync(document, 1, Encoding.UTF8.GetBytes(Source), Extensions)
                    .AsTask().WaitAsync(TimeSpan.FromSeconds(25));
                Check(receipt.Status == DocumentUpdateStatus.Ready && receipt.Published, "The same-row multiresource source publishes.");
                await SettledAsync(view, null, label + "-initial", log);
                await PositionAsync(view, null, "atomic", label, log);
                var original = Observe(view, "atomic");
                var selection = Selection(view);
                var text = view.DisplayedText;
                log.AppendLine("OBS: ThemeWidth " + label + " original atomic " + original);

                view.IsResourceRenderingDeferred = true;
                SetNativeTheme(view, dark: false);
                await ProvisionalAsync(view, original, label + "-provisional", log);
                var provisional = Observe(view, "wrapped");
                var anchor = view.GetViewportAnchor();
                Check(provisional.Offset > original.Offset && provisional.Within < original.Within,
                    "The retained tall-object displacement reaches a later real body line in the shorter Pending geometry.");
                var scroll = Scroll(view);
                var scale = view.XamlRoot.RasterizationScale;
                var beforeRow = Math.Floor(scroll.VerticalOffset * scale + 0.5);
                var rejected = api == "stale-restore";
                var admitted = api switch
                {
                    "viewport" => view.SetViewportBetweenPositions(anchor.DocumentId, anchor.Revision, anchor.TextGeneration,
                        anchor.LayoutGeneration, anchor.ScrollStartUtf16Offset, anchor.ScrollEndUtf16Offset, anchor.ScrollProgress),
                    "follow" => view.SetFollowViewportBetweenPositions(anchor.DocumentId, anchor.Revision, anchor.TextGeneration,
                        anchor.LayoutGeneration, anchor.ScrollStartUtf16Offset, anchor.ScrollEndUtf16Offset, anchor.ScrollProgress, 0, 0),
                    _ => view.TryRestoreFollowViewport(anchor.DocumentId, anchor.Revision, anchor.TextGeneration,
                        anchor.LayoutGeneration + (rejected ? 1UL : 0UL), anchor.ScrollStartUtf16Offset, anchor.ScrollEndUtf16Offset, anchor.ScrollProgress, 0, 0)
                };
                var afterRow = Math.Floor(scroll.VerticalOffset * scale + 0.5);
                log.AppendLine($"OBS: ThemeWidth {label} admitted={admitted}, expected={!rejected}, pixelRow={beforeRow:R}->{afterRow:R}, " +
                    $"span={anchor.ScrollStartUtf16Offset}:{anchor.ScrollEndUtf16Offset}:{anchor.ScrollProgress:R}, provisional={provisional}.");
                Check(admitted == !rejected && afterRow == beforeRow,
                    "The current follow request is admitted at the identical physical row, while a stale restore is rejected without movement.");
                await ProvisionalAsync(view, original, label + "-after-request", log);
                var held = Observe(view, "wrapped");
                Check(held.Offset == provisional.Offset && Math.Abs(held.Within - provisional.Within) <= PixelTolerance(view),
                    "The same-row request leaves the actual Pending body line and displacement unchanged before release.");

                view.IsResourceRenderingDeferred = false;
                await SettledAsync(view, null, label + "-ready", log);
                var after = Observe(view, rejected ? "atomic" : "wrapped");
                log.AppendLine("OBS: ThemeWidth " + label + " after resources " + after);
                if (rejected)
                    AssertPreserved(view, "atomic", original, after);
                else
                {
                    AssertPreserved(view, "wrapped", provisional, after);
                    Check(after.Offset > original.Offset,
                        "The newer admitted body-line intent survives resource completion instead of returning to the obsolete tall-object anchor.");
                }
                var source = view.GetSourceState();
                Check(source.StateKnown && source.DocumentId == document && source.Revision == 1 && view.DisplayedText == text && Selection(view) == selection,
                    "Same-row admission/rejection and deferred resource completion preserve exact Main text, source identity/revision and selection.");
            }
            finally
            {
                view.Cancel();
                root.Children.Clear();
                Window.Current.Content = previous;
            }
        }

        private static async Task ProvisionalAsync(MarkdownView view, Observation original, string label, StringBuilder log)
        {
            var complete = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
            Observation? previous = null;
            Observation? last = null;
            int turns = 0, consecutive = 0;
            void Rendering(object sender, object args)
            {
                try
                {
                    ++turns;
                    var anchor = view.GetViewportAnchor();
                    var objects = MarkdownMermaidTests.ReadObjects(view);
                    var quiet = view.IsResourceRenderingDeferred && view.IsSurfaceReady && string.IsNullOrEmpty(view.RenderingError) &&
                        anchor.IsAvailable && anchor.DocumentId == original.Document && anchor.Revision == original.Revision &&
                        anchor.TextGeneration == original.Text && anchor.LayoutGeneration > original.Layout && anchor.PreviewUtf16Offset > original.Offset &&
                        objects.Count(item => item.Family == "Formula") == 2 && objects.Count(item => item.Family == "Diagram") == 2 &&
                        objects.Where(item => item.Family == "Diagram").All(item => item.State == "Pending");
                    if (!quiet) { consecutive = 0; return; }
                    last = Observe(view, "wrapped");
                    consecutive = previous == last ? consecutive + 1 : 1;
                    previous = last;
                    if (consecutive >= 4) complete.TrySetResult(true);
                }
                catch (Exception error) when (unchecked((uint)error.HResult) == 0x80040201) { consecutive = 0; }
                catch (Exception error) { complete.TrySetException(error); }
            }
            CompositionTarget.Rendering += Rendering;
            try { await complete.Task.WaitAsync(TimeSpan.FromSeconds(25)); }
            finally
            {
                CompositionTarget.Rendering -= Rendering;
                log.AppendLine($"OBS: ThemeWidth {label} currentPending={complete.Task.Status}, renderingTurns={turns}, stableTurns={consecutive}, last={last}.");
                log.AppendLine("OBS: ThemeWidth " + label + " resources " + string.Join(", ", MarkdownMermaidTests.ReadObjects(view).Select(item => item.Family + ":" + item.State)) + ".");
            }
        }

        private static async Task PositionAsync(MarkdownView view, MarkdownExtensionView extension, string target, string label, StringBuilder log)
        {
            var identity = view.GetSelection();
            ulong offset;
            if (target == "atomic")
            {
                var sourceByte = checked((ulong)Encoding.UTF8.GetByteCount(Source.Substring(0,
                    Source.IndexOf("```mermaid\rstateDiagram-v2", StringComparison.Ordinal))));
                var mapped = await view.MapSourcePositionAsync(identity.DocumentId, identity.Revision, identity.TextGeneration,
                    sourceByte, MarkdownPositionAffinity.After).AsTask().WaitAsync(TimeSpan.FromSeconds(25));
                offset = mapped.PreviewUtf16Offset;
            }
            else
                offset = checked((ulong)(view.DisplayedText.IndexOf(target == "wrapped" ? WrappedWord : Middle, StringComparison.Ordinal) +
                    (target == "wrapped" ? 3 : 0)));
            Check(offset > 0 && offset < view.DisplayedTextLength, "The authored reading destination exists in the real Main text.");
            view.RevealRange(identity.DocumentId, identity.Revision, identity.TextGeneration, offset, 0, true, false);
            await SettledAsync(view, extension, label + "-reveal", log);
            var scroll = Scroll(view);
            var delta = target == "atomic" ? 100.25 : 6.25;
            Check(view.TrySetVerticalScrollProgress((scroll.VerticalOffset + delta) / scroll.ScrollableHeight),
                "An interior fractional reading position is admitted without changing selection.");
            await SettledAsync(view, extension, label + "-positioned", log);
            var positioned = Observe(view, target);
            Check(positioned.Y > 100 && positioned.Y < scroll.ScrollableHeight - scroll.ViewportHeight && positioned.Within > 1,
                "The reading oracle starts away from document clamps with a real offset inside its line/object.");
            if (target == "middle") Check(positioned.Offset == offset, "The initial standalone target is exactly the top Main line.");
            if (target == "wrapped")
                Check(positioned.Offset < offset && positioned.End > offset,
                    "The initial wrapped destination is inside an independently queried actual display line.");
            if (target == "atomic")
            {
                Check(positioned.Offset == offset && positioned.Within >= 90,
                    "The initial tall state diagram is the top atomic object with a nonzero within-object position.");
                var diagram = MarkdownMermaidTests.ReadObjects(view).Single(item => item.Name.StartsWith("Diagram: stateDiagram-v2", StringComparison.Ordinal));
                log.AppendLine($"OBS: ThemeWidth {label} positioned atomic bounds={diagram.Bounds}, signedWithin={positioned.Within:R}.");
                Check(diagram.Bounds.Height > 160 * view.XamlRoot.RasterizationScale,
                    "The actual rendered state object extends well beyond the within-object reading displacement.");
            }
        }

        private static void AssertPreserved(MarkdownView view, string target, Observation before, Observation after)
        {
            var tolerance = PixelTolerance(view);
            Check(after.Document == before.Document && after.Revision == before.Revision && after.Text == before.Text && after.Layout > before.Layout,
                "A newer actual presented layout retains the original document/text identity.");
            if (target == "wrapped")
                Check(after.Offset <= before.Offset && before.Offset < after.End,
                    "After rewrapping the old text position remains inside the actual first display line; its new line start may differ.");
            else
                Check(after.Offset == before.Offset, "The same unique standalone line or atomic object remains at the top.");
            Check(Math.Abs(after.Within - before.Within) <= tolerance,
                $"The within-line/object reading position survives relayout within one physical pixel: {before.Within:R}->{after.Within:R}.");
            if (target == "middle")
                Check(before.ProbeY.HasValue && after.ProbeY.HasValue && Math.Abs(after.ProbeY.Value - before.ProbeY.Value) <= 1.1,
                    "The independent native UIA glyph rectangle in the actually presented frame retains its screen Y within one physical pixel.");
        }

        private readonly record struct Observation(Guid Document, ulong Revision, ulong Text, ulong Layout, ulong Offset,
            double Within, ulong End, double Y, double Extent, double Width, double Height, double? ProbeY);

        private static Observation Observe(MarkdownView view, string target)
        {
            var anchor = view.GetViewportAnchor();
            Check(anchor.IsAvailable && view.IsSurfaceReady && string.IsNullOrEmpty(view.RenderingError), "An actual current presented reading frame is available.");
            ulong end = anchor.PreviewUtf16Offset + 1;
            double? probeY = null;
            var reading = Reading(view);
            if (target == "wrapped")
            {
                var visible = reading.GetVisibleRanges();
                Check(visible.Length > 0, "The actual native frame exposes a visible text range.");
                var line = visible[0].Clone();
                line.MoveEndpointByRange(TextPatternRangeEndpoint.End, line, TextPatternRangeEndpoint.Start);
                line.ExpandToEnclosingUnit(TextUnit.Line);
                var text = line.GetText(-1);
                var start = view.DisplayedText.IndexOf(text, StringComparison.Ordinal);
                Check(text.Length > 0 && start >= 0 && (ulong)start == anchor.PreviewUtf16Offset,
                    "The first native display line is uniquely identified in the public Main text.");
                end = checked((ulong)(start + text.Length));
            }
            if (target == "middle")
            {
                var probe = reading.DocumentRange.FindText("W1 independent following probe.", false, false);
                if (probe != null)
                {
                    probe.GetBoundingRectangles(out var rectangles);
                    if (rectangles is { Length: 4 }) probeY = rectangles[1];
                }
            }
            var scroll = Scroll(view);
            return new Observation(anchor.DocumentId, anchor.Revision, anchor.TextGeneration, anchor.LayoutGeneration,
                anchor.PreviewUtf16Offset, anchor.OffsetWithinLine, end, scroll.VerticalOffset, scroll.ExtentHeight,
                view.ActualWidth, view.ActualHeight, probeY);
        }

        // Four separate Rendering turns must observe unchanged actual presented
        // identity/geometry and terminal resources. This predicate does not know the
        // expected target and never scrolls to make the assertion pass.
        private static async Task SettledAsync(MarkdownView view, MarkdownExtensionView extension, string label, StringBuilder log)
        {
            var complete = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
            string previous = null, last = null;
            int consecutive = 0, turns = 0;
            void Rendering(object sender, object args)
            {
                try
                {
                    ++turns;
                    var anchor = view.GetViewportAnchor();
                    var scroll = Scroll(view);
                    var quiet = anchor.IsAvailable && view.GetSelection().IsAvailable && view.IsSurfaceReady &&
                        string.IsNullOrEmpty(view.RenderingError) && !view.IsResourceRenderingDeferred && ResourcesReady(view) &&
                        scroll.ViewportHeight > 0 && scroll.ScrollableHeight > scroll.ViewportHeight &&
                        (extension == null || (extension.SourceSession.IsCurrent && extension.TestThemeWidthLinkQuiescent));
                    last = FormattableString.Invariant($"{anchor.DocumentId}/{anchor.Revision}/{anchor.TextGeneration}/{anchor.LayoutGeneration}/{anchor.PreviewUtf16Offset}:{anchor.OffsetWithinLine:R}/{scroll.VerticalOffset:R}/{scroll.ExtentHeight:R}/{view.ActualWidth:R}/{view.ActualHeight:R}");
                    consecutive = quiet ? last == previous ? consecutive + 1 : 1 : 0;
                    previous = last;
                    if (consecutive >= 4) complete.TrySetResult(true);
                }
                catch (Exception error) when (unchecked((uint)error.HResult) == 0x80040201) { consecutive = 0; }
                catch (Exception error) { complete.TrySetException(error); }
            }
            CompositionTarget.Rendering += Rendering;
            try { await complete.Task.WaitAsync(TimeSpan.FromSeconds(25)); }
            finally
            {
                CompositionTarget.Rendering -= Rendering;
                log.AppendLine($"OBS: ThemeWidth {label} presented={complete.Task.Status}, renderingTurns={turns}, stableTurns={consecutive}, last={last}, link={extension?.TestResourceLinkState}.");
                log.AppendLine("OBS: ThemeWidth " + label + " resources " + string.Join(", ", MarkdownMermaidTests.ReadObjects(view).Select(item => item.Family + ":" + item.State)) + ".");
            }
        }

        private static bool ResourcesReady(MarkdownView view)
        {
            var objects = MarkdownMermaidTests.ReadObjects(view);
            var images = Peer(view).GetChildren()?.Where(item => item.GetAutomationControlType() == AutomationControlType.Image &&
                item.GetName().StartsWith("W1 loaded image, ", StringComparison.Ordinal)).ToArray();
            return objects.Count(item => item.Family == "Formula") == 2 && objects.Count(item => item.Family == "Diagram") == 2 &&
                objects.All(item => item.State == "Ready") && images is { Length: 1 } && images[0].GetHelpText() == "Image loaded";
        }

        private static AutomationPeer Peer(MarkdownView view) => FrameworkElementAutomationPeer.FromElement(view) ?? FrameworkElementAutomationPeer.CreatePeerForElement(view);
        private static ITextProvider Reading(MarkdownView view) => (ITextProvider)Peer(view).GetPattern(PatternInterface.Text);
        private static ScrollViewer Scroll(MarkdownView view) => Descendants(view).OfType<ScrollViewer>().Single(item => item.Name == "PreviewScrollViewer");
        private static double PixelTolerance(MarkdownView view) => 1.1 / view.XamlRoot.RasterizationScale;
        private static (Guid Document, ulong Revision, ulong Text, ulong Start, ulong Length) Selection(MarkdownView view)
        {
            var selection = view.GetSelection();
            return (selection.DocumentId, selection.Revision, selection.TextGeneration, selection.Start, selection.Length);
        }

        private static string Stamp(TextEditor editor)
        {
            var native = editor.TestNativeCore.TestFeatureNative;
            return editor.GetText() + "|" + native.DocumentRevision + "|" + editor.TestNativeCore.ContentVersion + "|" +
                native.DocumentSequence + "|" + native.UndoSequence + "|" + native.UndoSavePoint + "|" + native.Modify + "|" + editor.IsModified + "|" +
                native.CanUndo() + "|" + native.CanRedo() + "|" + native.Anchor + "|" + native.CurrentPos + "|" + native.SelectionRevision;
        }

        private static async Task ResizeAsync(double width)
        {
            Check(ApplicationView.GetForCurrentView().TryResizeView(new Size(width, 760)), "The owned window admits its initial fixture width.");
            await UntilAsync(() => Math.Abs(Window.Current.CoreWindow.Bounds.Width - width) < 0.6, "The owned window reaches its initial fixture width.");
        }

        private static void SetNativeTheme(MarkdownView view, bool dark) => view.SetTheme(new MarkdownTheme
        {
            Generation = view.ThemeGeneration + 1, FontFamily = "Segoe UI", CodeFontFamily = "Consolas",
            FontSize = 16, LineHeight = 24, Padding = 16,
            Foreground = dark ? Colors.White : Colors.Black, Background = dark ? Colors.Black : Colors.White,
            LinkForeground = Colors.DodgerBlue, BorderColor = Colors.Gray, PanelBackground = dark ? Colors.Black : Colors.White
        });

        [WinRT.GeneratedWinRTExposedType]
        private sealed partial class Images : IMarkdownImageProvider
        {
            public IAsyncOperation<IRandomAccessStream> OpenAsync(MarkdownImageRequest request) => AsyncInfo.Run<IRandomAccessStream>(async cancellation =>
            {
                var stream = new InMemoryRandomAccessStream();
                try
                {
                    await stream.WriteAsync(Convert.FromBase64String(Png).AsBuffer());
                    cancellation.ThrowIfCancellationRequested();
                    stream.Seek(0);
                    return stream;
                }
                catch { stream.Dispose(); throw; }
            });
        }
    }
}

namespace Notepads.Presentation.Controls.Markdown
{
    public sealed partial class MarkdownExtensionView
    {
        // A follow-off geometry notification can leave an inert resample flag:
        // no viewport mapping is eligible to consume it. All actual work,
        // restoration, rendering and source-animation guards still apply.
        internal bool TestThemeWidthLinkQuiescent
        {
            get
            {
                var source = _linkEditor?.Content is Grid grid && grid.Children.Count > 0 ? grid.Children[0] as WinUIEditor.EditorBaseControl : null;
                return _active && !_disposed && _linkRunning && !_restoringSessionViewport && !_viewportResourceReleasePending &&
                    _linkCompletion.IsCompleted && _linkCancellation == null && !_linkCoordinator.Active.HasValue && !_linkCoordinator.HasPending &&
                    !_linkRendering && (!FollowSource || !_linkViewportResample) && _linkApplying == 0 && _linkSampling == 0 &&
                    source != null && !source.IsViewportRestorePending && !source.IsScrollAnimating;
            }
        }
    }
}
