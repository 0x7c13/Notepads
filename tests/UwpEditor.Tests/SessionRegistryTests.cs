// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Composition;
using Notepads.Features.Sessions.Contracts;

namespace NotepadsEditorTests;

internal static class SessionRegistryTests
{
    public static async Task RunAsync(StringBuilder log)
    {
        var registry = new SessionRegistry();
        var healthy = new Participant();
        var failed = new Participant { Fail = true };
        var disabled = new Participant { IsBackupEnabled = false };
        var original = registry.Register(healthy);
        using (registry.Register(failed))
        using (registry.Register(disabled))
        {
            await registry.SaveBeforeSuspensionAsync(CancellationToken.None);
            Check(healthy.SaveCount == 1 && failed.SaveCount == 1 && disabled.SaveCount == 0,
                "One failed participant prevented an independent window's suspension save.");
        }
        original.Dispose();
        using (registry.Register(healthy))
        {
            original.Dispose();
            using (var deadline = new CancellationTokenSource())
            {
                deadline.Cancel();
                await registry.SaveBeforeSuspensionAsync(deadline.Token);
                Check(healthy.LastCancellation == deadline.Token && healthy.SaveCount == 2,
                    "An old token removed a newer registration or the suspension deadline was lost.");
            }
        }
        await registry.SaveBeforeSuspensionAsync(CancellationToken.None);
        Check(healthy.SaveCount == 2, "A logically closed window remained in the registry.");
        log.AppendLine("PASS: suspension saves isolate window failures, propagate deadlines and remove only the exact registration.");
    }

    private static void Check(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }

    private sealed class Participant : ISessionPersistenceParticipant
    {
        public bool IsBackupEnabled { get; set; } = true;
        public bool Fail { get; set; }
        public int SaveCount { get; private set; }
        public CancellationToken LastCancellation { get; private set; }

        public Task<bool> SaveForSuspensionAsync(CancellationToken cancellation)
        {
            SaveCount++;
            LastCancellation = cancellation;
            if (Fail) throw new InvalidOperationException("A single window could not save.");
            cancellation.ThrowIfCancellationRequested();
            return Task.FromResult(true);
        }

        public Task DrainAsync() => Task.CompletedTask;
    }
}
