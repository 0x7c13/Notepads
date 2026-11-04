// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Diagnostics;
using System.IO;
using System.Threading.Tasks;

namespace NotepadsTestIsolation;

// Short phases must not exit before the runner owns the exact process handle.
// A PID alone is insufficient: the acknowledgement also names its creation time.
internal static class TestProcessLifetime
{
    internal static async Task WaitForRetentionAsync(string runRoot)
    {
        using var process = Process.GetCurrentProcess();
        var marker = Path.Combine(runRoot, $"retained-process-{process.Id}.txt");
        var expected = process.StartTime.ToUniversalTime().Ticks.ToString(System.Globalization.CultureInfo.InvariantCulture);
        var watch = Stopwatch.StartNew();
        while (true)
        {
            try { if (File.Exists(marker) && File.ReadAllText(marker) == expected) return; }
            catch (IOException) { }
            if (watch.Elapsed > TimeSpan.FromSeconds(15)) throw new TimeoutException("The runner never retained this process.");
            await Task.Delay(10).ConfigureAwait(false);
        }
    }
}
