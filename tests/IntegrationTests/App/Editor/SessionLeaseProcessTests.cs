// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents.IO;
using Notepads.Features.Sessions.Storage;
using Windows.Storage;

namespace NotepadsEditorTests;

/// <summary>Two packaged processes verify admission, shared reader pins and abrupt-exit release.</summary>
internal static class SessionLeaseProcessTests
{
    public static async Task RunAsync(string arguments)
    {
        var parts = arguments.Split('=');
        if (parts.Length != 2 || !Guid.TryParseExact(parts[1], "N", out var owner))
            throw new ArgumentException("A lease probe requires an isolated scope ID.");
        var directory = Path.Combine(ApplicationData.Current.LocalFolder.Path, "LeaseTests", owner.ToString("N"));
        Directory.CreateDirectory(directory);
        // Exceed both MAX_PATH and the first final-name query buffer.
        var longSubpath = string.Join(Path.DirectorySeparatorChar,
            System.Linq.Enumerable.Repeat("long-document-directory-for-alias-exclusion-tests", 11));
        try
        {
            if (parts[0] == "--lease-holder")
            {
                IDisposable writer;
                IDisposable reader;
                using (await SessionRecoveryTransaction.EnterAsync())
                {
                    writer = await SessionScopeLease.AcquireWriterAsync(owner);
                    reader = SessionScopeLease.AcquireReader(owner);
                }
                var state = await StorageFolder.GetFolderFromPathAsync(directory);
                var documents = await state.CreateFolderAsync("documents", CreationCollisionOption.FailIfExists);
                var document = await documents.CreateFileAsync("shared.txt");
                var longDirectory = Directory.CreateDirectory(Path.Combine(documents.Path, longSubpath));
                var longPath = Path.Combine(longDirectory.FullName, "MixedCase.txt");
                File.WriteAllText(longPath, "long path");
                var longDocument = await StorageFile.GetFileFromPathAsync(longPath);
                using (writer)
                using (reader)
                using (await DocumentSaveTarget.AcquireAsync(document, CancellationToken.None))
                using (await DocumentSaveTarget.AcquireAsync(longDocument, CancellationToken.None))
                {
                    File.WriteAllText(Path.Combine(directory, "holder-ready"), Environment.ProcessId.ToString());
                    await WaitForAsync(Path.Combine(directory, "request-gate"));
                    using (await SessionRecoveryTransaction.EnterAsync())
                    {
                        File.WriteAllText(Path.Combine(directory, "gate-held"), "ready");
                        // The harness terminates this holder to verify OS handle release.
                        await Task.Delay(Timeout.Infinite);
                    }
                }
            }
            else if (parts[0] == "--lease-probe")
            {
                await WaitForAsync(Path.Combine(directory, "holder-ready"));
                using (await SessionRecoveryTransaction.EnterAsync())
                {
                    using var writer = SessionScopeLease.TryAcquireInactiveWriter(owner);
                    Check(writer == null, "another packaged process owns the scope writer");
                    using var exclusive = SessionScopeLease.TryAcquireExclusiveReaders(owner);
                    Check(exclusive == null, "reader pin prevents foreign scope retirement");
                    using var shared = SessionScopeLease.AcquireReader(owner);
                    Check(shared != null, "reader pins are shared across packaged processes");
                }
                var target = await StorageFile.GetFileFromPathAsync(Path.Combine(directory, "documents", "shared.txt"));
                var alias = await StorageFile.GetFileFromPathAsync(Path.Combine(directory, "alias", "shared.txt"));
                Check(!string.Equals(target.Path, alias.Path, StringComparison.OrdinalIgnoreCase), "the junction must exercise a distinct spelling");
                Check(await DocumentSaveTarget.IsSameFileAsync(target, alias, CancellationToken.None),
                    "the directory-entry comparison must agree with the cross-process lease");
                var longAlias = await StorageFile.GetFileFromPathAsync(Path.Combine(directory, "alias", longSubpath, "MIXEDCASE.TXT"));
                foreach (var spelling in new[] { target, alias, longAlias })
                {
                    using var canceled = new CancellationTokenSource();
                    Notepads.Infrastructure.Storage.FileLockLease.TestLeaseWait = _ => canceled.Cancel();
                    try
                    {
                        using var lease = await DocumentSaveTarget.AcquireAsync(spelling, canceled.Token);
                        throw new InvalidOperationException("A second packaged process acquired the same document through " + spelling.Path);
                    }
                    catch (OperationCanceledException) when (canceled.IsCancellationRequested) { }
                    finally { Notepads.Infrastructure.Storage.FileLockLease.TestLeaseWait = null; }
                }
                File.WriteAllText(Path.Combine(directory, "request-gate"), "ready");
                await WaitForAsync(Path.Combine(directory, "gate-held"));
                using (var canceled = new CancellationTokenSource(TimeSpan.FromMilliseconds(300)))
                {
                    var rejected = false;
                    try { using var admission = await SessionRecoveryTransaction.EnterAsync(canceled.Token); }
                    catch (OperationCanceledException) { rejected = true; }
                    Check(rejected, "global publication gate serializes separate packaged processes");
                }
                File.WriteAllText(Path.Combine(directory, "terminate-holder"), "ready");
                using (var deadline = new CancellationTokenSource(TimeSpan.FromSeconds(30)))
                using (await SessionRecoveryTransaction.EnterAsync(deadline.Token))
                {
                    using var writer = SessionScopeLease.TryAcquireInactiveWriter(owner);
                    using var readers = SessionScopeLease.TryAcquireExclusiveReaders(owner);
                    Check(writer != null && readers != null, "abrupt process exit releases writer, reader and publication leases");
                }
                using (var deadline = new CancellationTokenSource(TimeSpan.FromSeconds(5)))
                using (await DocumentSaveTarget.AcquireAsync(target, deadline.Token))
                    Check(true, "abrupt process exit releases the document save lease");
                File.WriteAllText(Path.Combine(directory, "result.txt"),
                    "PASS: two packaged processes verified exclusive writers, shared readers, junction-alias exclusion, cancellation, retirement fencing and abrupt-exit release.");
            }
            else
            {
                throw new ArgumentException("Unknown lease probe mode.");
            }
        }
        catch (Exception ex)
        {
            File.WriteAllText(Path.Combine(directory, "result.txt"), "FAIL: " + ex);
            throw;
        }
    }

    private static void Check(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }

    private static async Task WaitForAsync(string path)
    {
        var deadline = DateTime.UtcNow.AddSeconds(30);
        while (!File.Exists(path))
        {
            if (DateTime.UtcNow >= deadline) throw new TimeoutException("Lease probe handshake timed out.");
            await Task.Delay(20);
        }
    }
}
