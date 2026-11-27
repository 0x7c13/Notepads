// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Documents.IO;
using Notepads.Infrastructure.Storage;
using Windows.Storage;
using Windows.Storage.Provider;

namespace NotepadsEditorTests;

internal static partial class DocumentFileWriterTests
{
    private static async Task VerifyOrdinaryTargetLocationAsync(StorageFolder folder, StringBuilder log)
    {
        var parent = await folder.CreateFolderAsync("ordinary-parent");
        var file = await parent.CreateFileAsync("ordinary.txt");
        var oldBytes = "original"u8.ToArray();
        await FileIO.WriteBytesAsync(file, oldBytes);
        var version = await DocumentSaveTarget.ReadVersionAsync(file, default);
        var path = parent.Path;
        var parked = Path.Combine(folder.Path, "ordinary-parked");
        using (var pin = DocumentSaveTarget.Pin(file, version.TargetPath))
        {
            bool blocked = false;
            try { Directory.Move(path, parked); }
            catch (Exception ex) when (ex.HResult is unchecked((int)0x80070020) or unchecked((int)0x80070005)) { blocked = true; }
            if (!blocked) Directory.Move(parked, path);
            Assert(blocked, "An ordinary local parent must remain in place between validation and transaction open.");
        }
        Directory.Move(path, parked);
        Directory.Move(parked, path);

        foreach (var (missingTag, hideDos) in new[] { (false, false), (true, false), (false, true), (true, true) })
        {
            DocumentSaveTarget.TestMissingParentTag = missingTag;
            DocumentSaveTarget.TestMissingParentName = !missingTag;
            DocumentSaveTarget.TestHideNormalizedDosPath = hideDos ? _ => true : null;
            try
            {
                var error = await ExpectAsync<DocumentSaveException>(() => Task.Run(() =>
                {
                    using var unexpected = DocumentSaveTarget.Pin(file, version.TargetPath);
                }));
                Assert(error.Stage == DocumentSaveStage.Coordinate,
                    $"Pinned directories require both a verified tag and name (missing tag={missingTag}, hidden DOS name={hideDos}).");
            }
            finally
            {
                DocumentSaveTarget.TestMissingParentTag = DocumentSaveTarget.TestMissingParentName = false;
                DocumentSaveTarget.TestHideNormalizedDosPath = null;
            }
        }
        DocumentSaveTarget.TestPinnedLocation = _ => null;
        try
        {
            var error = await ExpectAsync<DocumentSaveException>(() => Task.Run(() =>
            {
                using var unexpected = DocumentSaveTarget.Pin(file, version.TargetPath, renamingEntry: true);
            }));
            Assert(error.Stage == DocumentSaveStage.Conflict, "Rename must re-observe the grant after pinning the parent.");
        }
        finally { DocumentSaveTarget.TestPinnedLocation = null; }
        Directory.Move(path, parked);
        Directory.Move(parked, path);

        // A provider may lose the observed location even when its staged bytes match.
        DocumentFileWriter.TestTransactionLocation = _ => null;
        try
        {
            await DocumentFileWriter.WriteAsync(file, stream => WriteAsync(stream, "candidate"u8.ToArray()),
                new TestFileUpdates(), expectedVersion: version);
            throw new InvalidOperationException("A transaction with a lost target location committed.");
        }
        catch (DocumentSaveException ex) when (ex.Stage == DocumentSaveStage.Conflict) { }
        finally { DocumentFileWriter.TestTransactionLocation = null; }
        await AssertBytesAsync(file, oldBytes, "lost location before commit");
        using (DocumentSaveTarget.TestPinNamespace(file.Path)) { }
        DocumentSaveTarget.TestAnchorMoved = true;
        try
        {
            var moved = await ExpectAsync<DocumentSaveException>(() => Task.Run(() =>
            {
                using var unexpected = DocumentSaveTarget.TestPinNamespace(file.Path);
            }));
            Assert(moved.Stage == DocumentSaveStage.Conflict,
                "An anchor whose actual pin has moved must reject admission before protecting descendants.");
        }
        finally { DocumentSaveTarget.TestAnchorMoved = false; }
        Directory.Move(path, parked);
        Directory.Move(parked, path);
        await file.DeleteAsync();
        var disappeared = await ExpectAsync<DocumentSaveException>(() => Task.Run(() =>
        {
            using var unexpected = DocumentSaveTarget.Pin(file, version.TargetPath);
        }));
        Assert(disappeared.Stage == DocumentSaveStage.Conflict,
            "A target disappearing between resolution and namespace admission must be a version conflict.");
        log.AppendLine("PASS: ordinary saves protect the local parent, release it after use and roll back a lost target location.");
    }

