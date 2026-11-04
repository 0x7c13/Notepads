// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System.Diagnostics;
using NotepadsArchitectureTests;
using NotepadsSessionIndexTests;

namespace NotepadsIntegrationTests;

internal static class Program
{
    private static async Task<int> Main(string[] args)
    {
        var watch = Stopwatch.StartNew();
        try
        {
            if (args is ["--list"])
            {
                Console.WriteLine("storage\narchitecture-self-test\narchitecture <Notepads.dll>\nmetrics-initialization-failure\nmetrics-counter-failure\nmetrics-duration-failure");
                return 0;
            }
            if (args is [] or ["--storage"])
                await RecoveryStorageSuite.RunAsync([]);
            else if (args is ["--architecture-self-test"])
                return ArchitectureSuite.Run(["--self-test"]);
            else if (args is ["--architecture", var assembly])
                return ArchitectureSuite.Run([assembly]);
            else if (args is [var mode] && mode.StartsWith("--metrics-", StringComparison.Ordinal))
                await RecoveryStorageSuite.RunAsync(args);
            else throw new ArgumentException("Unknown or incomplete integration selection. Use --list.");
            Console.WriteLine($"PASS: Headless integration ({watch.ElapsedMilliseconds} ms).");
            return 0;
        }
        catch (Exception error)
        {
            Console.Error.WriteLine($"FAIL: Headless integration ({watch.ElapsedMilliseconds} ms): {error}");
            return 1;
        }
    }
}
