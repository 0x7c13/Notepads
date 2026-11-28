// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Threading.Tasks;
using Notepads.Markdown;
using Notepads.Presentation.Controls.Markdown;
using Notepads.Presentation.Controls.TextEditor;
using Windows.Storage;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Media;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

// Opt-in timing diagnostics. The driver stages exact private input bytes beneath
// its owned test run before activation. Rendering callbacks are not GPU presents.
internal static partial class MarkdownStartupPerformanceTests
{
    private const string MiddleHeading = "06 Math";
    private const string ExpectedInputSha256 = "0A0669AC84953D4C89760FB93CD9A1486B31A8B4BA5F1A4773100BCEB12C0EBF";
    private const int BaseTimeoutMs = 30000, DerivedTimeoutMs = 30000, MaximumRows = 8192;

    internal static async Task RunAsync(StringBuilder log)
    {
        var root = ApplicationData.Current.LocalFolder.Path;
        var input = Path.Combine(root, "startup-input", "Markdown Rendering Lab.md");
        var output = Path.Combine(root, "markdown-startup-perf.json");
        var cases = new List<Case>();
        var assets = new Dictionary<string, string>(StringComparer.Ordinal);
        var originalWindow = Window.Current.Content;
        string fileSha = null, canonicalSha = null, preparationError = null;
        long inputBytes = 0;
        bool synchronousHighlights = false, viewportHighlightBaseline = false;
        try
        {
            var bytes = await File.ReadAllBytesAsync(input);
            inputBytes = bytes.Length;
            fileSha = Hash(bytes);
            // A staged Lab revision may supply its own byte manifest. The
            // original fixture still uses the compiled-in document hash.
            var manifestPath = input + ".fixture.json";
            using var manifest = File.Exists(manifestPath)
                ? JsonDocument.Parse(await File.ReadAllTextAsync(manifestPath))
                : null;
            var expectedHash = manifest == null ? ExpectedInputSha256 : manifest.RootElement.GetProperty("sha256").GetString();
            synchronousHighlights = manifest != null && manifest.RootElement.TryGetProperty("synchronousHighlights", out var sync)
                && sync.GetBoolean();
            viewportHighlightBaseline = manifest != null && manifest.RootElement.TryGetProperty("viewportHighlightBaseline", out var baseline)
                && baseline.GetBoolean();
            CoreValidationProbe.SetViewportHighlightBaseline(viewportHighlightBaseline);
            if (fileSha != expectedHash) throw new InvalidDataException("The staged Lab input does not match its expected SHA256.");
            var canonical = new UTF8Encoding(false, true).GetString(bytes).Replace("\r\n", "\r", StringComparison.Ordinal).Replace('\n', '\r');
            canonicalSha = Hash(Encoding.UTF8.GetBytes(canonical));
            var assetRoot = Path.Combine(Path.GetDirectoryName(input), "Markdown Rendering Lab Assets");
            if (Directory.Exists(assetRoot))
                foreach (var path in Directory.EnumerateFiles(assetRoot, "*", SearchOption.AllDirectories).OrderBy(path => path, StringComparer.Ordinal))
                    assets.Add(Path.GetRelativePath(assetRoot, path).Replace('\\', '/'), Hash(await File.ReadAllBytesAsync(path)));
            if (manifest == null)
            {
                if (assets.Count != 6) throw new InvalidDataException("Stage exactly the six original sibling Lab assets.");
            }
            else
            {
                var expectedAssets = manifest.RootElement.GetProperty("assetSha256");
                if (expectedAssets.EnumerateObject().Count() != assets.Count || assets.Any(asset =>
                    !expectedAssets.TryGetProperty(asset.Key, out var expected) || asset.Value != expected.GetString()))
                    throw new InvalidDataException("The staged Lab assets do not match the manifest names and SHA256 hashes.");
            }

            await using var host = await FeatureTestHost.CreateAsync();
            var file = await StorageFile.GetFileFromPathAsync(input);
            var openWatch = Stopwatch.StartNew();
            if (!await host.Page.OpenFileAsync(file)) throw new InvalidOperationException("The staged Lab file did not open through the production workspace.");
            var editor = (TextEditor)host.Core.GetSelectedReadyEditor();
            await editor.TestStopFileChecksAsync();
            await UntilAsync(() => editor.TestNativeCore.IsLoaded && editor.TestNativeCore.ActualWidth > 0,
                "The staged Lab source editor is arranged before preview timing.");
            openWatch.Stop();
            if (editor.GetText() != canonical) throw new InvalidDataException("The loaded source differs from exact canonical-CR input.");
            var original = SourceState(editor);
            var metadata = editor.GetTextEditorStateMetaData();
            metadata.IsContentPreviewPanelOpened = true;
            var midLine = Array.IndexOf(canonical.Split('\r'), "## " + MiddleHeading);
            if (midLine <= 0) throw new InvalidDataException("The exact Lab Math heading was not found.");

            MarkdownExtensionView previousClosed = null;
            foreach (var mode in new[] { "normal-first-renderer-use", "normal-process-warm-new-extension",
                "normal-warm-reopen-same-editor", "metadata-restore-same-editor", "metadata-restore-mid-document" })
            {
                var warmReopen = mode == "normal-warm-reopen-same-editor";
                if (!warmReopen)
                {
                    editor.TestEvictMarkdownReopenEntry();
                    if (previousClosed != null)
                        await previousClosed.DisposalCompletion.WaitAsync(TimeSpan.FromSeconds(20));
                }
                var measured = new Case { Name = mode, SourceOpenMs = openWatch.Elapsed.TotalMilliseconds, Before = SourceState(editor) };
                var previousSession = warmReopen ? previousClosed?.SourceSession : null;
                measured.FullBefore = previousSession?.FullSubmissions ?? 0;
                measured.DeltaBefore = previousSession?.DeltaSubmissions ?? 0;
                measured.CopiedBefore = previousSession?.CopiedBytes ?? 0;
                var restoration = mode.StartsWith("metadata-restore", StringComparison.Ordinal);
                var requested = metadata.Clone();
                if (mode == "metadata-restore-mid-document")
                {
                    requested.FirstVisibleDocumentLine = midLine;
                    requested.FirstVisibleSubLine = 0;
                    requested.FirstVisibleRowOffset = 0;
                    requested.ScrollViewerHorizontalOffset = requested.ScrollViewerVerticalOffset = 0;
                }
                if (restoration)
                {
                    measured.TargetSourceLine = requested.FirstVisibleDocumentLine ?? 0;
                    measured.TargetSourceSubLine = requested.FirstVisibleSubLine;
                    measured.TargetSourceRowOffset = requested.FirstVisibleRowOffset;
                    measured.TargetSourceByte = checked((ulong)editor.TestNativeCore.TestFeatureNative.PositionFromLine(measured.TargetSourceLine));
                }
                cases.Add(measured);
                using (var observer = new Observer(editor, measured))
                {
                    CoreValidationProbe.StartPreviewStartupTrace(synchronousHighlights);
                    try
                    {
                        observer.Start();
                        if (restoration) editor.ResetEditorState(requested);
                        else editor.ShowHideContentPreview();
                        measured.ActionReturnQpc = Stopwatch.GetTimestamp();
                        observer.BindAndObserve("action-return");
                        await observer.WaitAsync();
                        var view = editor.TestMarkdownPreview.PreviewView;
                        await UntilAsync(() => view.GetViewportAnchor().IsAvailable && !CoreValidationProbe.StartupHighlightsPending(view),
                            "Startup syntax highlighting has settled and the current preview geometry is available.", DerivedTimeoutMs);
                        if (restoration) await VerifyRestoredFirstFrameAsync(editor, measured);
                    }
                    catch (Exception error) { measured.Error = error.ToString(); }
                    finally
                    {
                        try { observer.Stop(); }
                        finally { measured.NativePhases = CoreValidationProbe.StopPreviewStartupTrace(); }
                    }
                }
                var activeExtension = editor.TestMarkdownPreview;
                var activeSession = activeExtension?.SourceSession;
                measured.ReusedOwner = ReferenceEquals(previousClosed, activeExtension);
                measured.FullSubmissionDelta = (activeSession?.FullSubmissions ?? 0) - measured.FullBefore;
                measured.DeltaSubmissionDelta = (activeSession?.DeltaSubmissions ?? 0) - measured.DeltaBefore;
                measured.CopiedByteDelta = (activeSession?.CopiedBytes ?? 0) - measured.CopiedBefore;
                if (warmReopen && (!measured.ReusedOwner || measured.FullSubmissionDelta != 0 ||
                    measured.DeltaSubmissionDelta != 0 || measured.CopiedByteDelta != 0 ||
                    HasNativeSubmission(measured.NativePhases)))
                    measured.Error = Append(measured.Error, "Warm reopen did not reuse its unchanged model without source copying/submission/parse.");
                measured.After = SourceState(editor);
                measured.SourceUnchanged = measured.Before == measured.After && measured.After == original && editor.GetText() == canonical;
                if (!measured.SourceUnchanged) measured.Error = Append(measured.Error, "Source text/revision/content version/undo/savepoint changed during preview.");
                if (measured.ObservationErrors.Count != 0 || measured.RowsOverflow)
                    measured.Error = Append(measured.Error, "The bounded observer reported errors or overflow; inspect its raw evidence.");
                WriteReport();
                log.AppendLine($"OBS: Markdown.Startup {mode} baseVisibleMs={Milliseconds(measured.StartQpc, measured.FirstVisibleRenderQpc):F3}, nativeSurfaceMs={Milliseconds(measured.StartQpc, measured.FirstNativeSurfaceQpc):F3}, derivedTerminalMs={Milliseconds(measured.StartQpc, measured.FirstDerivedTerminalQpc):F3}, maxRenderingGapMs={measured.MaxRenderingGapTicks * 1000.0 / Stopwatch.Frequency:F3}, full={measured.Rows.LastOrDefault()?.Full ?? 0}, delta={measured.Rows.LastOrDefault()?.Delta ?? 0}, unchanged={measured.SourceUnchanged}, error={measured.Error ?? "none"}.");
                var extension = editor.TestMarkdownPreview;
                if (editor.TestMarkdownPanelOpen) editor.ShowHideContentPreview();
                if (extension != null) await extension.SuspensionCompletion.WaitAsync(TimeSpan.FromSeconds(20));
                measured.RetainedForReopen = editor.TestHasMarkdownReopenEntry;
                if (measured.RetainedForReopen)
                    measured.RetainedBytes = extension.PreviewView.GetRetainedByteEstimate();
                previousClosed = extension;
                WriteReport();
            }
            if (Hash(await File.ReadAllBytesAsync(input)) != fileSha) throw new InvalidOperationException("The staged input file changed.");
        }
        catch (Exception error) { preparationError = error.ToString(); }
        finally
        {
            CoreValidationProbe.SetViewportHighlightBaseline(false);
            Window.Current.Content = originalWindow;
            WriteReport();
        }
        if (preparationError != null || cases.Count != 5 || cases.Any(item => item.Error != null))
            throw new InvalidOperationException("Markdown startup diagnostics incomplete; partial evidence: " + output + ". " + preparationError);
        log.AppendLine("PASS: Markdown.Startup diagnostic observations completed without a performance threshold; report=" + output);

        void WriteReport()
        {
            using var stream = new MemoryStream();
            using (var json = new Utf8JsonWriter(stream, new JsonWriterOptions { Indented = true }))
            {
                json.WriteStartObject();
                json.WriteNumber("schema", 4); json.WriteString("method", "markdown-startup-rendering-observer-v4");
                json.WriteBoolean("synchronousHighlights", synchronousHighlights);
                json.WriteBoolean("viewportHighlightBaseline", viewportHighlightBaseline);
                json.WriteString("scope", "Same process, one source editor; first renderer use, forced fresh extension, unchanged model reopen, metadata restore at captured top and exact Math-heading source viewport. No cold disk or GPU-present claim.");
                json.WriteString("visiblePredicate", "Rendering callback: loaded && surfaceReady && displayedCurrent && opacity > 0 && enabled && !loading; source-current and viewport-anchor availability are separate observations.");
                json.WriteString("input", input); json.WriteNumber("inputBytes", inputBytes); json.WriteString("fileSha256", fileSha);
                json.WriteString("canonicalCrUtf8Sha256", canonicalSha); json.WriteNumber("frequency", Stopwatch.Frequency);
                json.WriteNumber("baseTimeoutMs", BaseTimeoutMs); json.WriteNumber("derivedTimeoutMs", DerivedTimeoutMs);
                json.WriteString("error", preparationError);
                json.WriteStartObject("assetSha256"); foreach (var pair in assets) json.WriteString(pair.Key, pair.Value); json.WriteEndObject();
                json.WriteStartArray("cases"); foreach (var item in cases) WriteCase(json, item); json.WriteEndArray();
                json.WriteEndObject();
            }
            File.WriteAllBytes(output + ".tmp", stream.ToArray());
            File.Move(output + ".tmp", output, true);
        }
    }

