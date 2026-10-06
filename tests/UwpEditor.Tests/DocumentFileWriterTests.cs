// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents.IO;
using Notepads.Infrastructure.Storage;
using Windows.Storage;
using Windows.Storage.Provider;

namespace NotepadsEditorTests;

internal static class DocumentFileWriterTests
{
    public static async Task RunAsync(StringBuilder log)
    {
        var folder = await ApplicationData.Current.LocalFolder.CreateFolderAsync(
            "DocumentFileWriterTests-" + Guid.NewGuid().ToString("N"), CreationCollisionOption.FailIfExists);
        try
        {
            var file = await folder.CreateFileAsync("document", CreationCollisionOption.FailIfExists);
            var original = new UTF8Encoding(false, true).GetBytes("previous complete file\0中文😀\r");
            var replacement = new UTF8Encoding(false, true).GetBytes("new العربية\0😀\r");
            await AtomicFileWriter.WriteAsync(file, stream => WriteAsync(stream, original));

            var updates = new TestFileUpdates();
            var attempts = 0;
            var result = await DocumentFileWriter.WriteAsync(file, async stream =>
            {
                attempts++;
                await WriteAsync(stream, replacement);
                if (attempts == 1) throw SharingFailure();
            }, updates);
            Assert(result.File.IsEqual(file) && attempts == 2 && updates.Deferred == 1 && updates.Completed == 1,
                "A retried atomic save lost its file identity or provider deferral ownership.");
            await AssertBytesAsync(file, replacement);

            updates = new TestFileUpdates();
            attempts = 0;
            await ExpectAsync<COMException>(() => DocumentFileWriter.WriteAsync(file, async stream =>
            {
                attempts++;
                await WriteAsync(stream, original);
                throw SharingFailure();
            }, updates));
            Assert(attempts > 1 && attempts <= 5 && updates.Deferred == 1 && updates.Completed == 1,
                "Transient write retries were unbounded or leaked a provider deferral.");
            await AssertBytesAsync(file, replacement);

            using (var cancellation = new CancellationTokenSource())
            {
                updates = new TestFileUpdates();
                await ExpectAsync<OperationCanceledException>(() => DocumentFileWriter.WriteAsync(file, async stream =>
                {
                    await WriteAsync(stream, original);
                    cancellation.Cancel();
                }, updates, cancellation.Token));
                Assert(updates.Completed == 1, "Canceled candidate preparation leaked its provider deferral.");
                await AssertBytesAsync(file, replacement);
            }

            updates = new TestFileUpdates { FailCompletion = true };
            attempts = 0;
            result = await DocumentFileWriter.WriteAsync(file, stream =>
            {
                attempts++;
                return WriteAsync(stream, original);
            }, updates);
            Assert(attempts == 1 && result.UpdateStatus == FileUpdateStatus.Incomplete &&
                !string.IsNullOrEmpty(result.ProviderError),
                "Provider completion failure retried or reported failure for an already committed local file.");
            await AssertBytesAsync(file, original);

            updates = new TestFileUpdates { FailCompletion = true };
            await ExpectAsync<InvalidDataException>(() => DocumentFileWriter.WriteAsync(file, async stream =>
            {
                await WriteAsync(stream, replacement);
                throw new InvalidDataException("Injected candidate validation failure.");
            }, updates));
            await AssertBytesAsync(file, original);

            updates = new TestFileUpdates
            {
                CompletionStatus = FileUpdateStatus.CompleteAndRenamed,
                ProviderMutation = async savedFile => await savedFile.RenameAsync("renamed-document", NameCollisionOption.FailIfExists)
            };
            result = await DocumentFileWriter.WriteAsync(file, stream => WriteAsync(stream, replacement), updates);
            Assert(result.File.Name == "renamed-document" && result.UpdateStatus == FileUpdateStatus.CompleteAndRenamed &&
                result.ProviderError == null, "The save result lost the provider-renamed file identity.");
            await AssertBytesAsync(result.File, replacement);

            updates = new TestFileUpdates { FailDefer = true };
            result = await DocumentFileWriter.WriteAsync(file, stream => WriteAsync(stream, original), updates);
            Assert(result.UpdateStatus == null && updates.Completed == 0,
                "An unavailable provider deferral prevented a local commit or acquired unowned provider state.");
            await AssertBytesAsync(file, original);

            using (var cancellation = new CancellationTokenSource())
            {
                updates = new TestFileUpdates { AfterCommit = cancellation.Cancel };
                result = await DocumentFileWriter.WriteAsync(file, stream => WriteAsync(stream, replacement), updates, cancellation.Token);
                Assert(cancellation.IsCancellationRequested && result.UpdateStatus == FileUpdateStatus.Complete,
                    "Cancellation after local commit hid a successfully saved file.");
                await AssertBytesAsync(file, replacement);
            }

            updates = new TestFileUpdates { CompletionStatus = FileUpdateStatus.UserInputNeeded };
            result = await DocumentFileWriter.WriteAsync(file, stream => WriteAsync(stream, original), updates);
            Assert(result.UpdateStatus == FileUpdateStatus.UserInputNeeded && result.ProviderError != null,
                "An incomplete provider update was silently treated as synchronized.");
            await AssertBytesAsync(file, original);

            using (var cancellation = new CancellationTokenSource())
            {
                cancellation.Cancel();
                updates = new TestFileUpdates();
                await ExpectAsync<OperationCanceledException>(() => DocumentFileWriter.WriteAsync(file,
                    stream => throw new Exception("A pre-canceled request wrote a candidate."), updates, cancellation.Token));
                Assert(updates.Deferred == 0 && updates.Completed == 0, "A pre-canceled request acquired provider state.");
                await AssertBytesAsync(file, original);
            }

            log.AppendLine("PASS: document file transactions preserve committed bytes on retry/validation/cancel, balance provider ownership, and distinguish local save from pending synchronization.");
        }
        finally { await folder.DeleteAsync(StorageDeleteOption.PermanentDelete); }
    }

