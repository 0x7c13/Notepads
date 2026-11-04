// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Threading;
using System.Threading.Tasks;

namespace Notepads.Features.Documents.Text;

// Transforms only ASCII indentation in a UTF-8 stream. Body bytes (including
// embedded NULs, Unicode and line endings) pass through without decoding.
public static class IndentationConverter
{
    public static async Task<bool> CopyAsync(Stream source, Stream destination, bool useTabs, int tabWidth,
        CancellationToken cancellation = default)
    {
        ArgumentNullException.ThrowIfNull(source);
        ArgumentNullException.ThrowIfNull(destination);
        ArgumentOutOfRangeException.ThrowIfNegativeOrZero(tabWidth);
        var input = new byte[65536];
        var output = new byte[65536];
        var used = 0;
        long columns = 0, spaces = 0;
        var leading = true;
        var tabAfterSpace = false;
        var hasTab = false;
        var changed = false;

        async ValueTask FlushAsync()
        {
            if (used == 0) return;
            await destination.WriteAsync(output.AsMemory(0, used), cancellation).ConfigureAwait(false);
            used = 0;
        }
        async ValueTask FillAsync(byte value, long count)
        {
            while (count > 0)
            {
                var take = (int)Math.Min(count, output.Length - used);
                output.AsSpan(used, take).Fill(value);
                used += take;
                count -= take;
                if (used == output.Length) await FlushAsync().ConfigureAwait(false);
            }
        }
        async ValueTask PrefixAsync()
        {
            changed |= useTabs ? tabAfterSpace || spaces >= tabWidth : hasTab;
            if (useTabs) await FillAsync((byte)'\t', columns / tabWidth).ConfigureAwait(false);
            await FillAsync((byte)' ', useTabs ? columns % tabWidth : columns).ConfigureAwait(false);
            columns = spaces = 0;
            hasTab = tabAfterSpace = false;
        }
        int count;
        while ((count = await source.ReadAsync(input, cancellation).ConfigureAwait(false)) != 0)
        {
            cancellation.ThrowIfCancellationRequested();
            var position = 0;
            while (position < count)
            {
                if (leading)
                {
                    var ch = input[position];
                    if (ch is (byte)' ' or (byte)'\t')
                    {
                        if (ch == '\t')
                        {
                            hasTab = true;
                            tabAfterSpace |= spaces > 0;
                            columns = checked(columns + tabWidth - columns % tabWidth);
                        }
                        else { columns = checked(columns + 1); spaces++; }
                        position++;
                        continue;
                    }
                    await PrefixAsync().ConfigureAwait(false);
                    leading = false;
                }
                var end = input.AsSpan(position, count - position).IndexOfAny((byte)'\r', (byte)'\n');
                var length = end < 0 ? count - position : end + 1;
                leading = end >= 0;
                while (length > 0)
                {
                    var take = Math.Min(length, output.Length - used);
                    input.AsSpan(position, take).CopyTo(output.AsSpan(used));
                    used += take;
                    position += take;
                    length -= take;
                    if (used == output.Length) await FlushAsync().ConfigureAwait(false);
                }
            }
        }
        if (leading) await PrefixAsync().ConfigureAwait(false);
        await FlushAsync().ConfigureAwait(false);
        return changed;
    }
}