    private static string Hash(byte[] bytes) => Convert.ToHexString(SHA256.HashData(bytes));
    private static string Append(string first, string next) => first == null ? next : first + "\n" + next;
    private static double Milliseconds(long start, long end) => end == 0 ? -1 : (end - start) * 1000.0 / Stopwatch.Frequency;

    private static string SourceState(TextEditor editor)
    {
        var core = editor.TestNativeCore;
        var native = core.TestFeatureNative;
        return Hash(Encoding.UTF8.GetBytes(editor.GetText())) + "|" + core.ContentVersion + "|" + native.DocumentRevision + "|" +
            native.DocumentSequence + "|" + native.UndoSequence + "|" + native.UndoSavePoint + "|" + native.Modify + "|" + native.CanUndo() + "|" + native.CanRedo() + "|" + editor.IsModified;
    }

    private static async Task VerifyRestoredFirstFrameAsync(TextEditor editor, Case measured)
    {
        var first = measured.FirstVisible;
        if (first == null || !first.Anchor || first.SourceViewportPending || first.SourceLine != measured.TargetSourceLine ||
            first.SourceSubLine != measured.TargetSourceSubLine || Math.Abs(first.SourceRowOffset - measured.TargetSourceRowOffset) > 0.5)
            throw new InvalidOperationException("The first exposed restored frame lacks the exact settled requested source viewport or a presented anchor.");
        var extension = editor.TestMarkdownPreview;
        if (measured.TargetSourceLine > 0)
        {
            var view = extension.PreviewView;
            var document = extension.SourceSession.DocumentId;
            var revision = extension.SourceSession.AcceptedRevision;
            var point = await view.MapSourcePositionAsync(document, revision, first.Text, measured.TargetSourceByte,
                MarkdownPositionAffinity.After);
            measured.PointPreviewOffset = point.PreviewUtf16Offset;
            var sourceEnd = measured.TargetSourceByte + (ulong)Encoding.UTF8.GetByteCount("## " + MiddleHeading);
            var range = await view.MapSourceSpanAsync(document, revision, first.Text, measured.TargetSourceByte, sourceEnd,
                MarkdownPositionAffinity.After);
            measured.TargetPreviewOffset = range.Start.PreviewUtf16Offset;
            // Record the inverse top span separately. The geometry below is
            // taken from the exact first exposed frame.
            if (measured.TargetPreviewOffset == 0 || measured.TargetPreviewOffset < first.ScrollStart || measured.TargetPreviewOffset > first.ScrollEnd)
                measured.TargetOutsideTopFollowSpan = true;
            first.Heading = SelectMappedHeading(first.Heading, measured.TargetPreviewOffset);
            measured.SettledHeading = SelectMappedHeading(
                CoreValidationProbe.GetStartupHeadingGeometry(extension.PreviewView, MiddleHeading), measured.TargetPreviewOffset);
            if (range.IsDiscontinuous || !HeadingAtViewportTop(first.Heading) || !HeadingAtViewportTop(measured.SettledHeading))
                throw new InvalidOperationException("The mapped heading is missing from the viewport top or clipped during restoration.");
            if (measured.Rows.Any(row => row.Reason == "rendering" && row.Loaded && row.Surface && row.DisplayedCurrent &&
                row.Opacity > 0 && row.Enabled && !row.Loading && row.Anchor && row.PreviewOffset == 0 && row.ScrollStart == 0))
                throw new InvalidOperationException("A restored mid-document preview exposed the top before or after its requested destination.");
        }
        measured.RestoredFirstFrameVerified = true;
    }

