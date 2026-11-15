// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;
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
using Windows.UI.Xaml.Automation.Provider;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Media;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

// Opt-in actual staged Lab replay. Reports contain hashes, fixed labels and
// numeric observations only. Rendering callbacks are not GPU presentations.
internal static partial class MarkdownLabViewportHighlightTests
{
    private const int IntervalMs = 58, Keys = 104, PacedIntervalMs = 220, PacedKeys = 8, MaximumRows = 32768;
    private const int HeartbeatIntervalMs = 16, MaximumHeartbeats = 16384;
    private static readonly string[] CounterNames = { "version", "requestedBatches", "startedBatches", "completedBatches",
        "appliedBatches", "staleRejectedBatches", "reusedBlocks", "newlyCompletedBlocks", "activeWorker",
        "visibleEligibleBlocks", "visibleTerminalBlocks", "visibleCompleteBlocks", "textGeneration", "workQpcTicks" };

    internal static async Task RunAsync(StringBuilder log)
    {
        var root = ApplicationData.Current.LocalFolder.Path;
        var path = Path.Combine(root, "startup-input", "Markdown Rendering Lab.md");
        var output = Path.Combine(root, "markdown-lab-viewport-highlight.json");
        var originalWindow = Window.Current.Content;
        var report = new Report();
        string stage = "fixture";
        try
        {
            var bytes = await File.ReadAllBytesAsync(path);
            using var manifest = JsonDocument.Parse(await File.ReadAllTextAsync(path + ".fixture.json"));
            report.FileSha256 = Hash(bytes);
            Require(report.FileSha256 == manifest.RootElement.GetProperty("sha256").GetString(), "Fixture hash mismatch.");
            report.Baseline = manifest.RootElement.TryGetProperty("viewportHighlightBaseline", out var baseline) && baseline.GetBoolean();
            report.ExternalSamplingRequested = manifest.RootElement.TryGetProperty("externalThreadSampling", out var externalSampling) && externalSampling.GetBoolean();
            Require(!manifest.RootElement.TryGetProperty("animatedScrollReplay", out var legacyAnimated) || !legacyAnimated.GetBoolean(),
                "The raw-animation replay is retired; use the versioned explicit-intent workload.");
            report.FastSweep = manifest.RootElement.TryGetProperty("fastPacedScrollReplay", out var fastReplay) && fastReplay.GetBoolean();
            var expectedAssets = manifest.RootElement.GetProperty("assetSha256");
            var assetRoot = Path.Combine(Path.GetDirectoryName(path), "Markdown Rendering Lab Assets");
            var assets = Directory.GetFiles(assetRoot, "*", SearchOption.AllDirectories);
            Require(assets.Length == expectedAssets.EnumerateObject().Count(), "Fixture asset count mismatch.");
            foreach (var asset in assets)
            {
                var name = Path.GetRelativePath(assetRoot, asset).Replace('\\', '/');
                var digest = Hash(await File.ReadAllBytesAsync(asset));
                Require(expectedAssets.TryGetProperty(name, out var expected) && digest == expected.GetString(), "Fixture asset hash mismatch.");
                report.Assets.Add((asset, digest));
            }
            var canonical = new UTF8Encoding(false, true).GetString(bytes).Replace("\r\n", "\r", StringComparison.Ordinal).Replace('\n', '\r');
            report.CanonicalSha256 = Hash(Encoding.UTF8.GetBytes(canonical));
            var lines = canonical.Split('\r');
            var candidates = FindCodeLines(lines).ToArray();
            Require(candidates.Length > 0, "A recognized fenced code body is required.");
            CoreValidationProbe.SetViewportHighlightBaseline(report.Baseline);
            await using var host = await FeatureTestHost.CreateAsync();
            stage = "source-open";
            var sourceOpen = Stopwatch.GetTimestamp();
            Require(await host.Page.OpenFileAsync(await StorageFile.GetFileFromPathAsync(path)), "The staged Lab did not open.");
            var editor = (TextEditor)host.Core.GetSelectedReadyEditor();
            await editor.TestStopFileChecksAsync();
            Require(editor.GetText() == canonical, "Canonical source mismatch.");
            report.SourceOpenTicks = Stopwatch.GetTimestamp() - sourceOpen;
            var native = editor.TestNativeCore.TestFeatureNative;
            Require(!editor.TestMarkdownPanelOpen, "The first-open diagnostic requires a closed preview.");
            using var recorder = new Recorder(editor, report);
            recorder.Start();
            stage = "first-open";
            recorder.Begin(stage);
            editor.ShowHideContentPreview();
            await CurrentAsync(editor);
            var extension = editor.TestMarkdownPreview;
            extension.FollowSource = false;
            var view = extension.PreviewView;
            var scroll = Descendants(view).OfType<ScrollViewer>().Single(item => item.Name == "PreviewScrollViewer");
            recorder.Bind(scroll);
            await SettledAsync(editor, requireCode: false, report.Baseline);
            recorder.End();

            // Opt-in first visitation to the paragraph immediately after a cold
            // text target. Public source navigation only; no native cache overrides.
            if (manifest.RootElement.TryGetProperty("readyOnlyProbeSourceLine", out var readyOnlyLine))
            {
                Require(readyOnlyLine.TryGetInt32(out var sourceLine) && sourceLine > 0 && sourceLine <= lines.Length,
                    "The ready-only probe source line is invalid.");
                Require(!manifest.RootElement.TryGetProperty("continuousScrollReplay", out var probeSweep) || !probeSweep.GetBoolean(),
                    "A targeted first visitation cannot qualify a full cold sweep.");
                Require(scroll.VerticalOffset < 1, "The targeted first visitation must begin at the document top.");
                report.ReadyOnlyProbeSourceLine = sourceLine;
                report.ReadyOnlyProbeSourceHash = Hash(Encoding.UTF8.GetBytes(lines[sourceLine - 1]));
                report.ReadyOnlyProbeSourceByte = checked((ulong)native.PositionFromLine(sourceLine - 1));
                stage = "ready-only-first-navigation";
                recorder.Begin(stage);
                var phase = report.Phases[^1];
                phase.StartNativeTrace();
                try
                {
                    report.ReadyOnlyProbeLogicalOffset = await NavigateOnceAsync(view, report.ReadyOnlyProbeSourceByte);
                    await SettledAsync(editor, requireCode: false, report.Baseline);
                }
                finally { phase.StopNativeTrace(); }
                recorder.End();
            }

            if (manifest.RootElement.TryGetProperty("continuousScrollReplay", out var replay) && replay.GetBoolean())
            {
                await SweepAsync("cold-scroll-down", true);
                await SweepAsync("warm-scroll-up", false);
                await SweepAsync("warm-scroll-down", true);
                await SweepAsync("warm-scroll-return", false);
            }

            // Choose an unambiguous code probe entirely in memory. The content
            // is never included in the report or exception messages.
            var code = candidates.FirstOrDefault(item => Unique(view.DisplayedText, lines[item.Line]), (Line: -1, FenceLine: -1));
            var codeLine = code.Line;
            Require(codeLine >= 0, "The code body needs a unique readable line.");
            recorder.Probe = lines[codeLine];
            report.CodeLine = codeLine + 1;
            report.CodeProbeSha256 = Hash(Encoding.UTF8.GetBytes(recorder.Probe));
            stage = "code-source-navigation";
            recorder.Begin(stage);
            report.CodeLogicalOffset = await NavigateOnceAsync(view, checked((ulong)native.PositionFromLine(codeLine)));
            await UntilAsync(() => recorder.CodeVisible(), "The mapped code line is visibly presented.", 30000);
            await SettledAsync(editor, requireCode: true, report.Baseline);
            var codeOffset = scroll.VerticalOffset;
            recorder.End();

            stage = "actual-viewport-scroll";
            foreach (var target in new[] { 0.0, scroll.ScrollableHeight * 0.5, scroll.ScrollableHeight,
                Math.Max(0, codeOffset - scroll.ViewportHeight * 0.7), codeOffset,
                Math.Min(scroll.ScrollableHeight, codeOffset + scroll.ViewportHeight * 0.4), codeOffset })
            {
                recorder.Begin(stage);
                var phase = report.Phases[^1];
                phase.StartNativeTrace();
                try
                {
                    await ScrollAsync(target);
                    await SettledAsync(editor, requireCode: false, report.Baseline);
                }
                finally { phase.StopNativeTrace(); }
                recorder.End();
            }
            await UntilAsync(() => recorder.CodeVisible(), "The original code line is visible after real reverse scrolling.", 30000);
            await SettledAsync(editor, requireCode: true, report.Baseline);

            var outsideLine = Array.FindIndex(lines, 0, code.FenceLine, string.IsNullOrEmpty);
            Require(outsideLine >= 0, "An insertion line before the code body is required.");
            await TypeAsync("typing-unchanged-code", outsideLine, 0);
            await TypeAsync("typing-changed-code", codeLine, lines[codeLine].Length);
            // Each paced edit spans the host's 120 ms debounce.
            await TypeAsync("typing-paced-unchanged-code", outsideLine, 0, PacedKeys, PacedIntervalMs);
            Require(editor.GetText() == canonical, "Undo did not restore canonical source.");
            Require(Hash(await File.ReadAllBytesAsync(path)) == report.FileSha256, "The staged input changed on disk.");
            foreach (var asset in report.Assets)
                Require(Hash(await File.ReadAllBytesAsync(asset.Path)) == asset.Hash, "A staged asset changed on disk.");
            report.FileUnchanged = true;
            Require(!report.Overflow && !report.HeartbeatOverflow && report.ObservationErrors == 0, "Observation was incomplete.");
            Require(report.Phases.All(phase => phase.NativePhases == null || phase.EditorPhases?.Dropped == 0), "Editor trace was incomplete.");
            Require(report.ViewChanging > 0 && report.ViewChanged > 0, "The actual ScrollViewer emitted no scroll observations.");
            log.AppendLine($"PASS: staged Lab viewport highlighting baseline={report.Baseline}, phases={report.Phases.Count}, samples={report.Rows.Count}, sourceUnchanged={report.FileUnchanged}.");

            async Task SweepAsync(string name, bool down)
            {
                stage = name;
                // Bound each requested step even after a UI stall. Chase the
                // current bottom so late resource geometry cannot shorten a pass.
                // Four bounded sections keep each native trace below its capacity.
                var extent = scroll.ScrollableHeight;
                Require(extent > scroll.ViewportHeight, "The Lab must provide a scrollable sweep.");
                for (var section = 1; section <= 4; ++section)
                {
                    var target = extent * (down ? section / 4.0 : 1 - section / 4.0);
                    if (report.ExternalSamplingRequested && name == "cold-scroll-down" && section == 1)
                        await PrepareExternalSamplingAsync(root, report);
                    var started = Stopwatch.GetTimestamp();
                    var previous = started;
                    long stable = 0;
                    recorder.Begin(name + "-" + section);
                    var phase = report.Phases[^1];
                    phase.RequestedOffset = target;
                    phase.StartNativeTrace();
                    try
                    {
                        while (true)
                        {
                            Require(Stopwatch.GetElapsedTime(started).TotalSeconds < 45, "The scroll sweep timed out.");
                            if (down && section == 4)
                            {
                                if (Math.Abs(target - scroll.ScrollableHeight) >= 1) stable = 0;
                                target = scroll.ScrollableHeight;
                            }
                            phase.RequestedOffset = target;
                            var now = Stopwatch.GetTimestamp();
                            var remaining = Math.Min(target, scroll.ScrollableHeight) - scroll.VerticalOffset;
                            if (Math.Abs(remaining) >= 1)
                            {
                                stable = 0;
                                var step = Math.Min(Math.Abs(remaining), Math.Min(scroll.ViewportHeight / 2,
                                    Stopwatch.GetElapsedTime(previous, now).TotalSeconds * (report.FastSweep ? 18000 : 2200)));
                                // Use the control's explicit scroll intent so cold
                                // resource anchor restoration cannot fight this move.
                                var offset = scroll.VerticalOffset + Math.Sign(remaining) * step;
                                ++phase.ScrollRequests;
                                if (!view.TrySetVerticalScrollProgress(Math.Clamp(offset / scroll.ScrollableHeight, 0, 1)))
                                    ++phase.ScrollRequestsRejected;
                            }
                            else
                            {
                                if (stable == 0) stable = now;
                                if (Stopwatch.GetElapsedTime(stable, now).TotalMilliseconds >= 600)
                                {
                                    await SettledAsync(editor, requireCode: false, report.Baseline);
                                    var end = down && section == 4 ? scroll.ScrollableHeight : Math.Min(target, scroll.ScrollableHeight);
                                    if (Math.Abs(scroll.VerticalOffset - end) < 1) break;
                                    stable = 0;
                                }
                            }
                            previous = now;
                            await Task.Delay(16);
                        }
                    }
                    finally { phase.StopNativeTrace(); }
                    recorder.End();
                }
            }

            async Task ScrollAsync(double target)
            {
                target = Math.Clamp(target, 0, scroll.ScrollableHeight);
                if (report.Phases[^1].End == 0) report.Phases[^1].RequestedOffset = target;
                if (Math.Abs(scroll.VerticalOffset - target) > 0.5)
                {
                    Require(scroll.ChangeView(null, target, null, false), "The real ScrollViewer rejected the scroll.");
                    await UntilAsync(() => Math.Abs(scroll.VerticalOffset - Math.Min(target, scroll.ScrollableHeight)) < 1,
                        "The real preview scroll reaches its requested viewport.", 30000);
                }
                await Task.Delay(160);
            }

            async Task TypeAsync(string name, int line, int column, int keys = Keys, int intervalMs = IntervalMs)
            {
                stage = name;
                await ScrollAsync(codeOffset);
                await UntilAsync(() => recorder.CodeVisible(), "The code line is visible before sustained typing.", 30000);
                await SettledAsync(editor, requireCode: true, report.Baseline);
                var bytePosition = native.PositionFromLine(line) + Encoding.UTF8.GetByteCount(lines[line][..column]);
                var charPosition = lines.Take(line).Sum(item => item.Length + 1) + column;
                native.SetEmptySelection(bytePosition);
                var before = native.DocumentRevision;
                recorder.Begin(name);
                var typingPhase = report.Phases[^1];
                typingPhase.StartNativeTrace();
                try
                {
                    native.BeginUndoAction();
                    try
                    {
                        for (var index = 0; index < keys; ++index)
                        {
                            var begin = Stopwatch.GetTimestamp();
                            var revision = native.DocumentRevision;
                            editor.TypeText("x");
                            report.Input.Add(new Input(name, index, begin, Stopwatch.GetTimestamp(), revision, native.DocumentRevision));
                            await Task.Delay(intervalMs);
                        }
                    }
                    finally { native.EndUndoAction(); }
                    Require(editor.GetText() == canonical.Insert(charPosition, new string('x', keys)), "The actual typing sequence differs.");
                    Require(native.DocumentRevision > before, "Typing produced no source revision.");
                    await CurrentAsync(editor);
                    await SettledAsync(editor, requireCode: true, report.Baseline);
                    Require(recorder.CodeVisible(), "The code line is not visibly presented after sustained typing.");
                }
                finally { typingPhase.StopNativeTrace(); }
                recorder.End();
                recorder.Begin(name + "-undo");
                native.Undo();
                await CurrentAsync(editor);
                await SettledAsync(editor, requireCode: true, report.Baseline);
                Require(editor.GetText() == canonical && recorder.CodeVisible(), "Undo did not restore the visible canonical code.");
                recorder.End();
            }
        }
        catch (Exception error)
        {
            report.Error = stage + "/" + error.GetType().Name + "/" + error.HResult.ToString("X8");
            log.AppendLine("FAIL: Lab viewport highlight " + report.Error);
            log.AppendLine(error.StackTrace);
        }
        finally
        {
            CoreValidationProbe.SetViewportHighlightBaseline(false);
            Window.Current.Content = originalWindow;
            WriteReport(output, report);
        }
        if (report.Error != null) throw new InvalidOperationException("The staged Lab viewport highlight diagnostic failed; inspect its numeric report.");
    }

