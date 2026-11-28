// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Runtime.CompilerServices;
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

namespace NotepadsIntegrationTests
{
    // Opt-in, staged user fixture. Logs contain only hashes, counters and typed states.
    internal static class MarkdownLabEditReplayTests
    {
        private const string Fixture = "Markdown Rendering Lab.md";
        private const string Assets = "Markdown Rendering Lab Assets";
        private const int FastTypingIntervalMs = 58;
        private const string FastTyping = "Lab typing";
        private const string PacedTyping = " abc";

        internal static async Task RunAsync(StringBuilder log)
        {
            var root = ApplicationData.Current.LocalFolder.Path;
            var input = Path.Combine(root, "edit-replay-input");
            var path = Path.Combine(input, Fixture);
            var output = Path.Combine(root, "markdown-lab-edit-replay.json");
            using var stream = new MemoryStream();
            using var json = new Utf8JsonWriter(stream, new JsonWriterOptions { Indented = true });
            var originalWindow = Window.Current.Content;
            var stage = "verify-fixture";
            string error = null;
            json.WriteStartObject();
            json.WriteNumber("schema", 1);
            json.WriteNumber("clockFrequency", Stopwatch.Frequency);
            json.WriteNumber("fastTypingIntervalMs", FastTypingIntervalMs);
            try
            {
                var bytes = await File.ReadAllBytesAsync(path);
                using var manifest = JsonDocument.Parse(await File.ReadAllTextAsync(path + ".fixture.json"));
                var expected = manifest.RootElement.GetProperty("sha256").GetString();
                if (Hash(bytes) != expected)
                    throw new InvalidDataException("The exact document hash differs from its manifest.");
                var assets = manifest.RootElement.GetProperty("assets");
                var actual = Directory.GetFiles(Path.Combine(input, Assets), "*", SearchOption.AllDirectories);
                if (actual.Length != assets.EnumerateObject().Count())
                    throw new InvalidDataException("The staged asset count differs from its manifest.");
                foreach (var asset in actual)
                {
                    var relative = Path.GetRelativePath(input, asset).Replace('\\', '/');
                    if (!assets.TryGetProperty(relative, out var digest) || Hash(await File.ReadAllBytesAsync(asset)) != digest.GetString())
                        throw new InvalidDataException("A staged asset differs from its manifest.");
                }
                json.WriteString("fixtureSha256", expected);
                json.WriteNumber("assetCount", actual.Length);
                var canonical = new UTF8Encoding(false, true).GetString(bytes)
                    .Replace("\r\n", "\r", StringComparison.Ordinal).Replace('\n', '\r');

                await using var host = await FeatureTestHost.CreateAsync();
                stage = "open-fixture";
                var file = await StorageFile.GetFileFromPathAsync(path);
                if (!await host.Page.OpenFileAsync(file))
                    throw new InvalidOperationException("The isolated editor could not open the staged document.");
                var editor = (TextEditor)host.Core.GetSelectedReadyEditor();
                await editor.TestStopFileChecksAsync();
                if (editor.GetText() != canonical)
                    throw new InvalidOperationException("The isolated editor did not open the exact canonical source.");
                editor.ShowHideContentPreview();
                await UntilAsync(() => editor.TestMarkdownPreview?.SourceSession.IsCurrent == true,
                    "The staged Lab preview becomes current.", 30000);
                var extension = editor.TestMarkdownPreview;
                extension.FollowSource = false;
                json.WriteBoolean("followSource", extension.FollowSource);
                var view = extension.PreviewView;
                var native = editor.TestNativeCore.TestFeatureNative;
                var lines = canonical.Split('\r');
                var firstTitle = Array.FindIndex(lines, line => line.StartsWith("# ", StringComparison.Ordinal));
                if (firstTitle < 0 || firstTitle == lines.Length - 1 || lines[firstTitle + 1].Length != 0)
                    throw new InvalidDataException("The fixture lacks a blank insertion line immediately after its title.");
                var editAt = native.PositionFromLine(firstTitle + 1);
                var editOffset = lines.Take(firstTitle + 1).Sum(line => line.Length + 1);
                var noteLine = Array.FindIndex(lines, line => line.TrimStart().StartsWith("![[#", StringComparison.Ordinal));
                var externalLine = noteLine > 0 ? Array.FindLastIndex(lines, noteLine - 1, line =>
                    line.TrimStart().StartsWith("![[", StringComparison.Ordinal)) : -1;
                var diagramLine = Array.FindIndex(lines, line => line.TrimStart().StartsWith("\u0060\u0060\u0060mermaid", StringComparison.Ordinal));
                var mathLine = Array.FindIndex(lines, line => line.StartsWith("## 06 Math", StringComparison.Ordinal));
                var imageLine = Array.FindIndex(lines, lines.Length / 3, line =>
                    (line.Contains("](", StringComparison.Ordinal) || line.Contains("<img", StringComparison.OrdinalIgnoreCase)) &&
                    (line.Contains(Assets, StringComparison.Ordinal) ||
                     line.Contains("Markdown%20Rendering%20Lab%20Assets", StringComparison.Ordinal)));
                var targets = new[] {
                    (Name: "images", Line: imageLine), (Name: "formulas", Line: mathLine + (mathLine >= 0 ? 1 : 0)),
                    (Name: "diagrams", Line: diagramLine), (Name: "external-note", Line: externalLine),
                    (Name: "draft-note", Line: noteLine)
                };
                stage = "whole-note-scroll-cache";
                const string wholeNote = Assets + "/Embedded Note";
                var wholeNoteLine = Array.IndexOf(lines, "![[" + wholeNote + "]]");
                Check(wholeNoteLine >= 0, "The Lab contains the whole external note.");
                var innerOffset = await VerifyWholeNoteScrollCacheAsync(
                    extension, checked((ulong)native.PositionFromLine(wholeNoteLine)), wholeNote, log);
                json.WriteNumber("wholeNoteScrollReturnsWithSameView", 3);
                json.WriteNumber("wholeNoteInnerOffset", innerOffset);
                log.AppendLine("PASS: Lab whole external note stays ready and keeps its view and inner scroll after three returns.");

                var recorder = new Recorder(extension);
                json.WriteStartArray("cases");
                try
                {
                    foreach (var target in targets)
                    {
                        stage = target.Name;
                        if (target.Line < 0)
                            throw new InvalidDataException("The exact fixture lacks a requested sample viewport.");
                        stage = target.Name + "-map";
                        var mapped = await RevealAsync(view, checked((ulong)native.PositionFromLine(target.Line)));
                        stage = target.Name + "-settle";
                        await UntilAsync(() =>
                        {
                            var anchor = view.GetViewportAnchor();
                            return anchor.IsAvailable && anchor.ScrollStartUtf16Offset > 0 &&
                                anchor.ScrollStartUtf16Offset <= mapped + 16 &&
                                view.GetSelection().IsAvailable && recorder.HasBaseline(target.Name);
                        }, "The targeted right-hand viewport contains the requested ready resource.", 30000);
                        stage = target.Name + "-edit";

                        recorder.Start();
                        json.WriteStartObject();
                        json.WriteString("name", target.Name);
                        json.WriteNumber("targetLine", target.Line + 1);
                        try
                        {
                            recorder.Milestone("baseline");
                            native.SetEmptySelection(editAt);
                            native.BeginUndoAction();
                            try
                            {
                                recorder.Milestone("insert-line");
                                var before = native.DocumentRevision;
                                editor.TypeText("\r");
                                await WaitForEdit(before);
                                recorder.Milestone("insert-current");

                                recorder.Milestone("fast-typing");
                                native.SetEmptySelection(editAt);
                                before = native.DocumentRevision;
                                foreach (char c in FastTyping)
                                {
                                    editor.TypeText(c.ToString());
                                    await Task.Delay(FastTypingIntervalMs);
                                }
                                await WaitForEdit(before);
                                recorder.Milestone("fast-current");

                                // Above is real cadence below the editor's 120 ms
                                // debounce. Also force every key through a new revision.
                                for (var index = 0; index < PacedTyping.Length; ++index)
                                {
                                    recorder.Milestone("key-" + (index + 1));
                                    before = native.DocumentRevision;
                                    editor.TypeText(PacedTyping[index].ToString());
                                    await WaitForEdit(before);
                                    recorder.Milestone("key-current-" + (index + 1));
                                }
                            }
                            finally { native.EndUndoAction(); }
                            if (editor.GetText() != canonical.Insert(editOffset, FastTyping + PacedTyping + "\r"))
                                throw new InvalidOperationException("The editor did not apply the exact intended insert and typing sequence.");

                            var edited = native.DocumentRevision;
                            recorder.Milestone("undo");
                            native.Undo();
                            await WaitForEdit(edited);
                            if (editor.GetText() != canonical)
                                throw new InvalidOperationException("Undo did not restore the exact original in-memory document.");
                            recorder.Milestone("undo-current");
                        }
                        finally
                        {
                            recorder.Stop();
                            recorder.Write(json);
                            json.WriteEndObject();
                        }
                        log.AppendLine($"OBS: Lab edit replay {target.Name} samples={recorder.Count}, observationErrors={recorder.Errors}.");
                    }
                }
                finally { json.WriteEndArray(); }
                stage = "verify-unchanged";
                if (Hash(await File.ReadAllBytesAsync(path)) != expected)
                    throw new InvalidOperationException("The staged input file changed on disk.");
                json.WriteBoolean("fileUnchanged", true);

                // Keep sampling first publication and the resource-settling tail
                // without blocking the dispatcher or changing preview position.
                async Task WaitForEdit(ulong before)
                {
                    await UntilAsync(() => native.DocumentRevision > before &&
                        extension.SourceSession.IsCurrent &&
                        view.DisplayedRevision == extension.SourceSession.AcceptedRevision,
                        "The real source revision publishes in the preview.", 30000);
                    await Task.Delay(550);
                }
            }
            catch (Exception failure)
            {
                error = $"{stage}/{failure.GetType().Name}/{failure.HResult:X8}";
                log.AppendLine("FAIL: Lab edit replay " + error);
                if (stage == "whole-note-scroll-cache" && failure is TimeoutException)
                    log.AppendLine("Whole-note wait: " + failure.Message);
            }
            finally
            {
                Window.Current.Content = originalWindow;
                json.WriteString("error", error);
                json.WriteEndObject();
                json.Flush();
                File.WriteAllBytes(output, stream.ToArray());
            }
            if (error != null)
                throw new InvalidOperationException("Lab edit replay did not complete. Inspect its private diagnostic report.");
            log.AppendLine("PASS: actual staged Lab insertion, rapid typing, separately published keys and undo.");
        }

