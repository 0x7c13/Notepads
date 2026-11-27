// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Microsoft.Win32.SafeHandles;
using Notepads.Features.Documents.IO;
using Notepads.Infrastructure.Storage;
using Windows.Storage;
using Windows.Storage.Provider;

namespace NotepadsEditorTests;

internal static partial class DocumentFileWriterTests
{
    private static async Task VerifyTargetCasingAsync(StorageFolder folder, StringBuilder log)
    {
        await VerifyCaseProofThreadAsync(folder, log);
        await VerifyRetryGrantSpellingAsync(folder, log);
        await VerifyFileOnlyGrantAsync(folder, log);
        var peer = await folder.CreateFileAsync("MiXeD.txt");
        var alias = await StorageFile.GetFileFromPathAsync(Path.Combine(folder.Path, "MIXED.TXT"));
        var before = await DocumentSaveTarget.ResolveAsync(peer, default);
        var version = await DocumentSaveTarget.ReadVersionAsync(peer);
        var saved = "saved from alias"u8.ToArray();
        using (await DocumentSaveTarget.AcquireAsync(before, default))
        {
            var result = await DocumentFileWriter.WriteAsync(alias, stream => WriteAsync(stream, saved),
                new TestFileUpdates(), expectedVersion: version);
            var after = await DocumentSaveTarget.ResolveAsync(peer, default);
            Assert(result.Version.IsVerified && await before.MatchesAsync(after) == true && await after.MatchesAsync(before) == true,
                "The two spellings of an ordinary entry must still match after the transaction.");
            await AssertBytesAsync(peer, saved, "case-variant save");
            using var cancel = new CancellationTokenSource();
            FileLockLease.TestLeaseWait = _ => cancel.Cancel();
            try
            {
                using var unexpected = await DocumentSaveTarget.AcquireAsync(after, cancel.Token);
                throw new InvalidOperationException("Changing a leaf's case bypassed the first save lease.");
            }
            catch (OperationCanceledException) when (cancel.IsCancellationRequested) { }
            finally { FileLockLease.TestLeaseWait = null; }
        }
        var stale = await ExpectAsync<DocumentSaveException>(() => DocumentFileWriter.WriteAsync(peer,
            _ => throw new Exception("The stale peer prepared a candidate."), new TestFileUpdates(), expectedVersion: version));
        Assert(stale.Stage == DocumentSaveStage.Conflict, "The second tab must detect the first tab's changed bytes.");
        var reopened = await StorageFile.GetFileFromPathAsync(Path.Combine(folder.Path, "MiXeD.txt"));
        var recovered = (await DocumentSaveTarget.ReadVersionAsync(reopened)) with { TargetPath = version.TargetPath };
        var next = "reopened"u8.ToArray();
        var replay = await DocumentFileWriter.WriteAsync(reopened, stream => WriteAsync(stream, next),
            new TestFileUpdates(), expectedVersion: recovered);
        Assert(replay.Version.IsVerified, "A matching persisted version must survive a change in leaf casing.");
        await AssertBytesAsync(reopened, next, "reopened case-variant save");
        log.AppendLine("PASS: case-variant saves verify exact bytes, retain one lease, reject stale edits and reuse persisted versions.");

        var sensitive = await folder.CreateFolderAsync("case-sensitive");
        using (var handle = OpenCaseTestDirectory(sensitive.Path, 0x100 /* FILE_WRITE_ATTRIBUTES */,
            FileShare.ReadWrite | FileShare.Delete, IntPtr.Zero, FileMode.Open, 0x02000000 /* BACKUP_SEMANTICS */, IntPtr.Zero))
        {
            uint enabled = 1;
            if (handle.IsInvalid || !SetCaseTestInformation(handle, 23 /* FileCaseSensitiveInfo */, in enabled, sizeof(uint)))
            {
                int error = Marshal.GetLastPInvokeError();
                if (error is not (1 or 5 or 50 or 87 or 1314))
                    throw new InvalidOperationException("Setting directory case sensitivity failed with Win32 error " + error);
                log.AppendLine("SKIP: this test process cannot enable directory case sensitivity (Win32 " + error + ").");
                return;
            }
        }
        var lowerPath = Path.Combine(sensitive.Path, "draft.txt");
        var upperPath = Path.Combine(sensitive.Path, "DRAFT.TXT");
        File.WriteAllBytes(lowerPath, saved);
        using (var upperStream = new FileStream(upperPath, FileMode.CreateNew, FileAccess.Write)) upperStream.Write(saved);
        var lower = await StorageFile.GetFileFromPathAsync(lowerPath);
        var upper = await StorageFile.GetFileFromPathAsync(upperPath);
        var lowerVersion = await DocumentSaveTarget.ReadVersionAsync(lower);
        Assert(!await DocumentSaveTarget.IsSameFileAsync(lower, upper, default),
            "Two distinct directory entries whose names differ only by case must not share an identity.");
        var wrong = await ExpectAsync<DocumentSaveException>(() => DocumentFileWriter.WriteAsync(upper,
            _ => throw new Exception("The other case-sensitive entry prepared a candidate."), new TestFileUpdates(), expectedVersion: lowerVersion));
        Assert(wrong.Stage == DocumentSaveStage.Conflict, "Equal bytes must not authorize a save to the other case-sensitive entry.");
        var own = await DocumentFileWriter.WriteAsync(lower, stream => WriteAsync(stream, next),
            new TestFileUpdates(), expectedVersion: lowerVersion);
        Assert(own.Version.IsVerified, "An exact-spelling save in a case-sensitive directory must verify.");
        await AssertBytesAsync(lower, next, "exact-spelling case-sensitive save");
        await AssertBytesAsync(upper, saved, "untouched case-sensitive sibling");
        log.AppendLine("PASS: equal-byte case-sensitive siblings remain distinct across version checks and saves.");
    }