    private static bool IsCurrent(MarkdownExtensionView extension, MarkdownView view) =>
        extension?.SourceSession.IsCurrent == true && view?.IsSurfaceReady == true &&
        view.DisplayedDocumentId == extension.SourceSession.DocumentId &&
        view.DisplayedRevision == extension.SourceSession.AcceptedRevision && string.IsNullOrEmpty(view.RenderingError);

    private static async Task CurrentAsync(TextEditor editor) => await UntilAsync(() =>
    {
        var extension = editor.TestMarkdownPreview;
        var view = extension?.PreviewView;
        return IsCurrent(extension, view) && view.GetViewportAnchor().IsAvailable;
    }, "The actual source revision is presented in the preview.", 30000);

    private static async Task SettledAsync(TextEditor editor, bool requireCode, bool baseline)
    {
        await UntilAsync(() =>
        {
            var extension = editor.TestMarkdownPreview;
            var view = extension?.PreviewView;
            if (!IsCurrent(extension, view) || !view.GetViewportAnchor().IsAvailable) return false;
            var counters = CoreValidationProbe.GetViewportHighlightDiagnostics(view);
            return counters.Length >= CounterNames.Length && counters[0] == 1 && counters[8] == 0 &&
                !CoreValidationProbe.StartupHighlightsPending(view) && (baseline || counters[10] >= counters[9]) &&
                (!requireCode || counters[9] > 0 && counters[11] > 0);
        }, "Visible syntax highlight work reaches a terminal observation.", 30000);
        await Task.Delay(160);
    }

