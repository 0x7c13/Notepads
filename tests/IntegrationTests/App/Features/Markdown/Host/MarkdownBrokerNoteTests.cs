// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Concurrent;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using CommunityToolkit.WinUI;
using Notepads.Infrastructure.DocumentAssets;
using Notepads.Markdown;
using Windows.Storage;
using Windows.Storage.Streams;
using Windows.UI.Xaml.Controls;
using static NotepadsWorkspaceTests.DocumentFeatureAssertions;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal static partial class MarkdownFeatureTests
{
    internal static async Task RunBrokerNotesAsync(StringBuilder log)
    {
        await BrokerNoteDescendantsAsync(log);
        await BrokerNoteProtocolFallbackAsync(log);
        await BrokerEmptyNoteAsync(log);
        await BrokerNoteContainmentAsync(log);
        await BrokerNoteCancellationAsync(log);
    }

    private static async Task BrokerNoteDescendantsAsync(StringBuilder log)
    {
        var host = await FeatureTestHost.CreateAsync();
        var requests = new ConcurrentQueue<string>();
        var violations = new ConcurrentQueue<string>();
        try
        {
            const string markdown = "# Root file\n\n![[Notes/Private note#Chosen]]\n\n![[Local fallback]]";
            var editor = await OpenAsync(host, "broker-root.md", Encoding.UTF8.GetBytes(markdown));
            var root = editor.EditingFile;
            var fallback = await host.Folder.CreateFileAsync("Local fallback.md");
            await FileIO.WriteTextAsync(fallback, "# Local fallback\n\nOrdinary folder fallback remains available.");
            const string privateSource = "# Before\n\nOutside before.\n\n## Chosen\n\n" +
                "Broker-only UTF-16 text 中文😀.\n\n![Child broker pixel](icon.png)\n\n" +
                "![[Nested]]\n\n![[#Excerpt]]\n\n## Excerpt\n\nPrivate same-note excerpt.\n\n" +
                "## After\n\nOutside after.";
            byte[] privateBytes = Encoding.Unicode.GetPreamble().Concat(Encoding.Unicode.GetBytes(privateSource)).ToArray();
            byte[] nestedBytes = Encoding.UTF8.GetBytes("# Nested\n\nBroker-only nested text.");
            byte[] imageBytes = Convert.FromBase64String(PngBase64);

            DocumentAssetClient.TestRead = async (document, relative, maximumBytes, cancellation) =>
            {
                requests.Enqueue(relative);
                if (document == null || !document.IsEqual(root))
                    violations.Enqueue("A descendant request replaced the original StorageFile grant: " + relative);
                if (maximumBytes is 0 or > 16 * 1024 * 1024 ||
                    relative.EndsWith(".md", StringComparison.Ordinal) && maximumBytes > 4 * 1024 * 1024)
                    violations.Enqueue("A broker request exceeded its bounded host input limit: " + relative);
                byte[] bytes = relative switch
                {
                    "Notes\\Private note.md" => privateBytes,
                    "Notes\\Nested.md" => nestedBytes,
                    "Notes\\icon.png" => imageBytes,
                    "Local fallback.md" => null,
                    _ => Unexpected(relative)
                };
                cancellation.ThrowIfCancellationRequested();
                return bytes == null || maximumBytes < (ulong)bytes.Length ? null :
                    await CreateInMemoryTestStreamAsync(bytes, cancellation);
            };

            byte[] Unexpected(string relative)
            {
                violations.Enqueue("An unexpected or escaping broker path was requested: " + relative);
                return null;
            }

            void CheckRequests() => Check(violations.IsEmpty, string.Join(Environment.NewLine, violations));
            Check(!Directory.Exists(Path.Combine(host.Folder.Path, "Notes")),
                "The private notes and image have no direct StorageFolder/path fallback on disk.");
            editor.ShowHideContentPreview();
            var extension = await CurrentAsync(editor);
            var preview = extension.PreviewView;
            MarkdownView child = null;
            try
            {
                await UntilAsync(() =>
                {
                    CheckRequests();
                    child = BrokerChildren(preview).SingleOrDefault(view =>
                        view.DisplayedText.Contains("Broker-only UTF-16 text 中文😀.", StringComparison.Ordinal));
                    return child != null && BrokerChildren(child).Any(view =>
                        view.DisplayedText.Contains("Broker-only nested text.", StringComparison.Ordinal)) &&
                        ImageState(child, "Child broker pixel", true) &&
                        BrokerChildren(preview).Any(view => view.DisplayedText.Contains("Ordinary folder fallback", StringComparison.Ordinal));
                }, "The real host decodes broker-only UTF-16 notes, nested notes and relative images.");
            }
            catch (TimeoutException)
            {
                log.AppendLine($"OBS: Broker notes complete={extension.TestRootNoteLoadsCompleted}; notes=[{extension.TestRootNoteLoadSummary}].");
                LogViewportDiagnostics(log, preview, "broker note root", includeParts: true);
                foreach (var mounted in BrokerChildren(preview))
                {
                    log.AppendLine($"OBS: Broker child text=[{mounted.DisplayedText}], imageReady={ImageState(mounted, "Child broker pixel", true)}.");
                    LogViewportDiagnostics(log, mounted, "broker note child", includeParts: true);
                    foreach (var nested in BrokerChildren(mounted))
                    {
                        log.AppendLine($"OBS: Broker nested text=[{nested.DisplayedText}].");
                        LogViewportDiagnostics(log, nested, "broker note nested child", includeParts: true);
                    }
                }
                throw;
            }
            CheckRequests();
            Check(child.DisplayedText.StartsWith("Chosen", StringComparison.Ordinal) &&
                !child.DisplayedText.Contains("Outside", StringComparison.Ordinal) &&
                BrokerChildren(preview).Count() == 2,
                "Only the selected section and the ordinary fallback mount in the root.");
            Check(child.TrySetVerticalScrollProgress(1), "The child viewport scrolls to the same-note heading embed.");
            await UntilAsync(() =>
            {
                CheckRequests();
                return BrokerChildren(child).Any(view =>
                    view.DisplayedText.Contains("Private same-note excerpt.", StringComparison.Ordinal));
            }, "A broker-only note rereads its own stream to mount another heading of that same note.");
            Check(requests.Count(path => path == "Notes\\Private note.md") >= 2 &&
                requests.Contains("Notes\\Nested.md") && requests.Contains("Notes\\icon.png") &&
                requests.Contains("Local fallback.md"),
                "Every broker-only descendant retains the original grant and its full root-relative path.");
            Check(editor.GetText() == markdown.Replace("\n", "\r", StringComparison.Ordinal) &&
                !Directory.Exists(Path.Combine(host.Folder.Path, "Notes")),
                "Mounting broker streams preserves the root source and creates no sibling grants or files.");

            Check(CoreValidationProbe.HasNoteEmbedParentPin(child), "A mounted note owns its parent model pin.");
            var other = await OpenAsync(host, "broker-root-other.txt", Encoding.UTF8.GetBytes("Another tab"));
            await UntilAsync(() => ReferenceEquals(host.Core.GetSelectedReadyEditor(), other) && !BrokerChildren(preview).Any(),
                "Switching tabs detaches the mounted notes from their parent preview.");
            Check(!CoreValidationProbe.HasNoteEmbedParentPin(child),
                "Detaching a note releases its parent model pin while the child is still held.");
            log.AppendLine("PASS: Markdown.BrokerNoteParentPin releases the parent model when the hosted child detaches.");
            log.AppendLine("PASS: Markdown.BrokerNoteHost uses a test-only stream seam through the real host for original-grant paths, UTF-16, nested notes, relative images, same-note headings and ordinary fallback.");
            log.AppendLine("OBS: This phase does not claim real Windows file-only authorization or full-trust broker activation.");
        }
        catch
        {
            log.AppendLine("OBS: Broker note host requests: " + string.Join(", ", requests));
            foreach (string violation in violations) log.AppendLine("OBS: " + violation);
            throw;
        }
        finally
        {
            try { await host.DisposeAsync(); }
            finally { DocumentAssetClient.TestRead = null; }
        }
    }

    private static async Task BrokerNoteContainmentAsync(StringBuilder log)
    {
        var host = await FeatureTestHost.CreateAsync();
        var requests = new ConcurrentQueue<string>();
        try
        {
            DocumentAssetClient.TestRead = (_, relative, _, _) =>
            {
                requests.Enqueue(relative);
                return Task.FromResult<IRandomAccessStream>(null);
            };
            foreach (var fixture in new[] { ("escape.md", "../Outside", 1), ("absolute.md", "C:/Outside.md", 0) })
            {
                var editor = await OpenAsync(host, fixture.Item1, Encoding.UTF8.GetBytes("# Rejected\n\n![[" + fixture.Item2 + "]]"));
                editor.ShowHideContentPreview();
                var extension = await CurrentAsync(editor);
                await UntilAsync(() => extension.PreviewView.GetVisibleNoteEmbeds().Count == fixture.Item3 &&
                    extension.TestRootNoteLoadsCompleted, "The rejected target finishes parsing/host admission: " + fixture.Item2);
                Check(requests.IsEmpty && !BrokerChildren(extension.PreviewView).Any(),
                    "Absolute and escaping targets never widen broker authority or mount a child.");
            }
            log.AppendLine("PASS: Markdown.BrokerNoteContainment rejects parent traversal at the host and absolute note syntax at parsing before any broker request.");
        }
        finally
        {
            try { await host.DisposeAsync(); }
            finally { DocumentAssetClient.TestRead = null; }
        }
    }

    private static async Task BrokerNoteProtocolFallbackAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var fallback = await host.Folder.CreateFileAsync("Protocol fallback.md");
        await FileIO.WriteTextAsync(fallback, "# Local protocol fallback\n\nReadable through the existing StorageFolder grant.");
        var editor = await OpenAsync(host, "broker-protocol-root.md", Encoding.UTF8.GetBytes("![[Protocol fallback]]"));
        await using var broker = new DocumentAssetBrokerFixture(_ => throw new InvalidOperationException(
            "An incompatible broker must not read any asset."), "2\nFiles.ReadDocumentAsset\n");
        editor.ShowHideContentPreview();
        var extension = await CurrentAsync(editor);
        await UntilAsync(() => extension.TestRootNoteLoadsCompleted && BrokerChildren(extension.PreviewView).Any(child =>
            child.DisplayedText.Contains("Readable through the existing StorageFolder grant.", StringComparison.Ordinal)),
            "A mismatched ready payload returns through the real client failure path to the existing StorageFolder fallback.");
        Check(!DocumentAssetClient.TestHasActiveSession && broker.Sessions.Count == 1 && broker.Requests.IsEmpty,
            "Protocol fallback retains no active watcher/session and submits no request to the incompatible helper.");
        log.AppendLine("PASS: Markdown.BrokerNoteProtocolFallback — an incompatible helper releases its client session and the real note host uses its existing StorageFolder authorization.");
    }

    private static async Task BrokerEmptyNoteAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        const string source = "# Root\n\n![[Empty]]\n\n![[Missing]]";
        var editor = await OpenAsync(host, "broker-empty-root.md", Encoding.UTF8.GetBytes(source));
        Check(!File.Exists(Path.Combine(host.Folder.Path, "Empty.md")) &&
            !File.Exists(Path.Combine(host.Folder.Path, "Missing.md")),
            "The empty success and refusal have no direct filesystem fallback.");
        await using var broker = new DocumentAssetBrokerFixture(relative => relative switch
        {
            "Empty.md" => Array.Empty<byte>(),
            "Missing.md" => null,
            _ => throw new InvalidOperationException("Unexpected empty-note request: " + relative)
        });
        editor.ShowHideContentPreview();
        var extension = await CurrentAsync(editor);
        try
        {
            await UntilAsync(() =>
            {
                var children = BrokerChildren(extension.PreviewView).ToArray();
                return extension.TestRootNoteLoadsCompleted && children.Length == 1 && children[0].GetSelection().IsAvailable &&
                    children[0].DisplayedRevision == 1 && children[0].DisplayedTextLength == 0;
            }, "A successful zero-byte broker payload decodes and publishes one empty child while a refused note mounts none.");
        }
        catch (TimeoutException)
        {
            var preview = extension.PreviewView;
            var requests = broker.Requests.Select(message => message.GetNamedString("relative"));
            log.AppendLine($"OBS: Empty broker note: sessions={broker.Sessions.Count}; requests=[{string.Join(", ", requests)}]; " +
                $"loadsComplete={extension.TestRootNoteLoadsCompleted}; notes=[{extension.TestRootNoteLoadSummary}].");
            LogViewportDiagnostics(log, preview, "empty broker note root", includeParts: true);
            foreach (var child in BrokerChildren(preview))
                LogViewportDiagnostics(log, child, "empty broker note child", includeParts: true);
            throw;
        }
        Check(broker.Requests.Count == 2 && broker.Requests.Select(message => message.GetNamedString("relative"))
            .OrderBy(value => value, StringComparer.Ordinal).SequenceEqual(new[] { "Empty.md", "Missing.md" }) &&
            editor.GetText() == source.Replace("\n", "\r", StringComparison.Ordinal),
            "Empty note rendering preserves root text and distinguishes the exact successful/refused requests.");
        log.AppendLine("PASS: Markdown.BrokerEmptyNote — a process-free activation seam exercises the actual client, stream owner, decoder and mounted child for an empty payload; a separate refusal mounts no child. Native broker activation/containment remains separately qualified.");
    }

    private static async Task BrokerNoteCancellationAsync(StringBuilder log)
    {
        var host = await FeatureTestHost.CreateAsync();
        var entered = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var retired = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        try
        {
            var editor = await OpenAsync(host, "broker-cancel.md", Encoding.UTF8.GetBytes("# Pending\n\n![[Cancelled]]"));
            var root = editor.EditingFile;
            DocumentAssetClient.TestRead = async (document, relative, maximumBytes, cancellation) =>
            {
                Check(document != null && document.IsEqual(root) && relative == "Cancelled.md" &&
                    maximumBytes is > 0 and <= 4 * 1024 * 1024,
                    "The pending note uses its original grant, exact path and source limit.");
                entered.TrySetResult();
                try { await Task.Delay(Timeout.Infinite, cancellation); }
                finally { retired.TrySetResult(); }
                return null;
            };
            editor.ShowHideContentPreview();
            var preview = (await CurrentAsync(editor)).PreviewView;
            await entered.Task.WaitAsync(TimeSpan.FromSeconds(10));
            await OpenAsync(host, "broker-cancel-other.txt", Encoding.UTF8.GetBytes("Another tab"));
            await retired.Task.WaitAsync(TimeSpan.FromSeconds(10));
            await UntilAsync(() => !BrokerChildren(preview).Any(),
                "Hiding the parent cancels its actual host broker read and publishes no late child.");
            log.AppendLine("PASS: Markdown.BrokerNoteCancellation cancels an in-flight host stream request when its parent tab hides.");
        }
        finally
        {
            try { await host.DisposeAsync(); }
            finally { DocumentAssetClient.TestRead = null; }
        }
    }

    private static System.Collections.Generic.IEnumerable<MarkdownView> BrokerChildren(MarkdownView preview) =>
        (preview.FindDescendant("ViewportOverlay") as Canvas)?.Children.OfType<MarkdownView>() ??
            Enumerable.Empty<MarkdownView>();

    private static async Task<IRandomAccessStream> CreateInMemoryTestStreamAsync(byte[] bytes, CancellationToken cancellation)
    {
        var stream = new InMemoryRandomAccessStream();
        try
        {
            await stream.WriteAsync(bytes.AsBuffer());
            cancellation.ThrowIfCancellationRequested();
            stream.Seek(0);
            return stream;
        }
        catch { stream.Dispose(); throw; }
    }
}