    private static double[] SelectMappedHeading(double[] samples, ulong target)
    {
        const int stride = 8;
        if (samples == null) return null;
        for (int index = 0; index + stride <= samples.Length; index += stride)
            if (target >= samples[index + 6] && target <= samples[index + 7])
                return samples[index..(index + stride)];
        return null;
    }

    private static bool HasNativeSubmission(ulong[] samples)
    {
        if (samples == null) return false;
        for (var index = 0; index + 2 < samples.Length; index += 3)
            if (samples[index] is 0 or 2) return true; // Submit / ParseObserved.
        return false;
    }

    private static bool HeadingAtViewportTop(double[] heading) =>
        heading is { Length: 8 } && heading[5] > 0 && Math.Abs(heading[3]) <= 1 &&
        heading[4] > heading[3] && heading[4] <= heading[1] + 1;

    private sealed partial class Observer : IDisposable
    {
        private readonly TextEditor _editor;
        private readonly Case _case;
        private readonly WinUIEditor.EditorBaseControl _source;
        private MarkdownSourceSession _session;
        private long _lastRender;
        private bool _running;

        internal Observer(TextEditor editor, Case measured)
        {
            _editor = editor; _case = measured;
            _source = (WinUIEditor.EditorBaseControl)((Grid)editor.TestNativeCore.Content).Children[0];
        }
        internal void Start()
        {
            _case.StartQpc = _lastRender = Stopwatch.GetTimestamp();
            _running = true;
            CompositionTarget.Rendering += Rendering;
        }
        internal void BindAndObserve(string reason)
        {
            var session = _editor.TestMarkdownPreview?.SourceSession;
            if (session != _session)
            {
                if (_session != null) _session.Changed -= Changed;
                _session = session;
                if (_session != null) _session.Changed += Changed;
            }
            Observe(reason, false);
        }
        private void Changed(object sender, EventArgs args) => Observe("session-changed", false);
        private void Rendering(object sender, object args)
        {
            var qpc = Stopwatch.GetTimestamp();
            _case.MaxRenderingGapTicks = Math.Max(_case.MaxRenderingGapTicks, qpc - _lastRender);
            _lastRender = qpc; ++_case.RenderingCallbacks;
            Observe("rendering", true);
        }
        private void Observe(string reason, bool rendering)
        {
            if (!_running) return;
            try
            {
                var qpc = Stopwatch.GetTimestamp();
                var extension = _editor.TestMarkdownPreview;
                var session = extension?.SourceSession;
                var view = extension?.PreviewView;
                var anchor = view?.GetViewportAnchor();
                var current = session?.IsCurrent == true;
                var surface = view?.IsSurfaceReady == true;
                var displayed = session != null && view != null && view.DisplayedDocumentId == session.DocumentId && view.DisplayedRevision == session.AcceptedRevision;
                var loading = extension?.FindName("PreviewLoading") as Border;
                _editor.TestNativeCore.GetViewportAnchor(out var sourceLine, out var sourceSubLine);
                var row = new Row
                {
                    Qpc = qpc, Reason = reason, Current = current, Surface = surface, DisplayedCurrent = displayed,
                    Status = session?.Status.ToString() ?? "NoSession", Full = session?.FullSubmissions ?? 0, Delta = session?.DeltaSubmissions ?? 0,
                    Copied = session?.CopiedBytes ?? 0, Accepted = session?.AcceptedRevision ?? 0, Displayed = view?.DisplayedRevision ?? 0,
                    Text = anchor?.TextGeneration ?? 0, Layout = anchor?.LayoutGeneration ?? 0, Anchor = anchor?.IsAvailable == true,
                    PreviewOffset = anchor?.PreviewUtf16Offset ?? 0, ScrollStart = anchor?.ScrollStartUtf16Offset ?? 0,
                    ScrollEnd = anchor?.ScrollEndUtf16Offset ?? 0, ScrollProgress = anchor?.ScrollProgress ?? 0,
                    SourceLine = sourceLine ?? -1, SourceSubLine = sourceSubLine, SourceRowOffset = _editor.TestNativeCore.FirstVisibleRowOffset,
                    SourceViewportPending = _source.IsViewportRestorePending,
                    Loading = loading?.Visibility == Visibility.Visible, Opacity = view?.Opacity ?? 0,
                    Loaded = view?.IsLoaded == true, Enabled = view?.IsEnabled == true, Width = view?.ActualWidth ?? 0, Height = view?.ActualHeight ?? 0,
                    RenderingError = view?.RenderingError, LinkBusy = extension?.TestSourceFindBusy == true
                };
                if (current && _case.FirstSourceCurrentQpc == 0) _case.FirstSourceCurrentQpc = row.Qpc;
                if (surface && displayed && _case.FirstNativeSurfaceQpc == 0) _case.FirstNativeSurfaceQpc = row.Qpc;
                if (row.Anchor && displayed && _case.FirstAnchorAvailableQpc == 0) _case.FirstAnchorAvailableQpc = row.Qpc;
                if (rendering && surface && displayed && row.Loaded && row.Opacity > 0 && row.Enabled && !row.Loading && _case.FirstVisibleRenderQpc == 0)
                {
                    _case.FirstVisibleRenderQpc = row.Qpc;
                    _case.FirstVisible = row;
                    if (_case.TargetSourceLine > 0)
                        row.Heading = CoreValidationProbe.GetStartupHeadingGeometry(view, MiddleHeading);
                }
                row.EndQpc = Stopwatch.GetTimestamp();
                if (_case.Rows.Count < MaximumRows) _case.Rows.Add(row); else _case.RowsOverflow = true;
            }
            catch (Exception error)
            {
                if (_case.ObservationErrors.Count < 8) _case.ObservationErrors.Add(error.ToString());
            }
        }
        internal async Task WaitAsync()
        {
            while (_case.FirstVisibleRenderQpc == 0 && Stopwatch.GetElapsedTime(_case.StartQpc).TotalMilliseconds < BaseTimeoutMs)
                await Task.Delay(25);
            if (_case.FirstVisibleRenderQpc == 0) throw new TimeoutException("No visible current native surface within the bounded base observation.");
            var derivedStart = Stopwatch.GetTimestamp();
            var consecutiveTerminal = 0;
            while (Stopwatch.GetElapsedTime(derivedStart).TotalMilliseconds < DerivedTimeoutMs)
            {
                var sample = ReadDerived(_editor.TestMarkdownPreview.PreviewView);
                _case.Derived.Add(sample);
                if (sample.Formulas > 0 && sample.Diagrams > 0 && sample.PendingDerived == 0 && sample.UnknownDerived == 0)
                    if (_case.FirstDerivedTerminalQpc == 0) _case.FirstDerivedTerminalQpc = sample.Qpc;
                if (sample.Images > 0 && sample.PendingImages == 0 && sample.UnknownImages == 0)
                    if (_case.FirstImagesTerminalQpc == 0) _case.FirstImagesTerminalQpc = sample.Qpc;
                var terminal = sample.Formulas > 0 && sample.Diagrams > 0 && sample.Images > 0 && sample.PendingDerived == 0 &&
                    sample.UnknownDerived == 0 && sample.PendingImages == 0 && sample.UnknownImages == 0;
                consecutiveTerminal = terminal ? consecutiveTerminal + 1 : 0;
                if (consecutiveTerminal == 2) return;
                await Task.Delay(100);
            }
            throw new TimeoutException("Derived/image objects did not reach observed terminal states within the separate bounded observation.");
        }
        internal void Stop()
        {
            if (!_running) return;
            Observe("stop", false);
            _case.StopQpc = Stopwatch.GetTimestamp();
            _running = false;
            CompositionTarget.Rendering -= Rendering;
            if (_session != null) _session.Changed -= Changed;
        }
        public void Dispose() => Stop();
    }