    private static async Task<ulong> NavigateOnceAsync(MarkdownView view, ulong sourceByte)
    {
        var start = Stopwatch.GetTimestamp();
        while (Stopwatch.GetElapsedTime(start).TotalSeconds < 20)
        {
            var selection = view.GetSelection();
            if (selection.IsAvailable)
            {
                try
                {
                    var mapped = await view.MapSourcePositionAsync(selection.DocumentId, selection.Revision,
                        selection.TextGeneration, sourceByte, MarkdownPositionAffinity.After).AsTask().WaitAsync(TimeSpan.FromSeconds(5));
                    var anchor = view.GetViewportAnchor();
                    if (anchor.IsAvailable && view.SetViewportBetweenPositions(selection.DocumentId, selection.Revision,
                        selection.TextGeneration, anchor.LayoutGeneration, mapped.PreviewUtf16Offset, mapped.PreviewUtf16Offset, 0))
                        return mapped.PreviewUtf16Offset;
                }
                catch (Exception error) when (error.HResult == unchecked((int)0x80040201)) { }
            }
            await Task.Delay(60);
        }
        throw new TimeoutException("The one-time code source navigation did not acquire a current frame.");
    }

    // Opt-in rendezvous outside the measured phase. The desktop helper is owned
    // by the runner, uses the supplied process identity and never elevates.
    private static async Task PrepareExternalSamplingAsync(string root, Report report)
    {
        var before = Stopwatch.GetTimestamp();
        Require(NativeMethods.GetCurrentThreadId() == report.UiThreadId, "External sampling moved off the UI thread.");
        Require(NativeMethods.GetProcessTimes(NativeMethods.GetCurrentProcess(), out var creation, out _, out _, out _) != 0,
            "Cannot establish the current process identity for external sampling.");
        var target = Path.Combine(root, "external-ui-target.json");
        var directory = Path.Combine(root, "wct-sampler");
        var ready = Path.Combine(directory, "ready.json");
        var start = Path.Combine(directory, "start");
        Require(!File.Exists(target) && !File.Exists(ready) && !File.Exists(start), "External sampling rendezvous must be fresh.");
        Directory.CreateDirectory(directory);
        using (var data = new MemoryStream())
        {
            using (var json = new Utf8JsonWriter(data))
            {
                json.WriteStartObject(); json.WriteNumber("schema", 1);
                json.WriteNumber("pid", Environment.ProcessId); json.WriteNumber("uiThreadId", report.UiThreadId);
                json.WriteString("processCreationFileTime", creation.ToString(System.Globalization.CultureInfo.InvariantCulture));
                json.WriteNumber("qpc", before); json.WriteNumber("frequency", Stopwatch.Frequency);
                json.WriteEndObject(); json.Flush();
            }
            File.WriteAllBytes(target + ".tmp", data.ToArray());
        }
        File.Move(target + ".tmp", target);
        report.ExternalSamplingStatus = "timeout";
        while (Stopwatch.GetElapsedTime(before).TotalSeconds < 5)
        {
            if (File.Exists(ready))
            {
                using var response = JsonDocument.Parse(await File.ReadAllBytesAsync(ready));
                var available = response.RootElement.GetProperty("available").GetBoolean();
                report.ExternalSamplingStatus = available ? "available" : "unavailable";
                if (available)
                {
                    report.ExternalSamplingStartQpc = Stopwatch.GetTimestamp();
                    File.WriteAllBytes(start, Array.Empty<byte>());
                }
                break;
            }
            await Task.Delay(10);
        }
        report.ExternalSamplingWaitTicks = Stopwatch.GetTimestamp() - before;
    }

