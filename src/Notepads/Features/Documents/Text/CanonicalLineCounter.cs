// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System.IO;
using System.Threading;
using System.Threading.Tasks;

namespace Notepads.Features.Documents.Text;

internal static class CanonicalLineCounter
{
    // Mirrors the native MaximumDiffLines (NativeDiff.h); diff admission rejects any larger count.
    private const long MaximumDiffLines = 1_000_000;

    // Counted streaming admission; no strings or per-line allocations.
    internal static async Task<long> CountAsync(Stream stream, CancellationToken cancellationToken)
    {
        var buffer = new byte[65536];
        long lines = 1;
        while (true)
        {
            var count = await stream.ReadAsync(buffer, cancellationToken).ConfigureAwait(false);
            if (count == 0) return lines;
            for (var i = 0; i < count; i++)
                if (buffer[i] == '\r') lines++;
            if (lines > MaximumDiffLines) return lines;
        }
    }
}
