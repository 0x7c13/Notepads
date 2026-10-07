// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Documents.Storage;

namespace Notepads.Features.Documents;

/// <summary>Saved file metadata and an independently retained immutable document generation.</summary>
public sealed class DocumentSnapshot : IDisposable
{
    private readonly DocumentBaseline _baseline;
    private readonly Encoding _encoding;

    public DocumentSnapshot(DocumentBaseline baseline, Encoding encoding, LineEnding lineEnding,
        long dateModifiedFileTime = -1)
    {
        if (baseline == null) throw new ArgumentNullException(nameof(baseline));
        if (encoding == null) throw new ArgumentNullException(nameof(encoding));
        if (!Enum.IsDefined(typeof(LineEnding), lineEnding)) throw new ArgumentOutOfRangeException(nameof(lineEnding));
        _encoding = (Encoding)encoding.Clone();
        _baseline = baseline.Retain();
        LineEnding = lineEnding;
        DateModifiedFileTime = dateModifiedFileTime;
    }

    /// <summary>Borrowed lease; retain it before starting an independently owned operation.</summary>
    public DocumentBaseline Baseline => _baseline;

    // Encoding exposes mutable fallback properties, so return a detached copy.
    public Encoding Encoding => (Encoding)_encoding.Clone();

    public LineEnding LineEnding { get; }

    /// <summary>The saved file's modification time; -1 means it could not be queried.</summary>
    public long DateModifiedFileTime { get; }

    public DocumentSnapshot Retain()
    {
        return new DocumentSnapshot(_baseline, _encoding, LineEnding, DateModifiedFileTime);
    }

    /// <summary>
    /// Explicit full-text materialization for preview, diff, and legacy callers.
    /// Persistent document I/O should use the retained baseline stream instead.
    /// </summary>
    public async Task<string> ReadTextAsync(CancellationToken cancellationToken = default)
    {
        using (var stream = await _baseline.OpenReadStreamAsync(cancellationToken).ConfigureAwait(false))
        using (var reader = new StreamReader(stream, new UTF8Encoding(false, true),
            detectEncodingFromByteOrderMarks: false, bufferSize: 16384))
        {
            var characters = new char[16384];
            var text = new StringBuilder();
            while (true)
            {
                cancellationToken.ThrowIfCancellationRequested();
                var count = await reader.ReadAsync(characters, 0, characters.Length).ConfigureAwait(false);
                if (count == 0) break;
                text.Append(characters, 0, count);
            }
            cancellationToken.ThrowIfCancellationRequested();
            return text.ToString();
        }
    }

    public void Dispose() => _baseline.Dispose();

    public Task DisposeAsync() => _baseline.DisposeAsync();
}
