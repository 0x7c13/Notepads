// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Notepads.Markdown;

namespace NotepadsIntegrationTests;

internal static class MarkdownCoreHighlightTests
{
    internal static async Task RunAsync(StringBuilder log)
    {
        string[] expected =
        [
            "highlight_snapshot_json_rust_unicode", "highlight_snapshot_explicit_skips",
            "highlight_snapshot_retired_source_owners"
        ];
        var operation = CoreValidationProbe.RunHighlightAsync();
        Task<CoreValidationReport> pending = operation.AsTask();
        CoreValidationReport result;
        try { result = await pending.WaitAsync(TimeSpan.FromSeconds(45)); }
        catch (TimeoutException)
        {
            operation.Cancel();
            _ = ObserveLateFailureAsync(pending);
            throw;
        }
        CoreValidationCheck[] checks = result.Checks;
        foreach (var check in checks)
            log.AppendLine($"Native highlight {check.Name}: passed={check.Passed}; status={check.Status}; {check.Detail}");
        if (checks.Length != expected.Length || !result.Passed ||
            expected.Any(name => checks.Count(check => check.Name == name) != 1) ||
            checks.Any(check => !check.Passed))
            throw new InvalidOperationException("All original native highlight diagnostics, including C++ aliases, must pass once.");
        log.AppendLine($"PASS: exact component highlight diagnostic set ({checks.Length} cases, Core ABI {result.CoreAbiVersion}).");
    }

    private static async Task ObserveLateFailureAsync(Task pending)
    {
        try { await pending; }
        catch { }
    }
}
