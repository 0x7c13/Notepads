// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Security.Cryptography;
using System.Threading;
using System.Threading.Tasks;

namespace Notepads.Features.Documents.IO;

/// <summary>Fingerprint the exact sequential output, tagging failures on the user-file side of the codec.</summary>
internal sealed partial class DocumentSaveOutputStream(Stream destination) : Stream
{
    private readonly IncrementalHash _hash = IncrementalHash.CreateHash(HashAlgorithmName.SHA256);
    private long _length;
    internal byte[] FinishHash() => _hash.GetHashAndReset();

    public override bool CanRead => false;
    public override bool CanSeek => false;
    public override bool CanWrite => true;
    public override long Length => _length;
    public override long Position { get => _length; set => throw new NotSupportedException(); }

    public override async ValueTask WriteAsync(ReadOnlyMemory<byte> buffer, CancellationToken cancellationToken = default)
    {
        try { await destination.WriteAsync(buffer, cancellationToken).ConfigureAwait(false); }
        catch (Exception ex) when (DocumentSaveException.IsStorageError(ex))
        {
            throw new DocumentSaveException(DocumentSaveStage.WriteTarget, ex);
        }
        _hash.AppendData(buffer.Span);
        _length += buffer.Length;
    }

    public override Task WriteAsync(byte[] buffer, int offset, int count, CancellationToken cancellationToken) =>
        WriteAsync(buffer.AsMemory(offset, count), cancellationToken).AsTask();

    public override void Write(byte[] buffer, int offset, int count)
    {
        try { destination.Write(buffer, offset, count); }
        catch (Exception ex) when (DocumentSaveException.IsStorageError(ex))
        {
            throw new DocumentSaveException(DocumentSaveStage.WriteTarget, ex);
        }
        _hash.AppendData(buffer, offset, count);
        _length += count;
    }

    public override void Flush() => destination.Flush();
    public override Task FlushAsync(CancellationToken cancellationToken) => destination.FlushAsync(cancellationToken);
    public override int Read(byte[] buffer, int offset, int count) => throw new NotSupportedException();
    public override long Seek(long offset, SeekOrigin origin) => throw new NotSupportedException();
    public override void SetLength(long value) => throw new NotSupportedException();

    protected override void Dispose(bool disposing)
    {
        if (disposing) _hash.Dispose();
        base.Dispose(disposing);
    }
}
