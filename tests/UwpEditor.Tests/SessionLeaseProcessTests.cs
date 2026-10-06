// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Threading;
using System.Threading.Tasks;
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
                using (writer)
                using (reader)
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
                File.WriteAllText(Path.Combine(directory, "result.txt"),
                    "PASS: two packaged processes verified exclusive writers, shared readers, retirement fencing, cancellation and abrupt-exit release.");
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