        private static async Task<ulong> RevealAsync(MarkdownView view, ulong sourceByte)
        {
            var start = Stopwatch.GetTimestamp();
            while (Stopwatch.GetElapsedTime(start) < TimeSpan.FromSeconds(20))
            {
                var selection = view.GetSelection();
                if (selection.IsAvailable)
                {
                    try
                    {
                        var mapped = await view.MapSourcePositionAsync(selection.DocumentId, selection.Revision,
                            selection.TextGeneration, sourceByte, MarkdownPositionAffinity.After)
                            .AsTask().WaitAsync(TimeSpan.FromSeconds(5));
                        var anchor = view.GetViewportAnchor();
                        if (anchor.IsAvailable && anchor.TextGeneration == selection.TextGeneration &&
                            anchor.Revision == selection.Revision && anchor.DocumentId == selection.DocumentId &&
                            view.TryRestoreFollowViewport(selection.DocumentId, selection.Revision,
                                selection.TextGeneration, anchor.LayoutGeneration, mapped.PreviewUtf16Offset,
                                mapped.PreviewUtf16Offset, 0, 0, 0))
                            return mapped.PreviewUtf16Offset;
                    }
                    catch (Exception error) when (error.HResult == unchecked((int)0x80040201))
                    {
                        // An image/diagram worker can replace the reading frame
                        // while the asynchronous source map is in flight.
                    }
                }
                await Task.Delay(60);
            }
            throw new TimeoutException("The target viewport mapping never acquired a current reading frame.");
        }