    private static IEnumerable<(int Line, int FenceLine)> FindCodeLines(string[] lines)
    {
        var languages = new HashSet<string>(new[] { "json", "rust", "rs", "cpp", "c++", "c", "js", "javascript", "ts", "typescript",
            "cs", "csharp", "c#", "python", "py", "yaml", "yml", "bash", "sh", "shell", "html", "xml", "css", "sql", "go", "java" }, StringComparer.OrdinalIgnoreCase);
        for (int index = 0; index < lines.Length; ++index)
        {
            var header = lines[index].TrimStart();
            if (!header.StartsWith("```", StringComparison.Ordinal) && !header.StartsWith("~~~", StringComparison.Ordinal)) continue;
            var fence = header[0];
            var count = header.TakeWhile(c => c == fence).Count();
            var openingLine = index;
            var info = header[count..].Trim();
            if (fence == '`' && info.Contains('`')) continue;
            var language = new string(info.TakeWhile(c => !char.IsWhiteSpace(c) && c != ':' && c != '{').ToArray());
            var supported = languages.Contains(language);
            for (++index; index < lines.Length; ++index)
            {
                var body = lines[index].TrimStart();
                var closing = body.TakeWhile(c => c == fence).Count();
                if (closing >= count && string.IsNullOrWhiteSpace(body[closing..])) break;
                if (supported && lines[index].Length >= 8 && !string.IsNullOrWhiteSpace(lines[index])) yield return (index, openingLine);
            }
        }
    }