    private static Derived ReadDerived(Notepads.Markdown.MarkdownView view)
    {
        var result = new Derived { Qpc = Stopwatch.GetTimestamp() };
        var peer = FrameworkElementAutomationPeer.FromElement(view) ?? FrameworkElementAutomationPeer.CreatePeerForElement(view);
        foreach (var child in peer?.GetChildren() ?? Array.Empty<AutomationPeer>())
        {
            var name = child.GetName();
            var formula = name.StartsWith("Formula:", StringComparison.Ordinal);
            var diagram = name.StartsWith("Diagram:", StringComparison.Ordinal);
            var image = !diagram && child.GetAutomationControlType() == AutomationControlType.Image;
            if (!formula && !diagram && !image) continue;
            var help = child.GetHelpText();
            var family = formula ? "Formula" : diagram ? "Diagram" : "Image";
            if (formula) ++result.Formulas;
            if (diagram) ++result.Diagrams;
            if (image) ++result.Images;
            if (image)
            {
                if (help.Contains("loading", StringComparison.OrdinalIgnoreCase)) ++result.PendingImages;
                else if (string.IsNullOrEmpty(help)) ++result.UnknownImages;
            }
            else
            {
                if (help.Contains(" loading.", StringComparison.Ordinal)) ++result.PendingDerived;
                else if (string.IsNullOrEmpty(help) || help.Contains("rendering unavailable", StringComparison.Ordinal)) ++result.UnknownDerived;
            }
            var key = family + ": " + help;
            result.States[key] = result.States.TryGetValue(key, out var count) ? count + 1 : 1;
        }
        result.EndQpc = Stopwatch.GetTimestamp();
        return result;
    }

