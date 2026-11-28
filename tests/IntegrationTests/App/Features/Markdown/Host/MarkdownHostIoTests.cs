// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Infrastructure.DocumentAssets;
using Windows.Storage;
using Windows.Storage.Streams;

namespace NotepadsIntegrationTests;

internal static class MarkdownHostIoTests
{
    internal static async Task RunAsync(StringBuilder log)
    {
        string root = Path.Combine(ApplicationData.Current.LocalFolder.Path, "MarkdownHostIo", Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(root);
        try
        {
            await NotificationAsync(root);
            await SessionRetirementAsync(root, log);
            await ProtocolMismatchAsync(root, log);
            await ReplyPublicationAsync(root, log);
            await FileOwnerAsync(root, log);
            log.AppendLine("PASS: Markdown.HostIo — real rename notifications, request cancellation, independent read-only views and final physical owner retirement.");
        }
        finally
        {
            if (Directory.Exists(root)) Directory.Delete(root, true);
        }
    }

    private static async Task NotificationAsync(string root)
    {
        string directory = Path.Combine(root, "notifications");
        Directory.CreateDirectory(directory);
        using var changes = new DocumentAssetClient.SessionChanges(directory);
        using var deadline = new CancellationTokenSource(TimeSpan.FromSeconds(5));
        string reply = Path.Combine(directory, "reply");
        Task wait = changes.WaitForFileAsync(reply, deadline.Token);
        string partial = Path.Combine(directory, "partial");
        await File.WriteAllTextAsync(partial, "published", deadline.Token);
        File.Move(partial, reply);
        await wait;
        await changes.WaitForFileAsync(reply, deadline.Token);
        using var cancel = new CancellationTokenSource();
        Task canceled = changes.WaitForFileAsync(Path.Combine(directory, "absent"), cancel.Token);
        cancel.Cancel();
        try { await canceled; throw new InvalidOperationException("Notification wait ignored cancellation."); }
        catch (OperationCanceledException) when (cancel.IsCancellationRequested) { }
        Task retired = changes.WaitForFileAsync(Path.Combine(directory, "retired"), deadline.Token);
        changes.Dispose();
        try { await retired; throw new InvalidOperationException("Disposed notification wait did not retire."); }
        catch (IOException) { }
    }

    private static async Task SessionRetirementAsync(string root, StringBuilder log)
    {
        string directory = Path.Combine(root, "retiring");
        Directory.CreateDirectory(directory);
        string path = Path.Combine(directory, "lease");
        using var lease = File.Open(path, FileMode.CreateNew, FileAccess.ReadWrite, FileShare.None);
        var id = Guid.NewGuid().ToString("N");
        foreach (string file in new[] { "ready", "ready.partial", "close", id + ".partial", id + ".request",
            id + ".reply", id + ".refused" })
            File.WriteAllText(Path.Combine(directory, file), "orphaned");
        using var cancellation = new CancellationTokenSource();
        Task canceled = DocumentAssetClient.TestWaitForRetirementAsync(directory, cancellation.Token);
        Check(!canceled.IsCompleted, "The broker waits while the previous worker still holds its lease.");
        cancellation.Cancel();
        try { await canceled; throw new InvalidOperationException("Broker lease wait ignored cancellation."); }
        catch (OperationCanceledException) when (cancellation.IsCancellationRequested) { }

        using var deadline = new CancellationTokenSource(TimeSpan.FromSeconds(5));
        Task next = DocumentAssetClient.TestWaitForRetirementAsync(directory, deadline.Token);
        Check(!next.IsCompleted, "A subsequent request also waits for the retiring worker.");
        lease.Dispose();
        await next;
        Check(!Directory.Exists(directory), "A worker exit removes its lease and all stale protocol files once no worker holds it.");

        Directory.CreateDirectory(directory);
        await DocumentAssetClient.TestWaitForRetirementAsync(directory, deadline.Token);
        Check(!Directory.Exists(directory), "The final retirement probe removes an empty directory with no lease.");

        Directory.CreateDirectory(directory);
        string close = Path.Combine(directory, "close");
        File.WriteAllText(close, "");
        await DocumentAssetClient.TestWaitForRetirementAsync(directory, deadline.Token);
        Check(File.Exists(close), "An activation that never connected keeps its close marker for a late worker.");
        await DocumentAssetClient.TestWaitForRetirementAsync(directory, deadline.Token, connected: true);
        Check(!Directory.Exists(directory), "A late close marker is reclaimed after a connected worker removes its lease.");

        Directory.CreateDirectory(directory);
        File.WriteAllText(path, "");
        File.WriteAllText(Path.Combine(directory, "unrelated.reply"), "keep");
        string subdirectory = Path.Combine(directory, "ready");
        Directory.CreateDirectory(subdirectory);
        await DocumentAssetClient.TestWaitForRetirementAsync(directory, deadline.Token);
        Check(!File.Exists(path) && File.Exists(Path.Combine(directory, "unrelated.reply")) && Directory.Exists(subdirectory),
            "Broker retirement leaves unrelated files and directories untouched.");
        log.AppendLine("PASS: Markdown.BrokerLeaseRetirement waits on a live worker, reclaims crashed session files and preserves foreign entries.");
    }

    private static async Task FileOwnerAsync(string root, StringBuilder log)
    {
        string directory = Path.Combine(root, "owners");
        Directory.CreateDirectory(directory);
        // Keep the session directory alive as a real broker's ready/lease files do.
        File.WriteAllText(Path.Combine(directory, "ready"), "test");
        string path = Path.Combine(directory, "asset.reply");
        File.WriteAllBytes(path, Array.Empty<byte>());
        using (IRandomAccessStream empty = DocumentAssetClient.OpenPublishedReply(path, 4))
        {
            Check(empty != null && empty.CanRead && empty.Size == 0 && (await ReadAsync(empty, 1)).Length == 0,
                "A successful empty broker reply is an ordinary readable EOF stream.");
            using IRandomAccessStream emptyClone = empty.CloneStream();
            empty.Dispose();
            Check(File.Exists(path) && emptyClone.Size == 0 && (await ReadAsync(emptyClone, 1)).Length == 0,
                "An empty clone retains its independent read view and physical reply owner.");
        }
        Check(!File.Exists(path), "The final empty reply owner physically deletes its file.");
        File.WriteAllBytes(path, new byte[] { 11, 22, 33, 44 });
        Check(DocumentAssetClient.OpenPublishedReply(path, 3) == null && File.Exists(path),
            "An oversized broker reply also remains visible until the request retires.");
        using IRandomAccessStream stream = DocumentAssetClient.OpenPublishedReply(path, 4);
        Check(stream != null && stream.CanRead && !stream.CanWrite && stream.Size == 4, "Read-only reply properties.");
        stream.Seek(2);
        using IRandomAccessStream clone = stream.CloneStream();
        using IInputStream input = stream.GetInputStreamAt(1);
        Check(clone.Position == 0 && stream.Position == 2, "Clones start at zero independently.");
        Check((await ReadAsync(clone, 1)).SequenceEqual(new byte[] { 11 }) && clone.Position == 1 && stream.Position == 2,
            "Clone position is independent.");
        Check((await ReadAsync(input, 1, InputStreamOptions.Partial)).SequenceEqual(new byte[] { 22 }) && stream.Position == 2,
            "Input position is independent.");
        stream.Dispose();
        Check(File.Exists(path), "Original Close must retain live clone/input owners.");
        Check((await ReadAsync(input, 1)).SequenceEqual(new byte[] { 33 }), "Input survives original Close.");
        input.Dispose();
        Check(File.Exists(path), "Input Close must retain live clone owner.");
        Check((await ReadAsync(clone, 8, InputStreamOptions.ReadAhead)).SequenceEqual(new byte[] { 22, 33, 44 }),
            "Clone survives other Close calls and returns actual EOF length.");
        clone.Seek(100);
        Check((await ReadAsync(clone, 1)).Length == 0 && clone.Position == 100, "Seek beyond EOF is bounded.");
        Expect<ArgumentOutOfRangeException>(() => clone.Seek(ulong.MaxValue));
        Expect<ArgumentOutOfRangeException>(() => clone.GetInputStreamAt(ulong.MaxValue));
        Expect<NotSupportedException>(() => clone.Size = 5);
        Expect<NotSupportedException>(() => clone.GetOutputStreamAt(0));
        clone.Dispose();
        Check(!File.Exists(path), "Final owner Close must physically delete reply.");
        File.WriteAllBytes(path, new byte[4]);
        Check(DocumentAssetRandomAccessStream.Open(path, 3) == null && !File.Exists(path), "Oversized reply is closed and deleted.");

        int pending = 0;
        for (int round = 0; round < 4; round++)
        {
            using (var create = new FileStream(path, FileMode.CreateNew, FileAccess.Write)) create.SetLength(16 * 1024 * 1024);
            var owner = new DocumentAssetFileOwner(path);
            var view = new DocumentAssetFileView(owner, 0);
            using var cancel = new CancellationTokenSource();
            ValueTask<int> read = view.ReadAsync(new byte[16 * 1024 * 1024], cancel.Token);
            view.Dispose();
            bool exists = File.Exists(path);
            bool completed = read.IsCompleted;
            Check(exists || completed, "An actual read must retain the file until it retires.");
            if (!completed) pending++;
            if ((round & 1) == 0) cancel.Cancel();
            try { await read; }
            catch (OperationCanceledException) when (cancel.IsCancellationRequested) { }
            Check(!File.Exists(path), "Read completion/cancellation retires final physical owner.");
        }
        log.AppendLine($"OBS: Markdown.HostIo actual pending file reads sampled after view Close={pending}/4.");
    }

    private static async Task ProtocolMismatchAsync(string root, StringBuilder log)
    {
        string documentPath = Path.Combine(root, "protocol.md");
        File.WriteAllText(documentPath, "# Protocol fixture");
        var document = await StorageFile.GetFileFromPathAsync(documentPath);
        foreach (string ready in new[] { "2\nFiles.ReadDocumentAsset\n", "Files.ReadDocument", string.Empty })
        {
            var broker = new DocumentAssetBrokerFixture(_ => throw new InvalidOperationException(
                "An incompatible broker must never receive a request."), ready);
            try
            {
                using var deadline = new CancellationTokenSource(TimeSpan.FromSeconds(10));
                using var rejected = await DocumentAssetClient.ReadDocumentAssetAsync(document, "note.md", 1024, deadline.Token);
                Check(rejected == null && !DocumentAssetClient.TestHasActiveSession && broker.Requests.IsEmpty &&
                    broker.Sessions.Count == 1, "Stale, truncated and empty ready payloads return fallback without retaining a session.");
                var session = broker.Sessions.Single();
                try
                {
                    await session.Changes.WaitForFileAsync(Path.Combine(session.Directory, "absent"), deadline.Token);
                    throw new InvalidOperationException("The mismatched session kept its watcher live.");
                }
                catch (IOException) { }
            }
            finally { await broker.DisposeAsync(); }
            Check(broker.Sessions.All(session => !Directory.Exists(session.Directory)),
                "A rejected protocol closes the worker lease and retires the entire owned session directory.");
        }
        log.AppendLine("PASS: Markdown.BrokerProtocolMismatch — stale numeric, truncated and empty handshakes return null, close the real watcher, release the gate and retire owned session directories.");
    }

    private static async Task ReplyPublicationAsync(string root, StringBuilder log)
    {
        string documentPath = Path.Combine(root, "replies.md");
        File.WriteAllText(documentPath, "# Reply fixture");
        var document = await StorageFile.GetFileFromPathAsync(documentPath);
        await using var broker = new DocumentAssetBrokerFixture(relative => relative switch
        {
            "empty.md" => Array.Empty<byte>(),
            "missing.md" => null,
            "oversized.md" => new byte[] { 1, 2, 3 },
            "note.md" => new byte[] { 11, 22 },
            _ => throw new InvalidOperationException("Unexpected broker fixture request: " + relative)
        });
        using var deadline = new CancellationTokenSource(TimeSpan.FromSeconds(10));
        using (var empty = await DocumentAssetClient.ReadDocumentAssetAsync(document, "empty.md", 2, deadline.Token))
            Check(empty != null && empty.Size == 0 && (await ReadAsync(empty, 1)).Length == 0,
                "The real request path accepts a renamed zero-byte successful payload.");
        using (var missing = await DocumentAssetClient.ReadDocumentAssetAsync(document, "missing.md", 2, deadline.Token))
            Check(missing == null, "A refusal marker returns fallback instead of an empty successful stream.");
        using (var oversized = await DocumentAssetClient.ReadDocumentAssetAsync(document, "oversized.md", 2, deadline.Token))
            Check(oversized == null, "A successful payload still obeys the caller's exact encoded byte limit.");
        using (var next = await DocumentAssetClient.ReadDocumentAssetAsync(document, "note.md", 2, deadline.Token))
            Check(next != null && (await ReadAsync(next, 3)).SequenceEqual(new byte[] { 11, 22 }),
                "The same live session serves a bounded success after empty, refused and oversized replies.");
        Check(broker.Sessions.Count == 1 && broker.Requests.Count == 4 &&
            broker.Requests.All(message => message.GetNamedString("operation") == "Files.ReadDocumentAsset" &&
                !message.ContainsKey("version")), "Requests use one semantic operation contract without numeric protocol variants.");
        string directory = broker.Sessions.Single().Directory;
        Check(!Directory.GetFiles(directory).Any(path => Path.GetExtension(path) is ".partial" or ".request" or ".reply" or ".refused"),
            "Every completed request removes its staging, request, payload/refusal files after physical stream retirement.");
        log.AppendLine("PASS: Markdown.BrokerReplyPublication — actual client requests distinguish empty success/refusal, preserve limits and reuse the session without per-request file leaks.");
    }

    private static async Task<byte[]> ReadAsync(IInputStream stream, uint count, InputStreamOptions options = InputStreamOptions.None)
    {
        IBuffer buffer = await stream.ReadAsync(new Windows.Storage.Streams.Buffer(count), count, options);
        return buffer.ToArray();
    }

    private static void Expect<T>(Action action) where T : Exception
    {
        try { action(); }
        catch (T) { return; }
        throw new InvalidOperationException("Expected " + typeof(T).Name);
    }

    private static void Check(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }
}