    private static bool Unique(string text, string needle) => text.IndexOf(needle, StringComparison.Ordinal) is var at && at >= 0 &&
        text.IndexOf(needle, at + needle.Length, StringComparison.Ordinal) < 0;
    private static string Hash(byte[] bytes) => Convert.ToHexString(SHA256.HashData(bytes));
    private static void Require(bool condition, string message) { if (!condition) throw new InvalidOperationException(message); }
    private static IEnumerable<DependencyObject> Descendants(DependencyObject parent)
    {
        for (int index = 0; index < VisualTreeHelper.GetChildrenCount(parent); ++index)
        {
            var child = VisualTreeHelper.GetChild(parent, index);
            yield return child;
            foreach (var descendant in Descendants(child)) yield return descendant;
        }
    }

    private sealed partial class Recorder(TextEditor editor, Report report) : IDisposable
    {
        private ScrollViewer _scroll;
        private readonly DispatcherTimer _heartbeat = new() { Interval = TimeSpan.FromMilliseconds(HeartbeatIntervalMs) };
        private long _visibilityQpc, _visibilityAttemptQpc;
        private (ulong Text, ulong Layout, double Offset) _visibilityFrame;
        private bool _visible;
        internal string Probe;
        internal void Start()
        {
            EditorUiTrace.Prepare();
            report.UiThreadId = NativeMethods.GetCurrentThreadId();
            CompositionTarget.Rendering += Rendering;
            _heartbeat.Tick += OnHeartbeat;
            _heartbeat.Start();
        }
        internal void Bind(ScrollViewer scroll)
        { _scroll = scroll; scroll.ViewChanging += Changing; scroll.ViewChanged += Changed; }
        public void Dispose()
        {
            _heartbeat.Stop();
            _heartbeat.Tick -= OnHeartbeat;
            CompositionTarget.Rendering -= Rendering;
            if (_scroll == null) return;
            _scroll.ViewChanging -= Changing; _scroll.ViewChanged -= Changed;
        }
        internal void Begin(string phase)
        {
            report.Phases.Add(new Phase { Name = phase, Start = Stopwatch.GetTimestamp() });
            Capture("begin", true);
        }
        internal void End() { Capture("end", true); report.Phases[^1].End = Stopwatch.GetTimestamp(); }
        // The UI timer does not depend on Task.Delay's thread-pool continuation.
        // Record clocks, same-thread CPU totals and GC counters; do not query the view or force paint.
        private void OnHeartbeat(object sender, object args)
        {
            if (report.Phases.Count == 0 || report.Phases[^1].End != 0) return;
            if (report.Heartbeats.Count >= MaximumHeartbeats) { report.HeartbeatOverflow = true; return; }
            try
            {
                var qpc = Stopwatch.GetTimestamp();
                var threadId = NativeMethods.GetCurrentThreadId();
                if (threadId != report.UiThreadId) throw new InvalidOperationException("The heartbeat moved off the UI thread.");
                var thread = NativeMethods.GetCurrentThread(); // Pseudo-handle; do not close it.
                var cpuBegin = Stopwatch.GetTimestamp();
                var success = NativeMethods.GetThreadTimes(thread, out _, out _, out var kernel, out var user);
                var cpuEnd = Stopwatch.GetTimestamp();
                if (success == 0) throw new InvalidOperationException("The UI thread CPU query failed.");
                var gen0 = GC.CollectionCount(0);
                var gen1 = GC.CollectionCount(1);
                var gen2 = GC.CollectionCount(2);
                var index = report.Heartbeats.Count;
                report.Heartbeats.Add(default);
                report.Heartbeats[index] = new HeartbeatRow(report.Phases.Count - 1, qpc, Stopwatch.GetTimestamp(), gen0, gen1, gen2,
                    threadId, cpuBegin, cpuEnd, kernel, user);
            }
            catch { ++report.ObservationErrors; }
        }
        private void Rendering(object sender, object args) => Capture("rendering", false);
        private void Changing(object sender, ScrollViewerViewChangingEventArgs args)
        { ++report.ViewChanging; Capture("view-changing", false); }
        private void Changed(object sender, ScrollViewerViewChangedEventArgs args)
        { ++report.ViewChanged; Capture(args.IsIntermediate ? "view-intermediate" : "view-settled", false); }
        internal bool CodeVisible() => ObserveCodeVisibility() == true;

