// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Linq;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Text;
using System.Threading.Tasks;
using Notepads.Markdown;
using Notepads.Presentation.Controls.Markdown;
using Windows.Foundation;
using Windows.Storage.Streams;
using Windows.UI.Core;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Markup;
using Windows.UI.Xaml.Media;
using static NotepadsWorkspaceTests.DocumentFeatureAssertions;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests
{
    // Functional admission/lifecycle regressions. No elapsed-time performance
    // thresholds, network input, arbitrary settling sleeps or native substitutes.
    internal static partial class MarkdownResourceDeferralTests
    {
        private const string Png = "iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAEklEQVR4nGP4z8DwHwyBNBgAAEnICff5q7YNAAAAAElFTkSuQmCC";
        private const MarkdownExtensions Extensions = MarkdownExtensions.Math | MarkdownExtensions.Mermaid;

        private static string Source(string image, string label = "Resource barrier") =>
            "# " + label + "\r\r![barrier image](" + image + ")\r\r" +
            "$x^2$ $\\text{汉\u202E字}$ $\\frac{1}{$\r\r" +
            "```mermaid\rflowchart TD\rA-->B\r```\r\r" +
            "```mermaid\rflowchart LR\rA[\"unclosed node\r```\r\r" +
            string.Join("\r\r", Enumerable.Range(0, 30).Select(index => "Leading paragraph " + index)) +
            "\r\r## Deferred middle target\r\rMiddle destination text.\r\r" +
            string.Join("\r\r", Enumerable.Range(0, 35).Select(index => "Trailing paragraph " + index));

        internal static async Task RunAsync(StringBuilder log)
        {
            var failures = new List<string>();
            await CaseAsync("native-default-and-barrier", NativeBarrierAsync);
            await CaseAsync("native-queued-release-source-guard", NativeQueueRaceAsync);
            await CaseAsync("native-canceled-edit-retires-note", NativeCanceledEditRetiresNoteAsync);
            await CaseAsync("native-derived-edit-reuse", NativeDerivedEditReuseAsync);
            await CaseAsync("native-derived-failure-edit-reuse", NativeDerivedFailureEditReuseAsync);
            foreach (var mode in new[] { "top", "middle", "middle-repeat-2", "middle-repeat-3", "source-replaced", "unload-reload", "follow-off", "fallback", "close-before-ready", "failure" })
                await CaseAsync("host-" + mode, () => HostAsync(mode, log));
            if (failures.Count != 0) throw new InvalidOperationException(string.Join("\n", failures));

            async Task CaseAsync(string name, Func<Task> action)
            {
                try { await action(); log.AppendLine("PASS: Markdown.ResourceDeferral " + name); }
                catch (Exception error) { failures.Add(name + ": " + error); log.AppendLine("FAIL: Markdown.ResourceDeferral " + name + ": " + error); }
            }
        }

        internal static async Task RunStableImageGeometryAsync(StringBuilder log)
        {
            await using var host = await FeatureTestHost.CreateAsync();
            var root = (Panel)host.Page.Content;
            foreach (bool fixedSize in new[] { true, false })
            {
                var view = CreateView(root);
                try
                {
                    view.IsResourceRenderingDeferred = true;
                    var provider = new CountingImages();
                    view.SetImageProvider(provider, 1);
                    await LoadedAsync(view);
                    var image = fixedSize
                        ? "<p>Before <img src='fixed.png' alt='test image' width='40' height='24'> after.</p>"
                        : "Before ![test image](natural.png) after.";
                    await SetAsync(view, "# Image\r\r" + image + "\r\rAfter the image.",
                        Extensions | MarkdownExtensions.HtmlBlocks | MarkdownExtensions.HtmlInline);
                    var peer = FrameworkElementAutomationPeer.FromElement(view) ??
                        FrameworkElementAutomationPeer.CreatePeerForElement(view);
                    await UntilAsync(() => ImagePeer() != null && view.GetViewportAnchor().IsAvailable,
                        "The image object and its placeholder geometry are ready.");
                    var initial = view.GetViewportAnchor();
                    var initialBounds = ImagePeer().GetBoundingRectangle();
                    // The first publication may precede the newly visible gutter's
                    // XAML arrange. Settle it while decode is still prohibited.
                    await TurnsAsync();
                    await UntilAsync(() => ImagePeer() != null && view.GetViewportAnchor().IsAvailable,
                        "The arranged deferred image frame is current.");
                    var before = view.GetViewportAnchor();
                    var beforeBounds = ImagePeer().GetBoundingRectangle();
                    Check(provider.Total == 0, "The geometry baseline settles before any image request.");
                    log.AppendLine($"OBS: Markdown.ImageGeometry deferred fixed={fixedSize}, layout {initial.LayoutGeneration}->{before.LayoutGeneration}, bounds {initialBounds}->{beforeBounds}, offset {initial.PreviewUtf16Offset}->{before.PreviewUtf16Offset}, requests={provider.Total}.");
                    view.IsResourceRenderingDeferred = false;
                    await UntilAsync(() => view.GetViewportAnchor().IsAvailable &&
                        ImagePeer()?.GetHelpText() == "Image loaded",
                        "The provider's actual bitmap is painted and published.", 15000);
                    var after = view.GetViewportAnchor();
                    var afterBounds = ImagePeer().GetBoundingRectangle();
                    if (fixedSize)
                    {
                        Check(after.LayoutGeneration == before.LayoutGeneration && afterBounds.Equals(beforeBounds) &&
                            after.PreviewUtf16Offset == before.PreviewUtf16Offset,
                            $"Fixed-size image pixels preserve the layout generation, bounds and viewport anchor. layout {before.LayoutGeneration}->{after.LayoutGeneration}, bounds {beforeBounds}->{afterBounds}, offset {before.PreviewUtf16Offset}->{after.PreviewUtf16Offset}.");
                        var bitmap = new Windows.UI.Xaml.Media.Imaging.RenderTargetBitmap();
                        await bitmap.RenderAsync(view);
                        var pixels = (await bitmap.GetPixelsAsync()).ToArray();
                        bool hasRed = false;
                        for (int i = 0; i + 3 < pixels.Length; i += 4)
                            if (pixels[i] < 40 && pixels[i + 1] < 40 && pixels[i + 2] > 210 && pixels[i + 3] > 200)
                            {
                                hasRed = true;
                                break;
                            }
                        Check(hasRed, "The fixed-size surface now contains the PNG's actual red pixels.");
                    }
                    else
                        Check(after.LayoutGeneration > before.LayoutGeneration && afterBounds.Width < beforeBounds.Width,
                            "A previously unknown image size uses real layout when its inline metrics change.");

                    log.AppendLine($"PASS: Markdown.ImageGeometry fixed={fixedSize}, layout {before.LayoutGeneration}->{after.LayoutGeneration}, bounds {beforeBounds}->{afterBounds}.");
                    AutomationPeer ImagePeer() => peer.GetChildren()?.SingleOrDefault(item =>
                        item.GetAutomationControlType() == AutomationControlType.Image);
                }
                finally { view.Cancel(); root.Children.Remove(view); }
            }
        }

        private static async Task NativeCanceledEditRetiresNoteAsync()
        {
            await using var host = await FeatureTestHost.CreateAsync();
            var root = (Panel)host.Page.Content;
            var view = CreateView(root);
            try
            {
                await LoadedAsync(view);
                await SetAsync(view, "Before the note.\r\r![[Excerpt]]", MarkdownExtensions.NoteEmbeds);
                await UntilAsync(() => view.GetVisibleNoteEmbeds().Count == 1, "The standalone note slot is visible.");
                var slot = view.GetVisibleNoteEmbeds().Single();
                var before = view.GetSelection();
                var child = new MarkdownView();
                Check(view.SetNoteEmbedChild(before.DocumentId, before.Revision, before.TextGeneration, slot.ObjectId, child),
                    "The standalone view mounts the note child.");
                var overlay = Descendants(view).OfType<Canvas>().Single(item => item.Name == "ViewportOverlay");
                await LoadedAsync(child);
                await SetAsync(child, "Child body.");
                Check(overlay.Children.Contains(child), "The note is mounted before editing.");

                var bytes = Encoding.UTF8.GetBytes("Inserted.\r\r");
                var operation = view.ApplyEditsAsync(before.DocumentId, before.Revision, before.Revision + 1,
                    [new MarkdownSourceEdit { ReplacementLength = (ulong)bytes.Length }], bytes);
                Check(overlay.Children.Contains(child), "The edit retains the unchanged note while preparing a frame.");
                var pending = operation.AsTask();
                // Cancel before yielding the UI thread to the replacement frame.
                operation.Cancel();
                try
                {
                    var result = await pending.WaitAsync(TimeSpan.FromSeconds(15));
                    Check(result.Status == DocumentUpdateStatus.Canceled, "The edit must settle as Canceled.");
                }
                catch (OperationCanceledException) { }
                await UntilAsync(() => !overlay.Children.Contains(child),
                    "A canceled edit detaches the note without any host retirement.");
                Check(view.DisplayedRevision == before.Revision, "Cancellation never publishes the replacement frame.");
            }
            finally { view.Cancel(); root.Children.Remove(view); }
        }

        private static async Task NativeBarrierAsync()
        {
            await using var host = await FeatureTestHost.CreateAsync();
            var root = (Panel)host.Page.Content;
            var ordinary = CreateView(root);
            var provider = new CountingImages();
            try
            {
                Check(!ordinary.IsResourceRenderingDeferred, "A native consumer defaults to ordinary resource admissions.");
                ordinary.SetImageProvider(provider, 1);
                await LoadedAsync(ordinary);
                await SetAsync(ordinary, Source("ordinary.png"));
                await TerminalAsync(ordinary);
                Check(provider.Count("ordinary.png") == 1, "Default-false normal publication starts the image provider exactly once.");
            }
            finally { ordinary.Cancel(); root.Children.Remove(ordinary); }

            var view = CreateView(root);
            provider = new CountingImages();
            try
            {
                view.IsResourceRenderingDeferred = true;
                view.SetImageProvider(provider, 1);
                await LoadedAsync(view);
                await SetAsync(view, Source("held.png"));
                // Finish the standalone view's initial template arrangement
                // before observing a fixed base frame.
                await TurnsAsync();
                await HeldAcrossTurnsAsync(view);
                Check(provider.Total == 0, "Deferral admits no image provider request while the published base remains readable.");
                view.IsResourceRenderingDeferred = false;
                Check(provider.Total == 0, "Releasing the barrier queues resource work instead of running it inline.");
                await TerminalAsync(view);
                Check(provider.Count("held.png") == 1, "The queued release admits the same-source image once.");
                view.IsResourceRenderingDeferred = false;
                await TurnsAsync();
                Check(provider.Total == 1, "An equal false assignment does not duplicate already-bound resource work.");
            }
            finally { view.Cancel(); root.Children.Remove(view); }
        }

        private static async Task NativeQueueRaceAsync()
        {
            await using var host = await FeatureTestHost.CreateAsync();
            var root = (Panel)host.Page.Content;
            var view = CreateView(root);
            var provider = new CountingImages();
            try
            {
                view.IsResourceRenderingDeferred = true;
                view.SetImageProvider(provider, 1);
                await LoadedAsync(view);
                await SetAsync(view, Source("old-a.png", "A"));
                // A first publication can precede XAML arranging its final scroll extent.
                // Exercise the queued resource release against a settled base viewport.
                await TurnsAsync();
                view.IsResourceRenderingDeferred = false;
                view.IsResourceRenderingDeferred = true;
                await HeldAcrossTurnsAsync(view);
                Check(provider.Total == 0, "A later true assignment suppresses the already queued false kickoff.");
                await SetAsync(view, Source("old-b.png", "B"));
                await TurnsAsync();
                await HeldAcrossTurnsAsync(view);
                Check(provider.Total == 0, "A replacement source remains deferred.");
                view.IsResourceRenderingDeferred = false;
                var latest = view.SetDocumentWithExtensionsAsync(Guid.NewGuid(), 1,
                    Encoding.UTF8.GetBytes(Source("current-c.png", "C")), Extensions);
                var result = await latest.AsTask().WaitAsync(TimeSpan.FromSeconds(15));
                Check(result.Status == DocumentUpdateStatus.Ready && result.Published, "The replacement source publishes after an obsolete release was queued.");
                await TerminalAsync(view);
                Check(provider.Count("old-a.png") == 0 && provider.Count("old-b.png") == 0 && provider.Count("current-c.png") == 1,
                    "Only the current source starts resources; an obsolete kickoff does not strand default-false publication.");
            }
            finally { view.Cancel(); root.Children.Remove(view); }
        }

        private static async Task NativeDerivedEditReuseAsync()
        {
            await using var host = await FeatureTestHost.CreateAsync();
            var root = (Panel)host.Page.Content;
            var view = CreateView(root);
            view.Height = 440;
            const string source = "Before $x^2$ and $b^2$.\r\r```mermaid\rflowchart TD\rA-->B\r```\r";
            const string prefix = "Inserted paragraph.\r\r![Earlier object](missing.png)\r\r";
            try
            {
                await LoadedAsync(view);
                await SetAsync(view, source);
                await UntilAsync(AllReady, "The two formulas and diagram reach their real rendered state.");

                // Hold admission through both edits. Ready on a newly published
                // revision can only come from retaining an existing artifact.
                view.IsResourceRenderingDeferred = true;
                ulong[] phases;
                CoreValidationProbe.StartPreviewStartupTrace(false);
                try { await PublishEditAsync(view, 0, 0, prefix); }
                finally { phases = CoreValidationProbe.StopPreviewStartupTrace(); }
                Check(AllReady(), "An unrelated insertion preserves all three completed resources on its first published frame.");
                const ulong measureReadyPhase = 12; // StartupTrace::Phase::MeasureReady
                var readyPasses = phases.Where((value, index) => index % 3 == 0 && value == measureReadyPhase).Count();
                Check(readyPasses == 1, "Reusing completed resources on an edit needs exactly one measured layout.");
                var formulaStart = (ulong)Encoding.UTF8.GetByteCount(prefix + "Before $");
                await PublishEditAsync(view, formulaStart, 3, "z^2");
                var states = MarkdownMermaidTests.ReadObjects(view);
                Check(states.Length == 3 &&
                    states.Any(item => item.Name == "Formula: z^2" && item.State == "Pending") &&
                    states.Any(item => item.Name == "Formula: b^2" && item.State == "Ready") &&
                    states.Any(item => item.Family == "Diagram" && item.State == "Ready"),
                    "Only the changed formula loses its existing artifact.");
                var layout = view.GetViewportAnchor().LayoutGeneration;
                view.Width = 360;
                await UntilAsync(() => view.ActualWidth == 360 &&
                    view.GetViewportAnchor() is { IsAvailable: true } anchor && anchor.LayoutGeneration > layout,
                    "A new viewport width publishes while resource admission is still held.");
                states = MarkdownMermaidTests.ReadObjects(view);
                Check(states.Any(item => item.Name == "Formula: b^2" && item.State == "Ready") &&
                    states.Any(item => item.Family == "Diagram" && item.State == "Pending"),
                    "Changing the target width invalidates the fitted diagram but retains independent formula geometry.");
                view.IsResourceRenderingDeferred = false;
                await UntilAsync(AllReady, "Releasing admission renders the changed formula and resized diagram.");
            }
            finally { view.Cancel(); root.Children.Remove(view); }

            bool AllReady() => MarkdownMermaidTests.ReadObjects(view) is { Length: 3 } states &&
                states.All(item => item.State == "Ready");
        }

        private static async Task NativeDerivedFailureEditReuseAsync()
        {
            await using var host = await FeatureTestHost.CreateAsync();
            var root = (Panel)host.Page.Content;
            var view = CreateView(root);
            view.Height = 440;
            const string beforeFormula = "Valid $x^2$ and malformed $";
            const string failedFormula = @"\frac{1}{";
            const string beforeDiagram = "$.\r\r```mermaid\rflowchart LR\r";
            const string failedDiagram = "A[\"unclosed node";
            const string repairedFormula = "b^2";
            const string prefix = "Inserted paragraph.\r\r![Earlier object](missing.png)\r\r";
            try
            {
                await LoadedAsync(view);
                await SetAsync(view, beforeFormula + failedFormula + beforeDiagram + failedDiagram + "\r```\r");
                await UntilAsync(OriginalFailuresPublished, "The original formula and diagram publish real syntax failures.");

                view.IsResourceRenderingDeferred = true;
                await PublishEditAsync(view, 0, 0, prefix);
                Check(OriginalFailuresPublished(),
                    "An unrelated insertion keeps both syntax failures and the valid sibling while new work is deferred.");

                var formulaStart = (ulong)Encoding.UTF8.GetByteCount(prefix + beforeFormula);
                await PublishEditAsync(view, formulaStart, (ulong)Encoding.UTF8.GetByteCount(failedFormula), repairedFormula);
                var states = MarkdownMermaidTests.ReadObjects(view);
                Check(states.Length == 3 &&
                    states.Any(item => item.Name == "Formula: b^2" && item.State == "Pending") &&
                    states.Any(item => item.Family == "Diagram" && item.State == "Failed") &&
                    states.Any(item => item.Name == "Formula: x^2" && item.State == "Ready"),
                    "Repairing the formula invalidates its own failure while the unchanged diagram failure and valid sibling remain.");

                var diagramStart = (ulong)Encoding.UTF8.GetByteCount(prefix + beforeFormula + repairedFormula + beforeDiagram);
                await PublishEditAsync(view, diagramStart, (ulong)Encoding.UTF8.GetByteCount(failedDiagram), "A-->B");
                states = MarkdownMermaidTests.ReadObjects(view);
                Check(states.Length == 3 &&
                    states.Any(item => item.Family == "Diagram" && item.State == "Pending") &&
                    states.Any(item => item.Name == "Formula: x^2" && item.State == "Ready"),
                    "Repairing the diagram invalidates its failure while the valid sibling remains rendered.");

                view.IsResourceRenderingDeferred = false;
                await UntilAsync(() => MarkdownMermaidTests.ReadObjects(view) is { Length: 3 } rendered &&
                    rendered.All(item => item.State == "Ready"), "Releasing admission renders both repaired inputs.");
            }
            finally { view.Cancel(); root.Children.Remove(view); }

            bool OriginalFailuresPublished() => MarkdownMermaidTests.ReadObjects(view) is { Length: 3 } states &&
                states.Any(item => item.Name == "Formula: " + failedFormula &&
                    item.Help.StartsWith("Formula syntax error.", StringComparison.Ordinal)) &&
                states.Any(item => item.Family == "Diagram" && item.State == "Failed") &&
                states.Any(item => item.Name == "Formula: x^2" && item.State == "Ready");
        }

        private static async Task PublishEditAsync(MarkdownView view, ulong start, ulong length, string replacement)
        {
            var before = view.GetSourceState();
            var bytes = Encoding.UTF8.GetBytes(replacement);
            var result = await view.ApplyEditsAsync(before.DocumentId, before.Revision, before.Revision + 1,
                [new MarkdownSourceEdit { Start = start, DeleteLength = length, ReplacementLength = (ulong)bytes.Length }],
                bytes).AsTask().WaitAsync(TimeSpan.FromSeconds(15));
            Check(result.Status == DocumentUpdateStatus.Ready && result.Published &&
                view.DisplayedRevision == before.Revision + 1 && view.GetSelection().IsAvailable,
                "The incremental revision itself is published with a current reading frame.");
        }

        private static async Task HostAsync(string mode, StringBuilder log)
        {
            await using var host = await FeatureTestHost.CreateAsync();
            var source = Source("data:image/png;base64," + Png);
            var editor = await OpenAsync(host, mode + ".md", Encoding.UTF8.GetBytes(source));
            var original = Stamp(editor);
            var metadata = editor.GetTextEditorStateMetaData();
            metadata.IsContentPreviewPanelOpened = true;
            metadata.FirstVisibleDocumentLine = mode == "top" ? 0 : Array.IndexOf(source.Split('\r'), "## Deferred middle target");
            metadata.FirstVisibleSubLine = 0;
            metadata.FirstVisibleRowOffset = 0;
            metadata.ScrollViewerHorizontalOffset = metadata.ScrollViewerVerticalOffset = 0;
            var targetByte = checked((ulong)editor.TestNativeCore.TestFeatureNative.PositionFromLine(metadata.FirstVisibleDocumentLine.Value));
            MarkdownViewportAnchor first = null;
            bool exposedWithoutAnchor = false, exposedTop = false;
            void Rendering(object sender, object args)
            {
                var ext = editor.TestMarkdownPreview;
                if (!Visible(ext)) return;
                var anchor = ext.PreviewView.GetViewportAnchor();
                if (first == null) { first = anchor; exposedWithoutAnchor = !anchor.IsAvailable; }
                exposedTop |= anchor.IsAvailable && anchor.PreviewUtf16Offset == 0 && anchor.ScrollStartUtf16Offset == 0;
            }
            Action release = () => { };
            MarkdownExtensionView extension = null;
            var content = Window.Current.Content;
            var stage = "reset-metadata";
            CompositionTarget.Rendering += Rendering;
            try
            {
                editor.ResetEditorState(metadata);
                extension = editor.TestMarkdownPreview;
                Check(extension != null && extension.PreviewView.IsResourceRenderingDeferred && extension.PreviewView.Opacity == 0,
                    "The restore owner sets deferral before the new extension can load/start its session.");
                if (mode == "close-before-ready")
                {
                    editor.ShowHideContentPreview();
                    await extension.DisposalCompletion.WaitAsync(TimeSpan.FromSeconds(20));
                    await TurnsAsync();
                    Check(!extension.PreviewView.IsResourceRenderingDeferred && !editor.TestMarkdownPanelOpen,
                        "Closing before Ready retires activity and releases the flag without a stranded restore.");
                    return;
                }
                if (mode == "failure")
                {
                    extension.PreviewView.Template = (ControlTemplate)XamlReader.Load(
                        "<ControlTemplate xmlns='http://schemas.microsoft.com/winfx/2006/xaml/presentation' xmlns:m='using:Notepads.Markdown' TargetType='m:MarkdownView'><Grid/></ControlTemplate>");
                    await UntilAsync(() => extension.SourceSession.Status == MarkdownSessionStatus.Failed,
                        "An unsupported real native template fails the initial submission.", 20000);
                    Check(!extension.PreviewView.IsResourceRenderingDeferred && extension.PreviewView.Opacity == 1 && extension.PreviewView.IsEnabled,
                        "A failed initial restore releases deferral and removes its hidden/disabled state.");
                    return;
                }
                var held = extension.TestHoldNextSourceViewport(out release,
                    holdUntilRelease: mode is "unload-reload" or "source-replaced" or "fallback",
                    pauseRestoreDeadline: mode is "unload-reload" or "source-replaced");
                stage = "wait-held-viewport-map";
                await held.WaitAsync(TimeSpan.FromSeconds(20));
                Check(extension.PreviewView.IsResourceRenderingDeferred && !Visible(extension), "An asynchronously held viewport map retains restore ownership.");
                stage = "validate-held-reading-turns";
                await HeldAcrossTurnsAsync(extension.PreviewView);
                if (mode == "follow-off") extension.FollowSource = false;
                else if (mode == "unload-reload")
                {
                    stage = "detach-pending-restore";
                    Check(extension.TestSessionViewportRestoring && extension.PreviewView.IsResourceRenderingDeferred,
                        "The pending restore is still held just before the temporary unload.");
                    Window.Current.Content = new Grid();
                    await UntilAsync(() => !extension.IsLoaded, "The hidden restore actually unloads.");
                    Check(extension.PreviewView.IsResourceRenderingDeferred && extension.PreviewView.Opacity == 0,
                        "Temporary unload preserves both pending restore and resource deferral.");
                    release();
                    Window.Current.Content = content;
                }
                else if (mode == "source-replaced")
                {
                    stage = "replace-whole-source";
                    Check(first == null && extension.TestSessionViewportRestoring &&
                        extension.PreviewView.IsResourceRenderingDeferred && !Visible(extension),
                        "The original viewport remains held and hidden until the source is replaced.");
                    source += "\r\rNew current source revision.";
                    editor.TestNativeCore.SetText(source);
                    // Whole-source SetText resets native selection/viewport. Keep
                    // this race's declared middle destination using production
                    // metadata restoration on the same live preview extension.
                    stage = "reapply-middle-metadata-after-source-reset";
                    editor.ResetEditorState(metadata);
                    Check(ReferenceEquals(extension, editor.TestMarkdownPreview),
                        "Reapplying middle metadata retains the same adversarial restore owner.");
                }
                if (mode != "fallback") release();
                stage = "wait-visible-release";
                await UntilAsync(() => Visible(extension) && !extension.PreviewView.IsResourceRenderingDeferred,
                    "Success, cancellation or the real Ready-started fallback completes restore ownership.", mode == "fallback" ? 8000 : 20000);
                await TabDragTestHost.FrameAsync();
                if (mode is not ("follow-off" or "fallback"))
                {
                    stage = "immutable-first-anchor-oracle";
                    Check(first != null && first.IsAvailable && !exposedWithoutAnchor,
                        "The first exposed restore has an actual presented viewport anchor.");
                }
                // Keep the immutable first-frame oracle, but request semantic
                // mapping only after optional reflows finish. Mapping deliberately
                // rejects geometry while a newer resource frame is in flight.
                stage = "wait-terminal-resources-and-current-reading";
                await TerminalAsync(extension.PreviewView, allowOffscreenUnrequestedImage: mode != "top");
                if (mode is not ("follow-off" or "fallback"))
                {
                    stage = "wait-observed-link-quiescence-before-mapping";
                    await SettledAsync(editor, extension, mode, stage, log);
                    stage = "immutable-first-source-identity-oracle";
                    var latest = extension.PreviewView.GetSelection();
                    var sameIdentity = latest.IsAvailable && extension.SourceSession.IsCurrent &&
                        first.DocumentId == extension.SourceSession.DocumentId && first.Revision == extension.SourceSession.AcceptedRevision &&
                        latest.DocumentId == first.DocumentId && latest.Revision == first.Revision && latest.TextGeneration == first.TextGeneration;
                    log.AppendLine($"OBS: ResourceDeferral {mode} firstIdentity={first.DocumentId}/{first.Revision}/{first.TextGeneration}, " +
                        $"accepted={extension.SourceSession.DocumentId}/{extension.SourceSession.AcceptedRevision}, currentSelection={latest.DocumentId}/{latest.Revision}/{latest.TextGeneration}, match={sameIdentity}.");
                    Check(sameIdentity, "The immutable first exposed frame belongs to the latest accepted source identity, never the obsolete held mapping.");
                    stage = "map-immutable-first-source-target";
                    var mapped = await extension.PreviewView.MapSourcePositionAsync(extension.SourceSession.DocumentId,
                        extension.SourceSession.AcceptedRevision, first.TextGeneration, targetByte, MarkdownPositionAffinity.After)
                        .AsTask().WaitAsync(TimeSpan.FromSeconds(15));
                    stage = "immutable-first-target-oracle";
                    Check(metadata.FirstVisibleDocumentLine == 0 || (!exposedTop && mapped.PreviewUtf16Offset > 0 &&
                        mapped.PreviewUtf16Offset >= first.ScrollStartUtf16Offset && mapped.PreviewUtf16Offset <= first.ScrollEndUtf16Offset),
                        "The first exposed middle frame contains its exact mapped source destination without a top flash.");
                }
                if (mode is not ("top" or "follow-off" or "fallback"))
                {
                    stage = "map-final-current-source-target";
                    var identity = extension.PreviewView.GetSelection();
                    LogFinal("before-map", null);
                    var mapped = await extension.PreviewView.MapSourcePositionAsync(extension.SourceSession.DocumentId,
                        extension.SourceSession.AcceptedRevision, identity.TextGeneration, targetByte, MarkdownPositionAffinity.After)
                        .AsTask().WaitAsync(TimeSpan.FromSeconds(15));
                    LogFinal("after-map-before-settling", mapped.PreviewUtf16Offset);
                    stage = "wait-observed-link-quiescence-after-mapping";
                    await SettledAsync(editor, extension, mode, stage, log);
                    var after = extension.PreviewView.GetViewportAnchor();
                    stage = "final-target-oracle";
                    LogFinal("final-fresh-anchor", mapped.PreviewUtf16Offset);
                    Check(after.IsAvailable && mapped.PreviewUtf16Offset >= after.ScrollStartUtf16Offset &&
                        mapped.PreviewUtf16Offset <= after.ScrollEndUtf16Offset && !exposedTop,
                        "The restored middle source region remains aligned after optional resources settle.");

                    void LogFinal(string moment, ulong? mappedOffset)
                    {
                        var current = extension.PreviewView.GetViewportAnchor();
                        var scroll = PreviewScroll(extension.PreviewView);
                        var sourceView = (WinUIEditor.EditorBaseControl)((Grid)editor.TestNativeCore.Content).Children[0];
                        editor.TestNativeCore.GetViewportAnchor(out var line, out var subLine);
                        log.AppendLine($"OBS: ResourceDeferral {mode} {moment} targetByte={targetByte}, mapped={mappedOffset}, exposedTop={exposedTop}; " +
                            $"anchor={current.IsAvailable}/{current.DocumentId}/{current.Revision}/{current.TextGeneration}/layout{current.LayoutGeneration}/offset{current.PreviewUtf16Offset}:{current.OffsetWithinLine:R}/span{current.ScrollStartUtf16Offset}:{current.ScrollEndUtf16Offset}:{current.ScrollProgress:R}; " +
                            $"actualY={scroll.VerticalOffset:R}, scrollable={scroll.ScrollableHeight:R}, extent={scroll.ExtentHeight:R}, viewport={scroll.ViewportHeight:R}; " +
                            $"source={line}:{subLine}:{editor.TestNativeCore.FirstVisibleRowOffset:R}, sourcePending={sourceView.IsViewportRestorePending}, sourceAnimating={sourceView.IsScrollAnimating}; link={extension.TestResourceLinkState}.");
                    }
                }
                stage = "source-undo-savepoint-invariants";
                Check(mode == "source-replaced" ? editor.GetText() == source : Stamp(editor) == original,
                    "Restoration preserves exact source/revision/Undo/savepoint except the declared owned source replacement.");
                log.AppendLine($"OBS: ResourceDeferral {mode} first={first?.PreviewUtf16Offset}, firstLayout={first?.LayoutGeneration}, final={extension.PreviewView.GetViewportAnchor().PreviewUtf16Offset}, full={extension.SourceSession.FullSubmissions}.");
            }
            catch (Exception error)
            {
                log.AppendLine($"OBS: ResourceDeferral {mode} failedStage={stage}.");
                throw new InvalidOperationException($"Resource deferral {mode} failed at stage '{stage}'.", error);
            }
            finally
            {
                release();
                Window.Current.Content = content;
                CompositionTarget.Rendering -= Rendering;
                if (editor.TestMarkdownPanelOpen) editor.ShowHideContentPreview();
                if (extension != null) await extension.DisposalCompletion.WaitAsync(TimeSpan.FromSeconds(20));
                Check(mode == "source-replaced" || Stamp(editor) == original, "Every lifecycle exit preserves source/Undo/savepoint.");
            }
        }

        private static string Stamp(Notepads.Presentation.Controls.TextEditor.TextEditor editor)
        {
            var native = editor.TestNativeCore.TestFeatureNative;
            return editor.GetText() + "|" + native.DocumentRevision + "|" + editor.TestNativeCore.ContentVersion + "|" +
                native.DocumentSequence + "|" + native.UndoSequence + "|" + native.UndoSavePoint + "|" + native.Modify + "|" + native.CanUndo() + "|" + native.CanRedo();
        }
        private static bool Visible(MarkdownExtensionView extension) => extension?.PreviewView is { IsLoaded: true, IsSurfaceReady: true, IsEnabled: true, Opacity: > 0 } &&
            extension.FindName("PreviewLoading") is Border { Visibility: Visibility.Collapsed };
        private static ScrollViewer PreviewScroll(MarkdownView view) => Descendants(view).OfType<ScrollViewer>().Single(item => item.Name == "PreviewScrollViewer");

        private readonly record struct StableViewport(Guid Document, ulong Revision, ulong Text, ulong Layout, ulong Offset,
            double Within, ulong Start, ulong End, double Progress, double ActualY, double Scrollable, double Extent, double Viewport,
            int SourceLine, int SourceSubLine, double SourceRowOffset);

        // This observes completion independently of the expected target. Four
        // distinct Rendering callbacks must see identical actual geometry while
        // the host has no mapping, restoration or animation debt. No setters run.
        private static async Task SettledAsync(Notepads.Presentation.Controls.TextEditor.TextEditor editor,
            MarkdownExtensionView extension, string mode, string phase, StringBuilder log)
        {
            var view = extension.PreviewView;
            var scroll = PreviewScroll(view);
            var complete = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
            StableViewport? previous = null;
            int consecutive = 0, callbacks = 0;
            var clock = Stopwatch.StartNew();
            var history = new Queue<string>();
            void Rendering(object sender, object args)
            {
                try
                {
                    ++callbacks;
                    var anchor = view.GetViewportAnchor();
                    var selection = view.GetSelection();
                    editor.TestNativeCore.GetViewportAnchor(out var line, out var subLine);
                    var observed = new StableViewport(anchor.DocumentId, anchor.Revision, anchor.TextGeneration, anchor.LayoutGeneration,
                        anchor.PreviewUtf16Offset, anchor.OffsetWithinLine, anchor.ScrollStartUtf16Offset, anchor.ScrollEndUtf16Offset,
                        anchor.ScrollProgress, scroll.VerticalOffset, scroll.ScrollableHeight, scroll.ExtentHeight, scroll.ViewportHeight,
                        line ?? -1, subLine, editor.TestNativeCore.FirstVisibleRowOffset);
                    var quiet = extension.TestResourceLinkQuiescent && extension.SourceSession.IsCurrent && Visible(extension) &&
                        !view.IsResourceRenderingDeferred && anchor.IsAvailable && selection.IsAvailable;
                    consecutive = quiet ? previous == observed ? consecutive + 1 : 1 : 0;
                    previous = observed;
                    history.Enqueue($"render={callbacks}, quiet={quiet}, consecutive={consecutive}, observed={observed}, link={extension.TestResourceLinkState}");
                    if (history.Count > 8) history.Dequeue();
                    if (consecutive >= 4) complete.TrySetResult(true);
                }
                catch (Exception error) { complete.TrySetException(error); }
            }
            CompositionTarget.Rendering += Rendering;
            try { await complete.Task.WaitAsync(TimeSpan.FromSeconds(15)); }
            finally
            {
                CompositionTarget.Rendering -= Rendering;
                log.AppendLine(FormattableString.Invariant($"OBS: ResourceDeferral {mode} {phase} observedSettled={complete.Task.Status}, callbacks={callbacks}, consecutive={consecutive}, elapsedMs={clock.Elapsed.TotalMilliseconds:F3}."));
                foreach (var item in history) log.AppendLine("OBS: ResourceDeferral settled " + item);
            }
        }
        private static MarkdownView CreateView(Panel root)
        {
            var view = new MarkdownView { Width = 420, Height = 300, HorizontalAlignment = HorizontalAlignment.Left, VerticalAlignment = VerticalAlignment.Top };
            root.Children.Add(view);
            return view;
        }
        private static Task LoadedAsync(MarkdownView view) => UntilAsync(() => view.IsLoaded && view.ActualWidth > 0,
            "The standalone real native Markdown view is arranged.");
        private static async Task SetAsync(MarkdownView view, string source, MarkdownExtensions extensions = Extensions)
        {
            var result = await view.SetDocumentWithExtensionsAsync(Guid.NewGuid(), 1, Encoding.UTF8.GetBytes(source), extensions)
                .AsTask().WaitAsync(TimeSpan.FromSeconds(15));
            Check(result.Status == DocumentUpdateStatus.Ready && result.Published && view.IsSurfaceReady,
                "The real native base text/navigation publishes without optional completion.");
        }
        private static async Task TurnsAsync()
        {
            await TabDragTestHost.FrameAsync();
            await Window.Current.Dispatcher.RunAsync(CoreDispatcherPriority.Low, () => { });
            await TabDragTestHost.FrameAsync();
        }
        private static async Task HeldAcrossTurnsAsync(MarkdownView view)
        {
            var before = view.GetViewportAnchor();
            bool lostReading = false;
            void Rendering(object sender, object args) => lostReading |= !view.GetSelection().IsAvailable || !view.GetViewportAnchor().IsAvailable;
            Check(before.IsAvailable && view.GetSelection().IsAvailable, "A deferred base frame has current reading/viewport identities.");
            CompositionTarget.Rendering += Rendering;
            try { await TurnsAsync(); }
            finally { CompositionTarget.Rendering -= Rendering; }
            var objects = MarkdownMermaidTests.ReadObjects(view);
            var after = view.GetViewportAnchor();
            Check(!lostReading && after.IsAvailable && before.LayoutGeneration == after.LayoutGeneration &&
                objects.Count(item => item.Family == "Formula") == 3 && objects.Count(item => item.Family == "Diagram") == 2 &&
                objects.All(item => item.State == "Pending"),
                $"Across rendering and Low-dispatch turns no derived admission invalidates the base identity or advances object state/layout. before={before.IsAvailable}/{before.TextGeneration}/{before.LayoutGeneration}, after={after.IsAvailable}/{after.TextGeneration}/{after.LayoutGeneration}, lostReading={lostReading}, deferred={view.IsResourceRenderingDeferred}, size={view.ActualWidth:R}x{view.ActualHeight:R}, objects=" + string.Join(",", objects.Select(item => item.Family + ":" + item.State)) + ", text=" + view.DisplayedText.Substring(0, Math.Min(30, view.DisplayedText.Length)));
        }
        private static Task TerminalAsync(MarkdownView view, bool allowOffscreenUnrequestedImage = false) => UntilAsync(() =>
        {
            if (!view.IsSurfaceReady || !view.GetSelection().IsAvailable || !view.GetViewportAnchor().IsAvailable) return false;
            var objects = MarkdownMermaidTests.ReadObjects(view);
            var peer = FrameworkElementAutomationPeer.FromElement(view) ?? FrameworkElementAutomationPeer.CreatePeerForElement(view);
            var images = peer?.GetChildren()?.Where(item => item.GetAutomationControlType() == AutomationControlType.Image &&
                item.GetName().StartsWith("barrier image, ", StringComparison.Ordinal)).ToArray();
            if (images is not { Length: 1 }) return false;
            var imageState = images[0].GetHelpText();
            var bounds = images[0].GetBoundingRectangle();
            // Restoring deep in the document need not admit the cold top image.
            // Visible images and the standalone decode cases still require Ready.
            bool imageReady = imageState == "Image loaded" ||
                (allowOffscreenUnrequestedImage && imageState == "Image not loaded" && (bounds.Width <= 0 || bounds.Height <= 0));
            return objects.Count(item => item.Family == "Formula") == 3 && objects.Count(item => item.Family == "Diagram") == 2 &&
                objects.All(item => item.State is not ("Pending" or "Unavailable")) && objects.Any(item => item.State == "Unsupported") &&
                objects.Any(item => item.Family == "Formula" && item.State == "Failed") && objects.Any(item => item.Family == "Diagram" && item.State == "Failed") &&
                imageReady;
        }, "Released resources reach actual Ready/Unsupported/SyntaxError/Failed outcomes, settle any visible image and expose current reading geometry.", 25000);

        [WinRT.GeneratedWinRTExposedType]
        private sealed partial class CountingImages : IMarkdownImageProvider
        {
            private readonly object _gate = new();
            private readonly Dictionary<string, int> _calls = new(StringComparer.Ordinal);
            internal int Total { get { lock (_gate) return _calls.Values.Sum(); } }
            internal int Count(string destination) { lock (_gate) return _calls.TryGetValue(destination, out var value) ? value : 0; }
            public IAsyncOperation<IRandomAccessStream> OpenAsync(MarkdownImageRequest request)
            {
                lock (_gate) _calls[request.Destination] = Count(request.Destination) + 1;
                return AsyncInfo.Run<IRandomAccessStream>(async cancellation =>
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
}

namespace Notepads.Presentation.Controls.Markdown
{
    public sealed partial class MarkdownExtensionView
    {
        internal bool TestResourceLinkQuiescent
        {
            get
            {
                var source = _linkEditor?.Content is Grid grid && grid.Children.Count > 0 ? grid.Children[0] as WinUIEditor.EditorBaseControl : null;
                return _active && !_disposed && _linkRunning && !_restoringSessionViewport && !_viewportResourceReleasePending &&
                    _linkCompletion.IsCompleted && _linkCancellation == null && !_linkCoordinator.Active.HasValue && !_linkCoordinator.HasPending &&
                    !_linkRendering && !_linkViewportResample && _linkApplying == 0 && _linkSampling == 0 &&
                    source != null && !source.IsViewportRestorePending && !source.IsScrollAnimating;
            }
        }
        internal string TestResourceLinkState =>
            $"active={_active}, disposed={_disposed}, running={_linkRunning}, restoring={_restoringSessionViewport}, releasePending={_viewportResourceReleasePending}, " +
            $"task={_linkCompletion.Status}, cts={_linkCancellation != null}, work={_linkCoordinator.Active?.Operation}, pending={_linkCoordinator.HasPending}, rendering={_linkRendering}, " +
            $"resample={_linkViewportResample}, applying={_linkApplying}, sampling={_linkSampling}, pane={_linkCoordinator.Pane}, kind={_linkCoordinator.Kind}, " +
            $"epoch={_linkCoordinator.Epoch}, geometry={_linkCoordinator.Geometry}, scrollSerial={_linkCoordinator.ScrollSerial}, targetLayout={_linkTargetLayout}, targetAvailable={_linkTargetAvailable}, " +
            $"preservedRevision={_linkPreservedViewportRevision}, restoredSource={_linkRestoredViewport?.Source.DisplayRow}:{_linkRestoredViewport?.Source.Fraction}, " +
            $"restoredPreview={_linkRestoredViewport?.Preview.PreviewUtf16Offset}:{_linkRestoredViewport?.Preview.OffsetWithinLine}/layout{_linkRestoredViewport?.Preview.LayoutGeneration}";
        private TaskCompletionSource<bool> _testViewportMapReady, _testViewportMapRelease;
        private bool _testHoldViewportMapsUntilRelease, _testPauseViewportRestoreDeadline;
        internal Task TestHoldNextSourceViewport(out Action release, bool holdUntilRelease = false,
            bool pauseRestoreDeadline = false)
        {
            if (_testViewportMapRelease != null) throw new InvalidOperationException("A source viewport map is already armed.");
            _testHoldViewportMapsUntilRelease = holdUntilRelease;
            _testPauseViewportRestoreDeadline = pauseRestoreDeadline;
            _testViewportMapReady = new(TaskCreationOptions.RunContinuationsAsynchronously);
            _testViewportMapRelease = new(TaskCreationOptions.RunContinuationsAsynchronously);
            var gate = _testViewportMapRelease;
            release = () =>
            {
                if (ReferenceEquals(_testViewportMapRelease, gate))
                    _testViewportMapReady = _testViewportMapRelease = null;
                gate.TrySetResult(true);
            };
            return _testViewportMapReady.Task;
        }
        partial void OnLinkSourceViewportMapped(MarkdownFollowWork work, ref Task publicationWait)
        {
            if (work.Operation != MarkdownFollowOperation.SourceViewport || _testViewportMapRelease == null) return;
            publicationWait = _testViewportMapRelease.Task;
            var ready = _testViewportMapReady;
            _testViewportMapReady = null;
            // Geometry can cancel the first map and submit a replacement.
            // Explicit lifecycle barriers pause the deadline; fallback keeps the real timer.
            if (_testPauseViewportRestoreDeadline) StopViewportRestoreDeadline();
            if (!_testHoldViewportMapsUntilRelease) _testViewportMapRelease = null;
            ready?.TrySetResult(true);
        }
    }
}