    private static async Task VerifyCaseProofThreadAsync(StorageFolder folder, StringBuilder log)
    {
        var file = await folder.CreateFileAsync("Proof.txt");
        var version = await DocumentSaveTarget.ReadVersionAsync(file);
        var otherSpelling = Path.Combine(Path.GetDirectoryName(version.TargetPath), "PROOF.TXT");
        var dispatcher = Windows.UI.Xaml.Window.Current.Dispatcher;
        Assert(dispatcher.HasThreadAccess, "The case-variant save must start on the editor thread.");
        int entryChecks = 0, grantChecks = 0;
        DocumentSaveTarget.TestProvingCaseSpelling = grant =>
        {
            Assert(!dispatcher.HasThreadAccess, "Native case-equivalence queries must run off the editor thread.");
            if (grant) Interlocked.Increment(ref grantChecks);
            else Interlocked.Increment(ref entryChecks);
        };
        try
        {
            // A previous checkpoint and the provider's grant can independently change spelling.
            // Use a temporary name so that WinRT exposes the requested case on every run.
            var updates = new TestFileUpdates { ProviderMutation = async savedFile =>
            {
                await savedFile.RenameAsync("proof-parked.txt");
                await savedFile.RenameAsync("PROOF.TXT");
            } };
            var write = await DocumentFileWriter.WriteAsync(file, stream => WriteAsync(stream, "proof"u8.ToArray()),
                updates, expectedVersion: version with { TargetPath = otherSpelling });
            Assert(write.Version.IsVerified && entryChecks > 0 && grantChecks > 0,
                "The UI-initiated save must verify both the persisted entry spelling and the refreshed grant.");
        }
        finally { DocumentSaveTarget.TestProvingCaseSpelling = null; }

        var delayed = await DocumentFileWriter.WriteAsync(file, stream => WriteAsync(stream, "delayed"u8.ToArray()),
            new TestFileUpdates { CompletionStatus = FileUpdateStatus.CompleteAndRenamed });
        Assert(!delayed.Version.IsVerified, "A delayed provider rename must wait for the new spelling.");
        await file.RenameAsync("proof-parked.txt");
        await file.RenameAsync("pRoOf.TxT");
        Assert((await delayed.VerifyAsync(default)).IsVerified && delayed.File.Name == "pRoOf.TxT",
            "An explicitly reported case-only provider rename must follow the refreshed grant.");
        log.AppendLine("PASS: the editor thread delegates entry and grant case queries, and follows a delayed case-only provider rename.");
    }

    private static async Task VerifyRetryGrantSpellingAsync(StorageFolder folder, StringBuilder log)
    {
        var file = await folder.CreateFileAsync("Retry.txt");
        await FileIO.WriteBytesAsync(file, "before retry"u8.ToArray());
        var version = await DocumentSaveTarget.ReadVersionAsync(file);
        var target = await DocumentSaveTarget.ResolveAsync(file, default);
        var originalPath = file.Path;
        var metadataPath = @"\\?\" + originalPath;
        var saved = "committed after retry"u8.ToArray();
        var denied = 0;
        var candidates = 0;
        var updates = new TestFileUpdates { CompletionStatus = FileUpdateStatus.CompleteAndRenamed };
        DocumentSaveTarget.TestNamedMetadataUnavailable = path =>
        {
            if (!string.Equals(path, metadataPath, StringComparison.Ordinal) ||
                Interlocked.CompareExchange(ref denied, 1, 0) != 0) return false;
            // The first namespace open has no pins yet. Change the real grant's
            // case before the denied attempt reopens it; no normalization assumption.
            FileStorage.RenameAsync(file, "retry-parked.txt").GetAwaiter().GetResult();
            FileStorage.RenameAsync(file, "RETRY.TXT").GetAwaiter().GetResult();
            throw new UnauthorizedAccessException("Controlled pre-transaction access denial.");
        };
        try
        {
            var result = await DocumentFileWriter.WriteAsync(file, stream =>
            {
                candidates++;
                return WriteAsync(stream, saved);
            }, updates, expectedVersion: version, target: target);
            Assert(denied == 1 && candidates == 1 && updates.Deferred == 2 && updates.Completed == 2 &&
                !ReferenceEquals(result.File, file) && file.Path != originalPath &&
                result.File.Path == file.Path && result.File.Name == "RETRY.TXT",
                "Access denial must re-resolve the real renamed grant, balance both deferrals and write only once.");
            Assert(await DocumentSaveTarget.IsSameFileAsync(file, result.File, default),
                "The replacement retry grant must identify the same physical target.");
            Assert(!result.Version.IsVerified && !(await result.VerifyAsync(default)).IsVerified,
                "An access-denied retry must await the actual provider rename instead of confirming its earlier casing change.");
            await AssertBytesAsync(result.File, saved, "retry commit awaiting provider rename");
            await FileStorage.RenameAsync(result.File, "retry-provider-final.txt");
            var verified = await result.VerifyAsync(default);
            Assert(verified.IsVerified && result.File.Name == "retry-provider-final.txt" && candidates == 1 &&
                verified.TargetPath == (await DocumentSaveTarget.ResolveAsync(result.File, default)).FinalPath,
                "The pending retry receipt must follow and verify the actual provider rename without rewriting.");
            await AssertBytesAsync(result.File, saved, "verified retry provider destination");
        }
        finally { DocumentSaveTarget.TestNamedMetadataUnavailable = null; }
        log.AppendLine("PASS: access-denied grant replacement records its current spelling, waits for the actual provider rename, balances deferrals and verifies without a second write.");
    }