        private bool? ObserveCodeVisibility()
        {
            var view = editor.TestMarkdownPreview?.PreviewView;
            if (Probe == null || view == null || !view.GetSelection().IsAvailable) return null;
            try
            {
                var peer = FrameworkElementAutomationPeer.FromElement(view) ?? FrameworkElementAutomationPeer.CreatePeerForElement(view);
                var provider = peer?.GetPattern(PatternInterface.Text) as ITextProvider;
                var document = provider?.DocumentRange;
                if (document == null) return null;
                var range = document.FindText(Probe, false, false);
                if (range == null) return false;
                range.GetBoundingRectangles(out var rectangles);
                if (rectangles == null) return null;
                var clip = peer.GetBoundingRectangle();
                for (int index = 0; index + 3 < rectangles.Length; index += 4)
                    if (rectangles[index + 2] > 0 && rectangles[index + 3] > 0 && rectangles[index] < clip.Right &&
                        rectangles[index] + rectangles[index + 2] > clip.Left && rectangles[index + 1] < clip.Bottom &&
                        rectangles[index + 1] + rectangles[index + 3] > clip.Top) return true;
                return false;
            }
            catch (Exception error) when (error.HResult == unchecked((int)0x80040201)) { return null; }
        }
        private void Capture(string reason, bool force)
        {
            if (report.Phases.Count == 0 || report.Phases[^1].End != 0) return;
            if (report.Rows.Count >= MaximumRows) { report.Overflow = true; return; }
            var qpc = Stopwatch.GetTimestamp();
            try
            {
                var extension = editor.TestMarkdownPreview;
                var view = extension?.PreviewView;
                var anchorBegin = Stopwatch.GetTimestamp();
                var anchor = view?.GetViewportAnchor();
                var anchorEnd = Stopwatch.GetTimestamp();
                var offset = _scroll?.VerticalOffset ?? 0;
                var frame = (anchor?.TextGeneration ?? 0, anchor?.LayoutGeneration ?? 0, offset);
                var current = IsCurrent(extension, view);
                var probeInterval = _visibilityAttemptQpc == _visibilityQpc ? 160 : 32;
                if (current && anchor?.IsAvailable == true && Probe != null &&
                    (force || Stopwatch.GetElapsedTime(_visibilityAttemptQpc, qpc).TotalMilliseconds >= probeInterval))
                {
                    _visibilityAttemptQpc = qpc;
                    if (ObserveCodeVisibility() is bool visible)
                    {
                        _visible = visible;
                        _visibilityFrame = frame;
                        _visibilityQpc = qpc;
                    }
                }
                var hasVisibility = current && anchor?.IsAvailable == true && _visibilityQpc != 0 && _visibilityFrame == frame;
                var loading = extension?.FindName("PreviewLoading") as Border;
                var exposed = current && view?.IsLoaded == true && view.IsSurfaceReady && view.Opacity > 0 && view.IsEnabled && loading?.Visibility != Visibility.Visible;
                var row = new Row { Qpc = qpc, Phase = report.Phases.Count - 1, Reason = reason, Current = current, Exposed = exposed,
                    Accepted = extension?.SourceSession.AcceptedRevision ?? 0, Displayed = view?.DisplayedRevision ?? 0,
                    Full = extension?.SourceSession.FullSubmissions ?? 0, Delta = extension?.SourceSession.DeltaSubmissions ?? 0,
                    Offset = offset, ScrollableHeight = _scroll?.ScrollableHeight ?? 0,
                    ViewportHeight = _scroll?.ViewportHeight ?? 0,
                    Anchor = anchor?.IsAvailable == true, Text = anchor?.TextGeneration ?? 0,
                    Layout = anchor?.LayoutGeneration ?? 0, CodeVisible = hasVisibility && _visible,
                    VisibilityQpc = hasVisibility ? _visibilityQpc : 0,
                    AnchorBeginQpc = anchorBegin, AnchorEndQpc = anchorEnd };
                row.CountersBeginQpc = Stopwatch.GetTimestamp();
                row.Counters = view == null ? Array.Empty<ulong>() : CoreValidationProbe.GetViewportHighlightDiagnostics(view);
                row.CountersEndQpc = Stopwatch.GetTimestamp();
                report.Rows.Add(row);
                // Include storage in the observed body; do not leave list growth unaccounted.
                row.EndQpc = Stopwatch.GetTimestamp();
            }
            catch (Exception error) when (error.HResult == unchecked((int)0x80040201)) { ++report.TransientObservations; }
            catch { ++report.ObservationErrors; }
        }
    }