        private static async Task<double> VerifyWholeNoteScrollCacheAsync(
            MarkdownExtensionView extension, ulong sourceByte, string target, StringBuilder log)
        {
            var view = extension.PreviewView;
            var mapped = await RevealAsync(view, sourceByte);
            var scroll = Descendants(view).OfType<ScrollViewer>().First(item => item.Name == "PreviewScrollViewer");
            MarkdownView WholeNote() => extension.TestReadLabReplayNotes().FirstOrDefault(
                note => note.Depth == 1 && note.Key == "/" + target + "/0").View;
            bool SlotVisible() => view.GetVisibleNoteEmbeds().Any(slot => slot.Target == target);
            MarkdownView child = null;
            try
            {
                await UntilAsync(SlotVisible, "The whole external note slot is visible after navigation.", 30000);
                await UntilAsync(() => SlotVisible() &&
                    (child = WholeNote()) is { Visibility: Visibility.Visible, IsSurfaceReady: true } &&
                    child.GetViewportAnchor().IsAvailable, "The whole external note has a usable frame.", 30000);
            }
            catch (TimeoutException)
            {
                var anchor = view.GetViewportAnchor();
                child = WholeNote();
                log.AppendLine($"OBS: Whole note mapped={mapped}, slot={SlotVisible()}, " +
                    $"scroll={scroll.VerticalOffset:R}/{scroll.ScrollableHeight:R}, " +
                    $"anchor={anchor.IsAvailable}/{anchor.ScrollStartUtf16Offset}/{anchor.ScrollEndUtf16Offset}, " +
                    $"child={child?.Visibility}/{child?.IsSurfaceReady}/{child?.GetViewportAnchor().IsAvailable}.");
                log.AppendLine("OBS: Whole note load state: " + extension.TestRootNoteLoadSummary);
                throw;
            }
            var inner = Descendants(child).OfType<ScrollViewer>().First(item => item.Name == "PreviewScrollViewer");
            await UntilAsync(() => inner.ScrollableHeight > 4,
                "The whole Lab note has a scrollable inner viewport.", 30000);
            Check(child.TrySetVerticalScrollProgress(0.35), "The whole note accepts an explicit inner scroll.");
            await UntilAsync(() => child.GetViewportAnchor().IsAvailable && child.GetSelection().IsAvailable &&
                inner.VerticalOffset > 1,
                "The whole note presents its inner scroll before leaving the parent viewport.", 30000);
            var innerOffset = inner.VerticalOffset;
            var generation = child.GetSelection().TextGeneration;
            var parentOffset = scroll.VerticalOffset;
            for (var round = 0; round < 3; ++round)
            {
                var priorAction = extension.TestCaptureNoteLinkAction(child);
                Check(priorAction(), "A visible current note authorizes a new link action.");
                Check(scroll.ChangeView(null, 0, null, true), "The parent scrolls away from the note.");
                await UntilAsync(() => scroll.VerticalOffset < 1 && view.GetViewportAnchor().IsAvailable && !SlotVisible(),
                    "The whole note has left the parent viewport.", 30000);
                Check(ReferenceEquals(WholeNote(), child) && child.Visibility == Visibility.Collapsed,
                    "The offscreen whole note stays mounted and hidden.");
                Check(!priorAction(), "A pending note link loses its authorization while offscreen.");

                Check(scroll.ChangeView(null, parentOffset, null, true), "The parent scrolls back to the note.");
                await UntilAsync(() => scroll.VerticalOffset > 1 && view.GetViewportAnchor().IsAvailable && SlotVisible(),
                    "The whole note slot has returned to the parent viewport.", 30000);
                Check(ReferenceEquals(WholeNote(), child) &&
                    child is { Visibility: Visibility.Visible, IsSurfaceReady: true } &&
                    child.GetSelection() is { IsAvailable: true } selection && selection.TextGeneration == generation &&
                    Math.Abs(inner.VerticalOffset - innerOffset) < 1,
                    "The returned note already has its original ready frame and inner viewport.");
                Check(!priorAction(), "Returning the cached note does not reactivate its previous link action.");
            }
            return innerOffset;
        }

