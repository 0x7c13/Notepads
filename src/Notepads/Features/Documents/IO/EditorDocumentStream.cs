// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Infrastructure.Threading;
using WinUIEditor;

namespace Notepads.Features.Documents.IO;

/// <summary>
/// An asynchronous forward reader over a native document lease. WinUIEdit
/// owns the edit freeze and dispatches range reads to the editor apartment.
/// </summary>
internal sealed class EditorDocumentStream : Stream
{
    private const int BufferSize = 65536;
    private readonly EditorUtf8Reader _reader;
    private readonly bool _ownsReader;
    private readonly byte[] _buffer = new byte[BufferSize];
    private int _bufferOffset;
    private int _bufferLength;
    private long _readOffset;
    private long _position;
    private bool _disposed;

    public EditorDocumentStream(EditorUtf8Reader reader, bool ownsReader = true)
    {
        _reader = reader ?? throw new ArgumentNullException(nameof(reader));
        _ownsReader = ownsReader;
        Length = checked((long)reader.Length);
    }

    public override bool CanRead => !_disposed;
    public override bool CanSeek => false;
    public override bool CanWrite => false;
    public override long Length { get; }
    public override long Position
    {
        get => _position;
        set => throw new NotSupportedException();
    }

    public override async Task<int> ReadAsync(byte[] buffer, int offset, int count,
        CancellationToken cancellationToken)
    {
        if (buffer == null) throw new ArgumentNullException(nameof(buffer));
        if (offset < 0 || count < 0 || offset > buffer.Length - count)
            throw new ArgumentOutOfRangeException(nameof(offset));
        ThrowIfDisposed();
        cancellationToken.ThrowIfCancellationRequested();
        if (count == 0) return 0;

        if (_bufferOffset == _bufferLength)
        {
            if (_readOffset == Length) return 0;
            var bytes = await _reader.ReadAsync(_readOffset, BufferSize).AsCompletionTask(cancellationToken)
                .ConfigureAwait(false);
            ThrowIfDisposed();
            _bufferLength = checked((int)bytes.Length);
            if (_bufferLength == 0 || _bufferLength > Length - _readOffset)
                throw new InvalidDataException("The native document reader returned an invalid range.");
            bytes.CopyTo(0, _buffer, 0, _bufferLength);
            _bufferOffset = 0;
            _readOffset += _bufferLength;
        }

        var copied = Math.Min(count, _bufferLength - _bufferOffset);
        Array.Copy(_buffer, _bufferOffset, buffer, offset, copied);
        _bufferOffset += copied;
        _position += copied;
        return copied;
    }

    protected override void Dispose(bool disposing)
    {
        if (disposing && !_disposed)
        {
            _disposed = true;
            if (_ownsReader) _reader.Dispose();
        }
        base.Dispose(disposing);
    }

    private void ThrowIfDisposed()
    {
        if (_disposed) throw new ObjectDisposedException(nameof(EditorDocumentStream));
    }

    public override int Read(byte[] buffer, int offset, int count) => throw new NotSupportedException("Use asynchronous reads.");
    public override void Flush() { }
    public override long Seek(long offset, SeekOrigin origin) => throw new NotSupportedException();
    public override void SetLength(long value) => throw new NotSupportedException();
    public override void Write(byte[] buffer, int offset, int count) => throw new NotSupportedException();
}