    private sealed class Report
    {
        internal bool Baseline, FileUnchanged, Overflow, HeartbeatOverflow, ExternalSamplingRequested, FastSweep;
        internal string ExternalSamplingStatus = "not-requested";
        internal long ExternalSamplingStartQpc, ExternalSamplingWaitTicks;
        internal string FileSha256, CanonicalSha256, CodeProbeSha256, Error;
        internal int CodeLine, ObservationErrors, TransientObservations, ViewChanging, ViewChanged;
        internal uint UiThreadId;
        internal long SourceOpenTicks;
        internal ulong CodeLogicalOffset;
        internal int ReadyOnlyProbeSourceLine;
        internal string ReadyOnlyProbeSourceHash;
        internal ulong ReadyOnlyProbeSourceByte, ReadyOnlyProbeLogicalOffset;
        internal readonly List<(string Path, string Hash)> Assets = new();
        internal readonly List<Row> Rows = new(MaximumRows);
        internal readonly List<HeartbeatRow> Heartbeats = new(MaximumHeartbeats);
        internal readonly List<Phase> Phases = new();
        internal readonly List<Input> Input = new();
    }
    private sealed class Phase
    {
        internal string Name;
        internal long Start, End;
        internal double RequestedOffset = -1;
        internal int ScrollRequests, ScrollRequestsRejected;
        internal ulong[] NativePhases;
        internal EditorUiTrace.Result EditorPhases;
        private EditorUiTrace _editorTrace;

        internal void StartNativeTrace()
        {
            _editorTrace = EditorUiTrace.Start();
            try { CoreValidationProbe.StartPreviewStartupTrace(false); }
            catch { EditorPhases = _editorTrace.Stop(); throw; }
        }

        internal void StopNativeTrace()
        {
            try { NativePhases = CoreValidationProbe.StopPreviewStartupTrace(); }
            finally { EditorPhases = _editorTrace.Stop(); }
        }
    }
    private sealed class Row
    {
        internal string Reason;
        internal int Phase;
        internal long Qpc, EndQpc, VisibilityQpc, Full, Delta;
        internal long AnchorBeginQpc, AnchorEndQpc, CountersBeginQpc, CountersEndQpc;
        internal ulong Accepted, Displayed, Text, Layout;
        internal double Offset, ScrollableHeight, ViewportHeight;
        internal bool Current, Exposed, Anchor, CodeVisible;
        internal ulong[] Counters;
    }
    // FILETIME fields here are cumulative 100 ns CPU durations, not dates.
    // Keep ordinary P/Invoke GC transitions; these are observations, not a GC experiment.
    private static partial class NativeMethods
    {
        [LibraryImport("kernel32.dll")]
        internal static partial nint GetCurrentProcess();
        [LibraryImport("kernel32.dll")]
        internal static partial int GetProcessTimes(nint process, out ulong creation, out ulong exit, out ulong kernel, out ulong user);
        [LibraryImport("kernel32.dll")]
        internal static partial nint GetCurrentThread();
        [LibraryImport("kernel32.dll")]
        internal static partial uint GetCurrentThreadId();
        [LibraryImport("kernel32.dll")]
        internal static partial int GetThreadTimes(nint thread, out ulong creation, out ulong exit, out ulong kernel, out ulong user);
    }

    private readonly record struct HeartbeatRow(int Phase, long Qpc, long EndQpc, int Gen0, int Gen1, int Gen2,
        uint ThreadId, long CpuBeginQpc, long CpuEndQpc, ulong Kernel100ns, ulong User100ns);
    private sealed record Input(string Phase, int Index, long Begin, long End, ulong Before, ulong After);