        private static string Hash(byte[] bytes) => Convert.ToHexString(SHA256.HashData(bytes));
        private static string Tag(string value) => Hash(Encoding.UTF8.GetBytes(value))[..16];

        private sealed class Recorder(MarkdownExtensionView extension)
        {
            private readonly List<Row> _rows = new();
            private readonly ConditionalWeakTable<MarkdownView, ViewToken> _views = new();
            private int _nextView;
            private string _phase;
            private (ulong Revision, ulong Layout, ulong Offset, bool Current) _lastFrame;
            private long _lastSample, _started;

            internal int Count => _rows.Count;
            internal int Errors { get; private set; }

            internal bool HasBaseline(string group)
            {
                try
                {
                    if (group == "draft-note" || group == "external-note")
                    {
                        var draft = group == "draft-note";
                        return extension.TestReadLabReplayNotes().Any(note => note.Depth == 1 && note.IsDraft == draft &&
                            note.View is { Visibility: Visibility.Visible } && note.View.GetSelection().IsAvailable);
                    }
                    var family = group switch { "images" => "image", "formulas" => "formula", _ => "diagram" };
                    return ReadResources().Any(resource => resource.Family == family && resource.State == "ready" && resource.Visible);
                }
                catch { return false; }
            }

            internal void Start()
            {
                _rows.Clear();
                _lastFrame = default;
                _lastSample = 0;
                _started = Stopwatch.GetTimestamp();
                Errors = 0;
                CompositionTarget.Rendering += Rendering;
                extension.SourceSession.Changed += SessionChanged;
            }
            internal void Stop()
            {
                extension.SourceSession.Changed -= SessionChanged;
                CompositionTarget.Rendering -= Rendering;
            }
            internal void Milestone(string phase)
            {
                _phase = phase;
                Capture("mark", true);
            }
            private void Rendering(object sender, object args) => Capture("render", false);
            private void SessionChanged(object sender, EventArgs e) => Capture("session", true);