    public static async Task RunAsync(StringBuilder log)
    {
        var folder = await ApplicationData.Current.LocalFolder.CreateFolderAsync(
            "DocumentFileWriterTests-" + Guid.NewGuid().ToString("N"), CreationCollisionOption.FailIfExists);
        try
        {
            await DocumentVerificationCancellationTests.RunAsync(log);
            await FileLogSinkTests.RunAsync(folder, log);
            await VerifyChunkedFileVersionsAsync(folder);
            await VerifyOrdinaryTargetLocationAsync(folder, log);
            await VerifyTargetCasingAsync(folder, log);
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
            await AssertBytesAsync(file, replacement, "successful retry");
            Assert(result.Version.ModifiedTime == await FileStorage.GetDateModifiedAsync(file), "Matching committed bytes must get a verified version.");

            updates = new TestFileUpdates();
            attempts = 0;
            await ExpectAsync<DocumentSaveException>(() => DocumentFileWriter.WriteAsync(file, async stream =>
            {
                attempts++;
                await WriteAsync(stream, original);
                throw SharingFailure();
            }, updates));
            Assert(attempts > 1 && attempts <= 5 && updates.Deferred == 1 && updates.Completed == 1,
                "Transient write retries were unbounded or leaked a provider deferral.");
            await AssertBytesAsync(file, replacement, "exhausted sharing retries");

            attempts = 0;
            await ExpectAsync<DocumentSaveException>(() => DocumentFileWriter.WriteAsync(file, async stream =>
            {
                attempts++;
                await WriteAsync(stream, original);
                throw new DocumentSaveException(DocumentSaveStage.PrepareSnapshot, new UnauthorizedAccessException());
            }, new TestFileUpdates()));
            Assert(attempts == 1, "A private snapshot failure must not retry the destination transaction.");
            await AssertBytesAsync(file, replacement, "private snapshot denied");

            using (var cancellation = new CancellationTokenSource())
            {
                updates = new TestFileUpdates();
                await ExpectAsync<OperationCanceledException>(() => DocumentFileWriter.WriteAsync(file, async stream =>
                {
                    await WriteAsync(stream, original);
                    cancellation.Cancel();
                }, updates, cancellation.Token));
                Assert(updates.Completed == 1, "Canceled candidate preparation leaked its provider deferral.");
                await AssertBytesAsync(file, replacement, "canceled candidate preparation");
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
            await AssertBytesAsync(file, original, "provider completion failure after commit");

            updates = new TestFileUpdates { FailCompletion = true };
            await ExpectAsync<InvalidDataException>(() => DocumentFileWriter.WriteAsync(file, async stream =>
            {
                await WriteAsync(stream, replacement);
                throw new InvalidDataException("Injected candidate validation failure.");
            }, updates));
            await AssertBytesAsync(file, original, "candidate validation failure");

            // CommitAsync already released exclusivity before the first timestamp read.
            // Equal-length bytes specifically require content verification.
            var foreign = (byte[])replacement.Clone();
            foreign[0] ^= 1;
            DocumentFileWriter.TestLocalCommit = savedFile => File.WriteAllBytes(savedFile.Path, foreign);
            try
            {
                var failure = await ExpectAsync<DocumentSaveException>(() => DocumentFileWriter.WriteAsync(file,
                    stream => WriteAsync(stream, replacement), new TestFileUpdates { FailDefer = true }));
                Assert(failure.Stage == DocumentSaveStage.Superseded, "A proven same-length replacement must fail the save.");
            }
            finally { DocumentFileWriter.TestLocalCommit = null; }
            await AssertBytesAsync(file, foreign, "no stale overwrite after the immediate post-commit race");

            // Metadata-only sync completion can establish a version when its bytes match exactly.
            updates = new TestFileUpdates { ProviderMutation = async savedFile =>
            {
                var time = await FileStorage.GetDateModifiedAsync(savedFile);
                File.SetLastWriteTimeUtc(savedFile.Path, DateTime.FromFileTimeUtc(time).AddMinutes(1));
            } };
            result = await DocumentFileWriter.WriteAsync(file, stream => WriteAsync(stream, replacement), updates);
            Assert(result.Version.ModifiedTime == await FileStorage.GetDateModifiedAsync(file), "Byte-identical provider updates permit the next in-place save.");

            // A nonparticipating writer can change the destination while the
            // provider awaits completion, even when the document lease is held.
            updates = new TestFileUpdates { ProviderMutation = async savedFile =>
            {
                await AssertBytesAsync(savedFile, replacement, "local commit before provider completion");
                var committedTime = await FileStorage.GetDateModifiedAsync(savedFile);
                await FileIO.WriteBytesAsync(savedFile, original);
                File.SetLastWriteTimeUtc(savedFile.Path, DateTime.FromFileTimeUtc(committedTime).AddMinutes(1));
            } };
            var superseded = await ExpectAsync<DocumentSaveException>(() => DocumentFileWriter.WriteAsync(file,
                stream => WriteAsync(stream, replacement), updates));
            Assert(superseded.Stage == DocumentSaveStage.Superseded && updates.Completed == 1,
                "Provider completion must be balanced before reporting a superseded save.");
            await AssertBytesAsync(file, original, "the next save preserves the intervening writer's bytes");

            updates = new TestFileUpdates
            {
                CompletionStatus = FileUpdateStatus.CompleteAndRenamed,
                ProviderMutation = async savedFile => await savedFile.RenameAsync("renamed-document", NameCollisionOption.FailIfExists)
            };
            result = await DocumentFileWriter.WriteAsync(file, stream => WriteAsync(stream, replacement), updates);
            Assert(result.File.Name == "renamed-document" && result.Version.IsVerified && result.UpdateStatus == FileUpdateStatus.CompleteAndRenamed &&
                result.ProviderError == null, "The save result lost the provider-renamed file identity.");
            await AssertBytesAsync(result.File, replacement, "provider rename");
            int readAttempts = 0, writes = 0;
            DocumentSaveTarget.TestVersionRead = (_, _) => ++readAttempts == 1 ? Task.FromException(SharingFailure()) : Task.CompletedTask;
            try
            {
                result = await DocumentFileWriter.WriteAsync(result.File, stream =>
                {
                    writes++;
                    return WriteAsync(stream, original);
                }, new TestFileUpdates(), expectedVersion: result.Version);
            }
            finally { DocumentSaveTarget.TestVersionRead = null; }
            Assert(writes == 1 && readAttempts == 2 && result.Version.IsVerified,
                "The next save on a renamed file should recover a transient observation failure without repeating its write.");

            var delayed = new TestFileUpdates { CompletionStatus = FileUpdateStatus.CompleteAndRenamed };
            result = await DocumentFileWriter.WriteAsync(file, stream => WriteAsync(stream, original), delayed);
            Assert(!result.Version.IsVerified && !(await result.VerifyAsync(default)).IsVerified,
                "A delayed provider rename must not verify the abandoned name, even when its bytes match.");
            await file.RenameAsync("delayed-provider-name");
            Assert((await result.VerifyAsync(default)).IsVerified && delayed.Completed == 1 && delayed.Deferred == 1,
                "The same commit can be verified read-only when the grant exposes its new location.");

            FileStream emptyFileBlocker = null;
            DocumentFileWriter.TestLocalCommit = savedFile => emptyFileBlocker = new FileStream(savedFile.Path, FileMode.Open, FileAccess.Write, FileShare.None);
            try
            {
                var empty = await DocumentFileWriter.WriteAsync(file, _ => Task.CompletedTask, new TestFileUpdates());
                Assert(!empty.Version.IsVerified,
                    "An empty commit must still pin the file before claiming it was verified.");
                emptyFileBlocker.Dispose();
                Assert((await empty.VerifyAsync(CancellationToken.None)).IsVerified, "An empty commit can be verified after the writer releases it.");
            }
            finally
            {
                DocumentFileWriter.TestLocalCommit = null;
                emptyFileBlocker?.Dispose();
            }

            updates = new TestFileUpdates { FailDefer = true };
            result = await DocumentFileWriter.WriteAsync(file, stream => WriteAsync(stream, original), updates);
            Assert(result.UpdateStatus == null && updates.Completed == 0,
                "An unavailable provider deferral prevented a local commit or acquired unowned provider state.");
            await AssertBytesAsync(file, original, "unavailable provider deferral");

            using (var cancellation = new CancellationTokenSource())
            {
                updates = new TestFileUpdates { AfterCommit = cancellation.Cancel };
                result = await DocumentFileWriter.WriteAsync(file, stream => WriteAsync(stream, replacement), updates, cancellation.Token);
                Assert(cancellation.IsCancellationRequested && result.UpdateStatus == FileUpdateStatus.Complete && result.Version.ModifiedTime == -1,
                    "Cancellation after local commit hid a successfully saved file.");
                await AssertBytesAsync(file, replacement, "cancellation after commit");
            }

            updates = new TestFileUpdates { CompletionStatus = FileUpdateStatus.UserInputNeeded };
            result = await DocumentFileWriter.WriteAsync(file, stream => WriteAsync(stream, original), updates);
            Assert(result.UpdateStatus == FileUpdateStatus.UserInputNeeded && result.ProviderError != null,
                "An incomplete provider update was silently treated as synchronized.");
            await AssertBytesAsync(file, original, "provider needs user input");

            using (var cancellation = new CancellationTokenSource())
            {
                cancellation.Cancel();
                updates = new TestFileUpdates();
                await ExpectAsync<OperationCanceledException>(() => DocumentFileWriter.WriteAsync(file,
                    stream => throw new Exception("A pre-canceled request wrote a candidate."), updates, cancellation.Token));
                Assert(updates.Deferred == 0 && updates.Completed == 0, "A pre-canceled request acquired provider state.");
                await AssertBytesAsync(file, original, "pre-canceled request");
            }

            log.AppendLine("PASS: document file transactions preserve committed bytes on retry/validation/cancel, balance provider ownership, and distinguish local save from pending synchronization.");
        }
        finally { await folder.DeleteAsync(StorageDeleteOption.PermanentDelete); }
    }

    private static async Task VerifyChunkedFileVersionsAsync(StorageFolder folder)
    {
        Assert(!new DocumentFileVersion(1, new string('!', 64)).IsVerified, "Invalid recovery fingerprints must remain unverified.");
        const int chunkSize = 256 * 1024;
        var file = await folder.CreateFileAsync("chunked", CreationCollisionOption.FailIfExists);
        var version = await DocumentSaveTarget.ReadVersionAsync(file);
        var random = new Random(42);
        foreach (var length in new[] { chunkSize, 2 * chunkSize + 17, 0 })
        {
            var bytes = new byte[length];
            random.NextBytes(bytes);
            var saved = await DocumentFileWriter.WriteAsync(file, stream => WriteAsync(stream, bytes),
                new TestFileUpdates(), expectedVersion: version);
            version = saved.Version;
            Assert(version.IsVerified && version.Sha256 == Convert.ToHexString(SHA256.HashData(bytes)),
                "Disk verification must hash the exact bytes through the final partial chunk and EOF.");
            if (length <= chunkSize) continue;

            // A provider may return a short read even when more of the file is available.
            var dispatcher = Windows.UI.Xaml.Window.Current.Dispatcher;
            Assert(dispatcher.HasThreadAccess, "The save verification fixture must start on the UI thread.");
            using var shortReads = new ShortReadStream(bytes);
            using var original = shortReads.AsInputStream();
            shortReads.AfterRead = () => Assert(!dispatcher.HasThreadAccess, "Synchronous file reads must not scan on the UI thread.");
            await DocumentSaveTarget.VerifyAsync(file, version, original, CancellationToken.None);
            shortReads.Position = 0;
            using var cancel = new CancellationTokenSource();
            shortReads.AfterRead = cancel.Cancel;
            await ExpectAsync<OperationCanceledException>(() => DocumentSaveTarget.VerifyAsync(file, version, original, cancel.Token));
            Assert(shortReads.Position == ShortReadStream.MaxRead, "Cancellation must stop a scan between reads.");

            bytes[^1] ^= 1;
            await FileIO.WriteBytesAsync(file, bytes);
            File.SetLastWriteTimeUtc(file.Path, DateTime.FromFileTimeUtc(version.ModifiedTime));
            var conflict = await ExpectAsync<DocumentSaveException>(() => DocumentFileWriter.WriteAsync(file,
                _ => throw new Exception("A tail-only conflict admitted the writer."), new TestFileUpdates(), expectedVersion: version));
            Assert(conflict.Stage == DocumentSaveStage.Conflict, "The last byte must be checked even when length and timestamp match.");
            version = await DocumentSaveTarget.ReadVersionAsync(file);
        }
    }

    private sealed partial class ShortReadStream(byte[] contents) : MemoryStream(contents, writable: false)
    {
        internal const int MaxRead = 8191;
        internal Action AfterRead;

        public override Task<int> ReadAsync(byte[] buffer, int offset, int count, CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            int read = Read(buffer, offset, Math.Min(MaxRead, count));
            AfterRead?.Invoke();
            return Task.FromResult(read);
        }

        public override ValueTask<int> ReadAsync(Memory<byte> buffer, CancellationToken cancellationToken = default)
        {
            int count = Read(buffer.Span[..Math.Min(MaxRead, buffer.Length)]);
            AfterRead?.Invoke();
            return ValueTask.FromResult(count);
        }
    }

    private static Task WriteAsync(Stream stream, byte[] bytes) => stream.WriteAsync(bytes, 0, bytes.Length);

    private static COMException SharingFailure() => new("Injected transient sharing failure.", unchecked((int)0x80070020));

    private static async Task AssertBytesAsync(StorageFile file, byte[] expected, string scenario)
    {
        using var stream = await file.OpenStreamForReadAsync();
        using var buffer = new MemoryStream();
        await stream.CopyToAsync(buffer);
        var actual = buffer.ToArray();
        if (actual.SequenceEqual(expected)) return;
        var offset = 0;
        while (offset < Math.Min(expected.Length, actual.Length) && expected[offset] == actual[offset]) offset++;
        var expectedByte = offset < expected.Length ? $"0x{expected[offset]:X2}" : "<end>";
        var actualByte = offset < actual.Length ? $"0x{actual[offset]:X2}" : "<end>";
        Assert(false, $"{scenario}: first difference at byte {offset}.\n" +
            $"  Expected {expected.Length} bytes, {expectedByte} at this offset.\n" +
            $"  Actual {actual.Length} bytes, {actualByte} at this offset.");
    }

    private static async Task<T> ExpectAsync<T>(Func<Task> operation) where T : Exception
    {
        try { await operation(); }
        catch (T error) { return error; }
        throw new Exception("Expected " + typeof(T).Name + " was not raised.");
    }

    private static void Assert(bool condition, string message)
    {
        if (!condition) throw new Exception(message);
    }

    internal sealed class TestFileUpdates : IDocumentFileUpdates
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
