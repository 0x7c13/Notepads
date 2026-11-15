// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Linq;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Text;
using System.Threading.Tasks;
using Notepads.Markdown;
using Windows.Foundation;
using Windows.Storage.Streams;

namespace NotepadsIntegrationTests;

internal static partial class MarkdownFeatureTests
{
    internal static async Task RunCoreInteropAsync(StringBuilder log)
    {
        await VerifyCoreProbeAsync(CoreValidationProbe.RunAsync(), 28, "Core controller", log);
        await VerifyCoreProbeAsync(CoreValidationProbe.RunPreviewAsync(), 7, "Preview tickets", log);
        var provider = new CoreProbeImageProvider();
        await VerifyCoreProbeAsync(CoreValidationProbe.RunImageProviderAsync(provider), 2, "Managed image provider", log);
        if (provider.Calls != 1) throw new InvalidOperationException("Expected exactly one managed image provider request.");
    }

    private static async Task VerifyCoreProbeAsync(IAsyncOperation<CoreValidationReport> operation, int expected,
        string name, StringBuilder log)
    {
        Task<CoreValidationReport> pending = operation.AsTask();
        CoreValidationReport result;
        try { result = await pending.WaitAsync(TimeSpan.FromSeconds(120)); }
        catch (TimeoutException)
        {
            operation.Cancel();
            _ = ObserveCoreProbeFailureAsync(pending);
            throw;
        }

        var checks = result.Checks;
        foreach (var check in checks)
            log.AppendLine($"Native {name} {check.Name}: passed={check.Passed}; status={check.Status}; {check.Detail}");
        if (!result.Passed || checks.Length != expected || checks.Any(check => !check.Passed) ||
            checks.Select(check => check.Name).Distinct(StringComparer.Ordinal).Count() != expected)
            throw new InvalidOperationException($"All {expected} {name} diagnostics must pass exactly once.");
    }

    private static async Task ObserveCoreProbeFailureAsync(Task pending)
    {
        try { await pending; }
        catch { }
    }

    [WinRT.GeneratedWinRTExposedType]
    private sealed partial class CoreProbeImageProvider : IMarkdownImageProvider
    {
        internal int Calls { get; private set; }

        public IAsyncOperation<IRandomAccessStream> OpenAsync(MarkdownImageRequest request)
        {
            if (request.Destination != "notepads-markdown-test:provider-seam" || request.ImageId != 7)
                throw new InvalidOperationException("The image provider probe did not preserve the request identity.");
            ++Calls;
            return AsyncInfo.Run<IRandomAccessStream>(cancellation =>
                CreateInMemoryTestStreamAsync(Convert.FromBase64String(PngBase64), cancellation));
        }
    }
}