    private static Task WriteAsync(Stream stream, byte[] bytes) => stream.WriteAsync(bytes, 0, bytes.Length);

    private static COMException SharingFailure() => new("Injected transient sharing failure.", unchecked((int)0x80070020));

    private static async Task AssertBytesAsync(StorageFile file, byte[] expected)
    {
        using (var stream = await file.OpenStreamForReadAsync())
        using (var buffer = new MemoryStream())
        {
            await stream.CopyToAsync(buffer);
            Assert(buffer.ToArray().SequenceEqual(expected), "A failed save changed the last committed file or a successful save changed Unicode/NUL bytes.");
        }
    }

    private static async Task ExpectAsync<T>(Func<Task> operation) where T : Exception
    {
        try { await operation(); }
        catch (T) { return; }
        throw new Exception("Expected " + typeof(T).Name + " was not raised.");
    }

    private static void Assert(bool condition, string message)
    {
        if (!condition) throw new Exception(message);
    }

    private sealed class TestFileUpdates : IDocumentFileUpdates
    {
        public int Deferred;
        public int Completed;
        public bool FailDefer;
        public bool FailCompletion;
        public Action AfterCommit;
        public Func<StorageFile, Task> ProviderMutation;
        public FileUpdateStatus CompletionStatus = FileUpdateStatus.Complete;

        public void Defer(StorageFile file)
        {
            Deferred++;
            if (FailDefer) throw new IOException("Injected unavailable provider deferral.");
        }

        public async Task<FileUpdateStatus> CompleteAsync(StorageFile file)
        {
            Completed++;
            AfterCommit?.Invoke();
            if (ProviderMutation != null) await ProviderMutation(file);
            if (FailCompletion) throw new IOException("Injected provider synchronization failure.");
            return CompletionStatus;
        }
    }
}
