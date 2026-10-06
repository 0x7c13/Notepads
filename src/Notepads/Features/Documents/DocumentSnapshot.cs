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
    private long _dateModifiedFileTime;
    private int _fileWriteState;
    private int _disposed;

    public DocumentSnapshot(DocumentBaseline baseline, Encoding encoding, LineEnding lineEnding,
        long dateModifiedFileTime = -1)
        : this(baseline, encoding, lineEnding, dateModifiedFileTime, preparedFileWrite: false)
    {
    }

    private DocumentSnapshot(DocumentBaseline baseline, Encoding encoding, LineEnding lineEnding,
        long dateModifiedFileTime, bool preparedFileWrite)
    {
        if (baseline == null) throw new ArgumentNullException(nameof(baseline));
        if (encoding == null) throw new ArgumentNullException(nameof(encoding));
        if (!Enum.IsDefined(typeof(LineEnding), lineEnding)) throw new ArgumentOutOfRangeException(nameof(lineEnding));
        _encoding = (Encoding)encoding.Clone();
        _baseline = baseline.Retain();
        LineEnding = lineEnding;
        _dateModifiedFileTime = dateModifiedFileTime;
        _fileWriteState = preparedFileWrite ? 0 : 1;
    }

    /// <summary>Borrowed lease; retain it before starting an independently owned operation.</summary>
    public DocumentBaseline Baseline => _baseline;

    // Encoding exposes mutable fallback properties, so return a detached copy.
    public Encoding Encoding => (Encoding)_encoding.Clone();

    public LineEnding LineEnding { get; }

    public long DateModifiedFileTime
    {
        get
        {
            EnsurePublished();
            return _dateModifiedFileTime;
        }
    }

    /// <summary>
    /// Allocate the complete saved state before committing the user file.
    /// This private candidate cannot be retained or published before completion.
    /// </summary>
    internal static DocumentSnapshot PrepareFileWrite(DocumentBaseline baseline, Encoding encoding, LineEnding lineEnding)
    {
        return new DocumentSnapshot(baseline, encoding, lineEnding, -1, preparedFileWrite: true);
    }

    /// <summary>One-time metadata stamp after file commit; -1 means its time could not be queried.</summary>
    internal void CompleteFileWrite(long dateModifiedFileTime)
    {
        if (Volatile.Read(ref _disposed) != 0) throw new ObjectDisposedException(nameof(DocumentSnapshot));
        if (Interlocked.CompareExchange(ref _fileWriteState, -1, 0) != 0)
            throw new InvalidOperationException("Only an unpublished prepared save can be completed once.");
        _dateModifiedFileTime = dateModifiedFileTime;
        Volatile.Write(ref _fileWriteState, 1);
    }

    public DocumentSnapshot Retain()
    {
        EnsurePublished();
        return new DocumentSnapshot(_baseline, _encoding, LineEnding, DateModifiedFileTime);
    }

    private void EnsurePublished()
    {
        if (Volatile.Read(ref _fileWriteState) != 1)
            throw new InvalidOperationException("The prepared save has not committed its file.");
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

    public void Dispose()
    {
        Interlocked.Exchange(ref _disposed, 1);
        _baseline.Dispose();
    }

    public Task DisposeAsync()
    {
        Interlocked.Exchange(ref _disposed, 1);
        return _baseline.DisposeAsync();
    }
}