    private sealed class Case
    {
        internal string Name, Error, Before, After;
        internal double SourceOpenMs;
        internal bool SourceUnchanged, RowsOverflow, RestoredFirstFrameVerified;
        internal bool ReusedOwner, RetainedForReopen;
        internal long FullBefore, DeltaBefore, CopiedBefore, FullSubmissionDelta, DeltaSubmissionDelta, CopiedByteDelta;
        internal ulong RetainedBytes;
        internal int TargetSourceLine = -1, TargetSourceSubLine;
        internal double TargetSourceRowOffset;
        internal ulong TargetSourceByte, TargetPreviewOffset, PointPreviewOffset;
        internal bool TargetOutsideTopFollowSpan;
        internal double[] SettledHeading;
        internal ulong[] NativePhases;
        internal Row FirstVisible;
        internal long StartQpc, ActionReturnQpc, StopQpc, FirstSourceCurrentQpc, FirstNativeSurfaceQpc, FirstVisibleRenderQpc,
            FirstAnchorAvailableQpc, FirstDerivedTerminalQpc, FirstImagesTerminalQpc, RenderingCallbacks, MaxRenderingGapTicks;
        internal readonly List<Row> Rows = new();
        internal readonly List<Derived> Derived = new();
        internal readonly List<string> ObservationErrors = new();
    }
    private sealed class Row
    {
        internal long Qpc, EndQpc, Full, Delta, Copied;
        internal ulong Accepted, Displayed, Text, Layout, PreviewOffset, ScrollStart, ScrollEnd;
        internal int SourceLine, SourceSubLine;
        internal string Reason, Status, RenderingError;
        internal bool Current, Surface, DisplayedCurrent, Loading, Anchor, Loaded, Enabled, LinkBusy, SourceViewportPending;
        internal double Opacity, Width, Height, ScrollProgress, SourceRowOffset;
        internal double[] Heading;
    }
    private sealed class Derived
    {
        internal long Qpc, EndQpc;
        internal int Formulas, Diagrams, Images, PendingDerived, UnknownDerived, PendingImages, UnknownImages;
        internal readonly Dictionary<string, int> States = new(StringComparer.Ordinal);
    }