    private static async Task VerifyFileOnlyGrantAsync(StorageFolder folder, StringBuilder log)
    {
        var file = await folder.CreateFileAsync("Grant.txt");
        var alias = await StorageFile.GetFileFromPathAsync(Path.Combine(folder.Path, "GRANT.TXT"));
        var version = await DocumentSaveTarget.ReadVersionAsync(alias);
        var saved = "through grant"u8.ToArray();
        string during = null;
        DocumentSaveTarget.TestNamedMetadataUnavailable = _ => true;
        try
        {
            // FILE_READ_ATTRIBUTES is still allowed while a data reader is denied.
            using (var exclusive = new FileStream(file.Path, FileMode.Open, FileAccess.ReadWrite, FileShare.None))
            {
                var observed = await DocumentSaveTarget.ResolveAsync(alias, default);
                Assert(observed.FinalPath == version.TargetPath && observed.FinalPath != null,
                    "A file-only grant must observe the entry when a writer denies reading its bytes.");
            }
            DocumentFileWriter.TestTransactionLocation = path => during = path;
            var result = await DocumentFileWriter.WriteAsync(alias, stream => WriteAsync(stream, saved),
                new TestFileUpdates(), expectedVersion: version);
            Assert(during == version.TargetPath && result.Version.IsVerified,
                "The grant must re-observe its entry inside the transaction and confirm its committed leaf spelling.");
            await AssertBytesAsync(file, saved, "file-only grant");
            // A checkpoint alone contains no proof that an unrelated grant opened another spelling.
            var savedVersion = await DocumentSaveTarget.ReadVersionAsync(file);
            var unknown = savedVersion with { TargetPath = Path.Combine(Path.GetDirectoryName(savedVersion.TargetPath), "gRaNt.TxT") };
            var rejected = await ExpectAsync<DocumentSaveException>(() => DocumentFileWriter.WriteAsync(file,
                _ => throw new Exception("Unproven historical spelling reached the candidate."), new TestFileUpdates(), expectedVersion: unknown));
            Assert(rejected.Stage == DocumentSaveStage.Conflict, "A live receipt's casing authorization must not escape its commit.");

            var reopened = await StorageFile.GetFileFromPathAsync(Path.Combine(folder.Path, "Grant.txt"));
            var current = await DocumentSaveTarget.ReadVersionAsync(reopened);
            int writes = 0;
            DocumentSaveTarget.TestVersionRead = (_, _) => Task.FromException(SharingFailure());
            var pending = await DocumentFileWriter.WriteAsync(reopened, stream =>
            {
                writes++;
                return WriteAsync(stream, saved);
            }, new TestFileUpdates(), expectedVersion: current);
            DocumentSaveTarget.TestVersionRead = null;
            Assert(!pending.Version.IsVerified && (await pending.VerifyAsync(default)).IsVerified && writes == 1,
                "A retry must verify the file-only grant's committed spelling without writing a second time.");
        }
        finally
        {
            DocumentSaveTarget.TestVersionRead = null;
            DocumentFileWriter.TestTransactionLocation = null;
            DocumentSaveTarget.TestNamedMetadataUnavailable = null;
        }
        log.AppendLine("PASS: file-only grant metadata survives denied data reads and confirms its own case-only commit and read-only retry.");
    }

    [LibraryImport("api-ms-win-core-file-fromapp-l1-1-0.dll", EntryPoint = "CreateFileFromAppW",
        StringMarshalling = StringMarshalling.Utf16, SetLastError = true)]
    private static partial SafeFileHandle OpenCaseTestDirectory(string name, uint access, FileShare share,
        IntPtr security, FileMode disposition, uint flags, IntPtr template);

    [LibraryImport("api-ms-win-core-file-l1-2-0.dll", EntryPoint = "SetFileInformationByHandle", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool SetCaseTestInformation(SafeFileHandle handle, int infoClass, in uint flags, uint size);
}
