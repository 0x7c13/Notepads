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
            if (lines > 1_000_000) return lines;
        }
    }
}