            private void Capture(string kind, bool force)
            {
                if (_rows.Count >= 1200) return;
                try
                {
                    var view = extension.PreviewView;
                    var time = Stopwatch.GetTimestamp();
                    var anchor = view.GetViewportAnchor();
                    var current = view.GetSelection().IsAvailable;
                    var frame = (view.DisplayedRevision, anchor.LayoutGeneration, anchor.ScrollStartUtf16Offset, current);
                    if (!force && frame == _lastFrame && Stopwatch.GetElapsedTime(_lastSample, time).TotalMilliseconds < 160)
                        return;
                    _lastSample = time;
                    _lastFrame = frame;
                    var notes = extension.TestReadLabReplayNotes().Select(note =>
                    {
                        var child = note.View;
                        var token = child == null ? 0 : _views.GetValue(child, _ => new ViewToken(++_nextView)).Id;
                        var selection = child?.GetSelection();
                        return new Note(Tag(note.Key), note.Depth, note.IsDraft, token, selection?.IsAvailable == true,
                            child?.DisplayedRevision ?? 0, selection?.TextGeneration ?? 0);
                    }).ToArray();
                    var resources = Array.Empty<Resource>();
                    if (current)
                    {
                        try { resources = ReadResources(); }
                        catch { Errors++; }
                    }
                    var session = extension.SourceSession;
                    _rows.Add(new Row(time - _started, _phase, kind, frame.Item1, frame.Item2, frame.Item3, current,
                        session.AcceptedRevision, session.FullSubmissions, session.DeltaSubmissions, notes, resources));
                }
                catch { Errors++; }
            }

            private Resource[] ReadResources()
            {
                var view = extension.PreviewView;
                var peer = FrameworkElementAutomationPeer.FromElement(view) ?? FrameworkElementAutomationPeer.CreatePeerForElement(view);
                var children = peer.GetChildren();
                if (children == null) return Array.Empty<Resource>();
                var occurrences = new Dictionary<string, int>(StringComparer.Ordinal);
                var results = new List<Resource>();
                foreach (var child in children)
                {
                    var name = child.GetName();
                    var family = name.StartsWith("Formula:", StringComparison.Ordinal) ? "formula" :
                        name.StartsWith("Diagram:", StringComparison.Ordinal) ? "diagram" :
                        child.GetAutomationControlType() == AutomationControlType.Image ? "image" : null;
                    if (family == null) continue;
                    if (family == "image")
                    {
                        var comma = name.LastIndexOf(',');
                        if (comma >= 0) name = name[..comma];
                    }
                    var key = Tag(family + "|" + name);
                    var ordinal = occurrences.GetValueOrDefault(key);
                    occurrences[key] = ordinal + 1;
                    var help = child.GetHelpText();
                    var state = help == "Image loaded" || help.Contains(" rendered.", StringComparison.Ordinal) ? "ready" :
                        help.Contains("loading", StringComparison.OrdinalIgnoreCase) ? "pending" :
                        help.Contains("limit", StringComparison.OrdinalIgnoreCase) ? "limited" :
                        help.Contains("unsupported", StringComparison.OrdinalIgnoreCase) ||
                        help.Contains("not supported", StringComparison.OrdinalIgnoreCase) ? "unsupported" :
                        help.Contains("not loaded", StringComparison.OrdinalIgnoreCase) ? "not-loaded" :
                        help.Contains("failed", StringComparison.OrdinalIgnoreCase) || help.Contains("error", StringComparison.OrdinalIgnoreCase) ?
                            "failed" : "unavailable";
                    var rect = child.GetBoundingRectangle();
                    results.Add(new Resource(family, key + "/" + ordinal, state, rect.Width > 0 && rect.Height > 0));
                }
                return results.ToArray();
            }