    private static void WriteHeading(Utf8JsonWriter json, string name, double[] values)
    {
        if (values is not { Length: 8 }) return;
        json.WriteStartObject(name);
        json.WriteNumber("viewportY", values[0]);
        json.WriteNumber("height", values[1]);
        json.WriteNumber("headingBlockTop", values[2]);
        json.WriteNumber("headingTextTop", values[3]);
        json.WriteNumber("headingTextBottom", values[4]);
        json.WriteNumber("rectangles", values[5]);
        json.WriteNumber("headingStart", values[6]);
        json.WriteNumber("headingEnd", values[7]);
        json.WriteEndObject();
    }

    internal static void WriteNativePhases(Utf8JsonWriter json, ulong[] samples)
    {
        string[] names =
        [
            "submit", "submitted", "parse-observed", "layout-enqueued", "worker-started", "reading-main", "reading-main-ready",
            "highlights", "highlights-ready", "marker-probe", "marker-probe-ready", "measure", "measure-ready", "prefix-frame",
            "final-frame", "final-frame-ready", "draw", "draw-ready", "published",
            "async-highlights", "async-highlights-ready", "async-highlights-applied",
            "text-formats-created", "text-formats-reused", "derived-offscreen-batch", "initial-viewport-device-loss",
            "ui-install", "ui-install-end", "ui-start-layout", "ui-start-layout-end",
            "ui-layout-updated", "ui-layout-updated-end", "ui-view-changed", "ui-view-changed-end",
            "ui-draw", "ui-draw-end", "ui-image-refresh", "ui-image-refresh-end",
            "ui-derived-refresh", "ui-derived-refresh-end", "ui-scroll-progress", "ui-scroll-progress-end", "trace-dropped",
            "ui-dispatch-viewport", "ui-dispatch-viewport-end",
            "ui-resource-admission", "ui-resource-admission-end",
            "ui-image-complete", "ui-image-complete-end",
            "ui-image-animation", "ui-image-animation-end",
            "ui-notify-published", "ui-notify-published-end",
            "ui-raise-viewport", "ui-raise-viewport-end",
            "ui-update-notes", "ui-update-notes-end",
            "ui-raster-prepare", "ui-raster-prepare-end",
            "ui-raster-surface", "ui-raster-surface-end",
            "ui-raster-paint", "ui-raster-paint-end",
            "ui-raster-commit", "ui-raster-commit-end",
            "ui-raster-attach", "ui-raster-attach-end",
            "ui-raster-recovery", "ui-raster-recovery-end",
            "draw-slow-block-node",
            "draw-slow-block-kind",
            "draw-slow-block-rows",
            "draw-slow-block-texts",
            "draw-slow-block-ticks",
            "draw-cache-prepare-ticks",
            "draw-cache-hits",
            "draw-cache-builds",
            "draw-cache-uploads",
            "draw-cache-failures",
            "ui-diagram-create", "ui-diagram-create-end",
            "ui-diagram-paint", "ui-diagram-paint-end",
            "diagram-cache-miss",
            "ui-raster-surface-native-release",
            "ui-raster-surface-native-release-end",
            "ui-raster-surface-create-replace",
            "ui-raster-surface-create-replace-end",
            "ui-raster-surface-interface-copy",
            "ui-raster-surface-interface-copy-end",
            "ui-raster-surface-set-device",
            "ui-raster-surface-set-device-end",
            "ui-raster-surface-begin-draw",
            "ui-raster-surface-begin-draw-end",
            "surface-spare-reused", "surface-spare-bytes",
            "draw-slow-block-ordinary-setup-ticks",
            "draw-slow-block-ordinary-direct-ticks",
            "draw-slow-block-ordinary-cache-ticks",
            "draw-slow-block-ordinary-setup-calls",
            "draw-slow-block-ordinary-direct-calls",
            "draw-slow-block-ordinary-cache-calls",
            "draw-slow-block-ordinary-valid",
            "surface-spare-reasons", "surface-spare-requested-size", "surface-spare-borrowed-size", "surface-spare-donor-size",
            "draw-slow-block-cache-take-ticks",
            "draw-slow-block-cache-build-ticks",
            "draw-slow-block-cache-upload-ticks",
            "draw-slow-block-cache-replay-ticks",
            "draw-slow-block-cache-take-calls",
            "draw-slow-block-cache-build-calls",
            "draw-slow-block-cache-upload-calls",
            "draw-slow-block-cache-replay-calls",
            "draw-slow-block-cache-valid",
            "draw-slow-block-pretext-ticks",
            "draw-slow-block-utf16-units",
            "draw-slow-block-code-lines",
            "draw-slow-block-highlight-spans",
            "draw-slow-block-content-flags",
            "draw-slow-block-ordinary-direct-cpu-100ns",
            "draw-slow-block-ordinary-direct-cpu-calls",
            "draw-slow-block-ordinary-direct-calls-proven-no-counted-prewarm",
            "draw-prewarm-target-observations",
            "draw-prewarm-target-omitted",
            "draw-prewarm-target-duplicates",
            "draw-prewarm-target-flags",
            "draw-prewarm-target-node",
            "draw-prewarm-target-text-index",
            "draw-prewarm-target-layout-generation",
            "draw-prewarm-target-model-revision",
            "draw-prewarm-target-model-generation",
            "draw-prewarm-target-logical-start",
            "draw-prewarm-target-logical-end",
            "draw-prewarm-target-store-flags",
            "draw-prewarm-target-occupancy",
            "draw-prewarm-target-matched-slots",
            "draw-prewarm-target-item-flags",
            "draw-prewarm-target-item-key-comparison",
            "draw-prewarm-target-ready-key-comparison",
            "draw-prewarm-target-store-epoch",
            "draw-prewarm-target-geometry-version",
            "draw-table-bitmap-replay-calls",
            "draw-table-bitmap-replay-accepted",
            "draw-table-bitmap-replay-flags",
            "draw-table-bitmap-replay-max-ticks",
            "draw-table-bitmap-replay-prefix-ticks",
            "draw-table-bitmap-replay-suffix-ticks",
            "draw-table-bitmap-replay-text-index",
            "draw-table-bitmap-replay-logical-start",
            "draw-table-bitmap-replay-logical-end",
            "draw-table-bitmap-replay-ordinal",
            "draw-table-bitmap-replay-source",
            "draw-table-bitmap-replay-start-qpc",
            "draw-table-bitmap-replay-midpoint-qpc",
            "draw-table-bitmap-replay-end-qpc",
            "draw-table-bitmap-replay-api-end-qpc",
            "draw-table-bitmap-replay-draw-ticks",
            "draw-table-bitmap-replay-tail-ticks",
            "draw-table-bitmap-replay-cpu-100ns",
            "draw-table-bitmap-replay-cpu-accepted",
            "draw-table-bitmap-replay-proven-no-counted-prewarm"
        ];
        json.WriteStartArray("nativePhases");
        if (samples != null)
            for (var index = 0; index < samples.Length; index += 3)
            {
                json.WriteStartObject();
                var phase = names[checked((int)samples[index])];
                json.WriteString("phase", phase);
                json.WriteNumber(phase is "diagram-cache-miss" || phase.StartsWith("surface-spare-", StringComparison.Ordinal) || phase.StartsWith("draw-slow-block-", StringComparison.Ordinal) || phase.StartsWith("draw-cache-", StringComparison.Ordinal) || phase.StartsWith("draw-prewarm-target-", StringComparison.Ordinal) || phase.StartsWith("draw-table-bitmap-replay-", StringComparison.Ordinal) ? "value" :
                    phase is "text-formats-created" or "text-formats-reused" or "trace-dropped" ? "count" : "layout", samples[index + 1]);
                json.WriteNumber("qpc", samples[index + 2]);
                json.WriteEndObject();
            }
        json.WriteEndArray();
    }