    private static void WriteReport(string path, Report report)
    {
        using var stream = new MemoryStream();
        using (var json = new Utf8JsonWriter(stream, new JsonWriterOptions { Indented = true }))
        {
            json.WriteStartObject(); json.WriteNumber("schema", 1); json.WriteString("method", "actual-staged-lab-viewport-highlights-v1");
            json.WriteBoolean("baseline", report.Baseline); json.WriteNumber("frequency", Stopwatch.Frequency);
            json.WriteString("scope", "Fresh-process source open and preview, optional bounded continuous cold/warm sweeps through public scroll progress, one source-mapped code navigation, actual ScrollViewer observations and sustained TypeText. Rendering callbacks are not GPU presents; native terminal counts are not pixel color assertions.");
            json.WriteString("fileSha256", report.FileSha256); json.WriteString("canonicalSha256", report.CanonicalSha256);
            json.WriteString("codeProbeSha256", report.CodeProbeSha256); json.WriteNumber("codeSourceLine", report.CodeLine);
            json.WriteNumber("codeLogicalOffset", report.CodeLogicalOffset); json.WriteNumber("assetCount", report.Assets.Count);
            json.WriteNumber("readyOnlyProbeSourceLine", report.ReadyOnlyProbeSourceLine);
            json.WriteString("readyOnlyProbeSourceHash", report.ReadyOnlyProbeSourceHash);
            json.WriteNumber("readyOnlyProbeSourceByte", report.ReadyOnlyProbeSourceByte);
            json.WriteNumber("readyOnlyProbeLogicalOffset", report.ReadyOnlyProbeLogicalOffset);
            json.WriteNumber("sourceOpenTicks", report.SourceOpenTicks); json.WriteNumber("typingIntervalMs", IntervalMs); json.WriteNumber("keysPerBurst", Keys);
            json.WriteNumber("pacedTypingIntervalMs", PacedIntervalMs); json.WriteNumber("pacedKeys", PacedKeys);
            json.WriteBoolean("fileUnchanged", report.FileUnchanged); json.WriteString("error", report.Error);
            json.WriteBoolean("rowsOverflow", report.Overflow); json.WriteNumber("observationErrors", report.ObservationErrors);
            json.WriteNumber("uiThreadId", report.UiThreadId);
            json.WriteString("scrollReplayMode", report.FastSweep ? "paced-18000-dip-per-second-v1" : "paced-2200-dip-per-second");
            json.WriteBoolean("externalThreadSamplingRequested", report.ExternalSamplingRequested);
            json.WriteString("externalThreadSamplingStatus", report.ExternalSamplingStatus);
            json.WriteNumber("externalThreadSamplingStartQpc", report.ExternalSamplingStartQpc);
            json.WriteNumber("externalThreadSamplingWaitTicks", report.ExternalSamplingWaitTicks);
            json.WriteNumber("heartbeatIntervalMs", HeartbeatIntervalMs); json.WriteBoolean("heartbeatsOverflow", report.HeartbeatOverflow);
            json.WriteString("heartbeatScope", "Requested UI DispatcherTimer interval, not guaranteed cadence or GPU presents. This diagnostic adds dispatcher work. GC count changes are correlation, not pause duration or causation.");
            json.WriteString("observerScope", "Capture endQpc includes Rows.Add; storage is preallocated. Anchor and diagnostic-counter query boundaries use the same QPC clock. Other property/UIA observations remain inside the full Capture body. Wall intervals can include scheduling gaps.");
            json.WriteString("threadCpuScope", "GetThreadTimes same-UI-thread cumulative kernel/user 100 ns durations bracketed by cpuBeginQpc/cpuEndQpc. Query time and accounting granularity limit short-window CPU/wall ratios; preserve raw values without clamping. Low CPU cannot distinguish waiting from not being scheduled or identify a callee.");
            json.WriteNumber("transientObservations", report.TransientObservations); json.WriteNumber("viewChangingEvents", report.ViewChanging); json.WriteNumber("viewChangedEvents", report.ViewChanged);
            json.WriteStartArray("counterNames"); foreach (var name in CounterNames) json.WriteStringValue(name); json.WriteEndArray();
            json.WriteStartArray("phases");
            foreach (var phase in report.Phases)
            {
                json.WriteStartObject(); json.WriteString("name", phase.Name); json.WriteNumber("startQpc", phase.Start);
                json.WriteNumber("endQpc", phase.End); json.WriteNumber("requestedOffset", phase.RequestedOffset);
                json.WriteNumber("scrollRequests", phase.ScrollRequests);
                json.WriteNumber("scrollRequestsRejected", phase.ScrollRequestsRejected);
                if (phase.NativePhases != null) MarkdownStartupPerformanceTests.WriteNativePhases(json, phase.NativePhases);
                phase.EditorPhases?.Write(json);
                json.WriteEndObject();
            }
            json.WriteEndArray(); json.WriteStartArray("heartbeats");
            foreach (var pulse in report.Heartbeats)
            {
                json.WriteStartObject(); json.WriteNumber("phase", pulse.Phase); json.WriteNumber("qpc", pulse.Qpc);
                json.WriteNumber("endQpc", pulse.EndQpc); json.WriteNumber("gen0", pulse.Gen0);
                json.WriteNumber("gen1", pulse.Gen1); json.WriteNumber("gen2", pulse.Gen2);
                json.WriteNumber("threadId", pulse.ThreadId); json.WriteNumber("cpuBeginQpc", pulse.CpuBeginQpc); json.WriteNumber("cpuEndQpc", pulse.CpuEndQpc);
                json.WriteNumber("kernel100ns", pulse.Kernel100ns); json.WriteNumber("user100ns", pulse.User100ns); json.WriteEndObject();
            }
            json.WriteEndArray(); json.WriteStartArray("input");
            foreach (var key in report.Input)
            {
                json.WriteStartObject(); json.WriteString("phase", key.Phase); json.WriteNumber("index", key.Index);
                json.WriteNumber("beginQpc", key.Begin); json.WriteNumber("endQpc", key.End);
                json.WriteNumber("revisionBefore", key.Before); json.WriteNumber("revisionAfter", key.After); json.WriteEndObject();
            }
            json.WriteEndArray(); json.WriteStartArray("samples");
            foreach (var row in report.Rows)
            {
                json.WriteStartObject(); json.WriteNumber("qpc", row.Qpc); json.WriteNumber("endQpc", row.EndQpc); json.WriteNumber("phase", row.Phase);
                json.WriteNumber("anchorBeginQpc", row.AnchorBeginQpc); json.WriteNumber("anchorEndQpc", row.AnchorEndQpc);
                json.WriteNumber("countersBeginQpc", row.CountersBeginQpc); json.WriteNumber("countersEndQpc", row.CountersEndQpc);
                json.WriteString("reason", row.Reason); json.WriteBoolean("current", row.Current); json.WriteBoolean("exposed", row.Exposed);
                json.WriteBoolean("anchor", row.Anchor); json.WriteBoolean("codeVisible", row.CodeVisible); json.WriteNumber("visibilityQpc", row.VisibilityQpc);
                json.WriteNumber("accepted", row.Accepted); json.WriteNumber("displayed", row.Displayed); json.WriteNumber("textGeneration", row.Text);
                json.WriteNumber("layoutGeneration", row.Layout); json.WriteNumber("offset", row.Offset); json.WriteNumber("full", row.Full); json.WriteNumber("delta", row.Delta);
                json.WriteNumber("scrollableHeight", row.ScrollableHeight); json.WriteNumber("viewportHeight", row.ViewportHeight);
                json.WriteStartArray("counters"); foreach (var counter in row.Counters) json.WriteNumberValue(counter); json.WriteEndArray(); json.WriteEndObject();
            }
            json.WriteEndArray(); json.WriteEndObject(); json.Flush();
        }
        File.WriteAllBytes(path, stream.ToArray());
    }
}