            internal void Write(Utf8JsonWriter json)
            {
                json.WriteNumber("observationErrors", Errors);
                json.WriteBoolean("sampleLimitReached", _rows.Count >= 1200);
                json.WriteStartArray("samples");
                foreach (var row in _rows)
                {
                    json.WriteStartObject();
                    json.WriteNumber("ticks", row.Ticks);
                    json.WriteString("phase", row.Phase);
                    json.WriteString("kind", row.Kind);
                    json.WriteNumber("revision", row.Revision);
                    json.WriteNumber("layout", row.Layout);
                    json.WriteNumber("offset", row.Offset);
                    json.WriteBoolean("reading", row.Reading);
                    json.WriteNumber("accepted", row.Accepted);
                    json.WriteNumber("full", row.Full);
                    json.WriteNumber("delta", row.Delta);
                    json.WriteStartArray("notes");
                    foreach (var note in row.Notes)
                    {
                        json.WriteStartObject();
                        json.WriteString("key", note.Key);
                        json.WriteNumber("depth", note.Depth);
                        json.WriteString("kind", note.IsDraft ? "draft" : "external");
                        json.WriteNumber("view", note.View);
                        json.WriteBoolean("reading", note.Reading);
                        json.WriteNumber("revision", note.Revision);
                        json.WriteNumber("generation", note.Generation);
                        json.WriteEndObject();
                    }
                    json.WriteEndArray();
                    json.WriteStartArray("resources");
                    foreach (var item in row.Resources)
                    {
                        json.WriteStartObject();
                        json.WriteString("family", item.Family);
                        json.WriteString("key", item.Key);
                        json.WriteString("state", item.State);
                        json.WriteBoolean("visible", item.Visible);
                        json.WriteEndObject();
                    }
                    json.WriteEndArray();
                    json.WriteEndObject();
                }
                json.WriteEndArray();
            }

            private sealed record ViewToken(int Id);
            private sealed record Resource(string Family, string Key, string State, bool Visible);
            private sealed record Note(string Key, int Depth, bool IsDraft, int View, bool Reading, ulong Revision, ulong Generation);
            private sealed record Row(long Ticks, string Phase, string Kind, ulong Revision, ulong Layout, ulong Offset,
                bool Reading, ulong Accepted, long Full, long Delta, Note[] Notes, Resource[] Resources);
        }
    }
}

namespace Notepads.Presentation.Controls.Markdown
{
    public sealed partial class MarkdownExtensionView
    {
        internal Func<bool> TestCaptureNoteLinkAction(MarkdownView child)
        {
            var note = _noteHosts[child];
            var request = BeginNoteLinkRequest(note);
            return () => IsCurrentNoteLink(note, request);
        }

        internal (string Key, int Depth, bool IsDraft, MarkdownView View)[] TestReadLabReplayNotes()
        {
            var result = new List<(string, int, bool, MarkdownView)>();
            Read(_notes, "");
            return result.ToArray();

            void Read(Dictionary<uint, MountedNote> notes, string parent)
            {
                var counts = new Dictionary<string, int>(StringComparer.Ordinal);
                foreach (var note in notes.Values.OrderBy(note => note.Slot.ObjectId))
                {
                    var target = note.Slot.Target;
                    var ordinal = counts.GetValueOrDefault(target);
                    counts[target] = ordinal + 1;
                    var key = parent + "/" + target + "/" + ordinal;
                    result.Add((key, note.Depth, note.DraftRevision.HasValue || target.StartsWith('#'), note.Child));
                    Read(note.Children, key);
                }
            }
        }
    }
}