    private static void WriteCase(Utf8JsonWriter json, Case item)
    {
        json.WriteStartObject(); json.WriteString("name", item.Name); json.WriteString("error", item.Error);
        json.WriteBoolean("reusedOwner", item.ReusedOwner); json.WriteBoolean("retainedForReopen", item.RetainedForReopen);
        json.WriteNumber("retainedBytes", item.RetainedBytes);
        json.WriteNumber("fullSubmissionsBefore", item.FullBefore); json.WriteNumber("deltaSubmissionsBefore", item.DeltaBefore);
        json.WriteNumber("copiedBytesBefore", item.CopiedBefore); json.WriteNumber("fullSubmissionDelta", item.FullSubmissionDelta);
        json.WriteNumber("deltaSubmissionDelta", item.DeltaSubmissionDelta); json.WriteNumber("copiedByteDelta", item.CopiedByteDelta);
        json.WriteString("sourceBefore", item.Before); json.WriteString("sourceAfter", item.After); json.WriteBoolean("sourceUnchanged", item.SourceUnchanged);
        json.WriteNumber("sourceOpenPreparationMs", item.SourceOpenMs); json.WriteNumber("startQpc", item.StartQpc);
        json.WriteNumber("actionReturnQpc", item.ActionReturnQpc); json.WriteNumber("stopQpc", item.StopQpc);
        json.WriteNumber("firstSourceCurrentQpc", item.FirstSourceCurrentQpc); json.WriteNumber("firstNativeSurfaceReadyQpc", item.FirstNativeSurfaceQpc);
        json.WriteNumber("firstSkeletonHiddenVisiblePaintQpc", item.FirstVisibleRenderQpc); json.WriteNumber("firstDerivedTerminalQpc", item.FirstDerivedTerminalQpc);
        json.WriteNumber("firstAnchorAvailableQpc", item.FirstAnchorAvailableQpc);
        json.WriteNumber("targetSourceLine", item.TargetSourceLine); json.WriteNumber("targetSourceSubLine", item.TargetSourceSubLine);
        json.WriteNumber("targetSourceRowOffset", item.TargetSourceRowOffset); json.WriteNumber("targetSourceByte", item.TargetSourceByte);
        json.WriteNumber("targetPreviewOffset", item.TargetPreviewOffset);
        json.WriteNumber("pointPreviewOffset", item.PointPreviewOffset);
        json.WriteBoolean("restoredFirstFrameVerified", item.RestoredFirstFrameVerified);
        json.WriteBoolean("targetOutsideTopFollowSpan", item.TargetOutsideTopFollowSpan);
        WriteHeading(json, "settledHeading", item.SettledHeading);
        WriteNativePhases(json, item.NativePhases);
        if (item.FirstVisible is { } first)
        {
            json.WriteStartObject("firstVisibleViewport"); json.WriteBoolean("anchorAvailable", first.Anchor);
            json.WriteNumber("previewOffset", first.PreviewOffset); json.WriteNumber("scrollStart", first.ScrollStart);
            json.WriteNumber("scrollEnd", first.ScrollEnd); json.WriteNumber("scrollProgress", first.ScrollProgress);
            json.WriteNumber("sourceLine", first.SourceLine); json.WriteNumber("sourceSubLine", first.SourceSubLine);
            json.WriteNumber("sourceRowOffset", first.SourceRowOffset); json.WriteBoolean("sourceRestorePending", first.SourceViewportPending);
            WriteHeading(json, "heading", first.Heading);
            json.WriteEndObject();
        }
        json.WriteNumber("firstImagesTerminalQpc", item.FirstImagesTerminalQpc); json.WriteNumber("renderingCallbacks", item.RenderingCallbacks);
        json.WriteNumber("maxRenderingGapTicks", item.MaxRenderingGapTicks); json.WriteBoolean("rowsOverflow", item.RowsOverflow);
        json.WriteStartArray("observationErrors"); foreach (var error in item.ObservationErrors) json.WriteStringValue(error); json.WriteEndArray();
        json.WriteStartArray("rows");
        foreach (var row in item.Rows)
        {
            json.WriteStartObject(); json.WriteNumber("qpc", row.Qpc); json.WriteNumber("endQpc", row.EndQpc); json.WriteString("reason", row.Reason);
            json.WriteString("status", row.Status); json.WriteBoolean("current", row.Current); json.WriteBoolean("surfaceReady", row.Surface);
            json.WriteBoolean("displayedCurrent", row.DisplayedCurrent); json.WriteBoolean("loading", row.Loading); json.WriteBoolean("anchor", row.Anchor);
            json.WriteBoolean("loaded", row.Loaded); json.WriteBoolean("enabled", row.Enabled); json.WriteBoolean("linkBusy", row.LinkBusy);
            json.WriteNumber("opacity", row.Opacity); json.WriteNumber("width", row.Width); json.WriteNumber("height", row.Height);
            json.WriteNumber("full", row.Full); json.WriteNumber("delta", row.Delta); json.WriteNumber("copied", row.Copied);
            json.WriteNumber("acceptedRevision", row.Accepted); json.WriteNumber("displayedRevision", row.Displayed);
            json.WriteNumber("textGeneration", row.Text); json.WriteNumber("layoutGeneration", row.Layout); json.WriteString("renderingError", row.RenderingError);
            json.WriteNumber("previewOffset", row.PreviewOffset); json.WriteNumber("scrollStart", row.ScrollStart); json.WriteNumber("scrollEnd", row.ScrollEnd);
            json.WriteNumber("scrollProgress", row.ScrollProgress); json.WriteNumber("sourceLine", row.SourceLine); json.WriteNumber("sourceSubLine", row.SourceSubLine);
            json.WriteNumber("sourceRowOffset", row.SourceRowOffset); json.WriteBoolean("sourceRestorePending", row.SourceViewportPending);
            json.WriteEndObject();
        }
        json.WriteEndArray(); json.WriteStartArray("derivedSamples");
        foreach (var sample in item.Derived)
        {
            json.WriteStartObject(); json.WriteNumber("qpc", sample.Qpc); json.WriteNumber("endQpc", sample.EndQpc);
            json.WriteNumber("formulas", sample.Formulas); json.WriteNumber("diagrams", sample.Diagrams); json.WriteNumber("images", sample.Images);
            json.WriteNumber("pendingDerived", sample.PendingDerived); json.WriteNumber("unknownDerived", sample.UnknownDerived);
            json.WriteNumber("pendingImages", sample.PendingImages); json.WriteNumber("unknownImages", sample.UnknownImages);
            json.WriteStartObject("states"); foreach (var pair in sample.States) json.WriteNumber(pair.Key, pair.Value); json.WriteEndObject();
            json.WriteEndObject();
        }
        json.WriteEndArray(); json.WriteEndObject();
    }
}
