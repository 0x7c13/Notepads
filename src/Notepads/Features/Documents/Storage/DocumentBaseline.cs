// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Diagnostics.CodeAnalysis;
using System.IO;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Documents.Text;
using Windows.Storage;

namespace Notepads.Features.Documents.Storage;

/// <summary>
/// A lease on an immutable canonical UTF-8 document generation. Temporary
/// generations are deleted only after their final handle and reader close.
/// </summary>
public sealed class DocumentBaseline : IDisposable
{
    public const long MaximumByteLength = DocumentLimits.MaximumCanonicalByteLength;

    private const string EmptySha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    private readonly DocumentAssetLease _asset;
    private readonly Guid _ownerId;
    private readonly Guid _generationId;
    private readonly long _byteLength;
    private readonly string _sha256;

    private DocumentBaseline(DocumentAssetLease asset, Guid ownerId, Guid generationId, long byteLength, string sha256)
    {
        _asset = asset;
        _ownerId = ownerId;
        _generationId = generationId;
        _byteLength = byteLength;
        _sha256 = sha256;
    }

    public Guid OwnerId => _ownerId;

    public Guid GenerationId => _generationId;

    public long ByteLength => _byteLength;

    public string Sha256 => _sha256;

    /// <summary>Borrowed file identity; null for an empty generation created without disk I/O.</summary>
    public StorageFile File => _asset.File;

    public static DocumentBaseline CreateEmpty(Guid? ownerId = null)
    {
        return new DocumentBaseline(DocumentAssetLease.Acquire(null), ownerId ?? DocumentAssetLease.DefaultOwnerId,
            Guid.NewGuid(), 0, EmptySha256);
    }

    /// <summary>
    /// The writer must emit UTF-8 document bytes without an added BOM, using
    /// CR line endings. Writes are sequential and the callback must await them.
    /// A generation is published only after validation and flushing succeed.
    /// </summary>
    public static async Task<DocumentBaseline> CreateAsync(Func<Stream, Task> writeCanonicalUtf8Async,
        CancellationToken cancellationToken = default, Guid? ownerId = null)
    {
        if (writeCanonicalUtf8Async == null) throw new ArgumentNullException(nameof(writeCanonicalUtf8Async));
        cancellationToken.ThrowIfCancellationRequested();
        var owner = ownerId ?? DocumentAssetLease.DefaultOwnerId;
        var generation = Guid.NewGuid();
        var folder = await ApplicationData.Current.LocalFolder.CreateFolderAsync(
            "DocumentBaselines", CreationCollisionOption.OpenIfExists);
        cancellationToken.ThrowIfCancellationRequested();
        var asset = await DocumentAssetLease.CreateNewAsync(folder,
            owner.ToString("N") + "-" + generation.ToString("N") + ".utf8", cancellationToken);
        var file = asset.File;
        try
        {
            long byteLength;
            string hash;
            using (var stream = await file.OpenStreamForWriteAsync().ConfigureAwait(false))
            using (var writer = new CanonicalWriteStream(stream, cancellationToken))
            {
                cancellationToken.ThrowIfCancellationRequested();
                await writeCanonicalUtf8Async(writer).ConfigureAwait(false);
                hash = writer.Complete();
                await stream.FlushAsync(cancellationToken).ConfigureAwait(false);
                byteLength = writer.Length;
            }
            cancellationToken.ThrowIfCancellationRequested();
            return new DocumentBaseline(asset, owner, generation, byteLength, hash);
        }
        catch
        {
            await asset.DisposeAsync().ConfigureAwait(false);
            throw;
        }
    }

    /// <summary>Explicit adapter for callers that already materialize a managed snapshot.</summary>
    public static Task<DocumentBaseline> FromTextAsync(string text,
        CancellationToken cancellationToken = default, Guid? ownerId = null)
    {
        if (text == null) throw new ArgumentNullException(nameof(text));
        return CreateAsync(async stream =>
        {
            using (var writer = new StreamWriter(stream, new UTF8Encoding(false, true), 16384, leaveOpen: true))
            {
                await LineEndingUtility.WriteAsync(writer, text, LineEnding.Cr).ConfigureAwait(false);
                await writer.FlushAsync().ConfigureAwait(false);
            }
        }, cancellationToken, ownerId);
    }

    /// <summary>Validate and retain an existing manifest-owned generation without copying its text.</summary>
    public static async Task<DocumentBaseline> OpenExistingAsync(StorageFile file, Guid ownerId, Guid generationId,
        long byteLength, string sha256, CancellationToken cancellationToken = default)
    {
        if (generationId == Guid.Empty || byteLength < 0 || byteLength > MaximumByteLength || string.IsNullOrEmpty(sha256))
            throw new InvalidDataException("Invalid existing document baseline identity.");
        cancellationToken.ThrowIfCancellationRequested();
        if (file == null)
        {
            if (byteLength != 0 || !string.Equals(sha256, EmptySha256, StringComparison.OrdinalIgnoreCase))
                throw new InvalidDataException("Only an empty baseline can omit its backing file.");
            return new DocumentBaseline(DocumentAssetLease.Acquire(null, persistent: true), ownerId, generationId, 0, EmptySha256);
        }
        if (!string.Equals(file.Name, ownerId.ToString("N") + "-" + generationId.ToString("N") + ".utf8",
            StringComparison.OrdinalIgnoreCase))
        {
            throw new InvalidDataException("The saved document baseline file does not match its generation identity.");
        }

        var asset = DocumentAssetLease.Acquire(file, persistent: true);
        try
        {
            using (var stream = await file.OpenStreamForReadAsync().ConfigureAwait(false))
            using (var validator = new CanonicalWriteStream(Stream.Null, cancellationToken))
            {
                if (stream.Length != byteLength) throw new InvalidDataException("The saved document baseline length changed.");
                var buffer = new byte[65536];
                while (true)
                {
                    var count = await stream.ReadAsync(buffer, 0, buffer.Length, cancellationToken).ConfigureAwait(false);
                    if (count == 0) break;
                    await validator.WriteAsync(buffer, 0, count, cancellationToken).ConfigureAwait(false);
                }
                var actualHash = validator.Complete();
                if (validator.Length != byteLength || !string.Equals(actualHash, sha256, StringComparison.OrdinalIgnoreCase))
                    throw new InvalidDataException("The saved document baseline checksum changed.");
                cancellationToken.ThrowIfCancellationRequested();
                return new DocumentBaseline(asset, ownerId, generationId, byteLength, actualHash);
            }
        }
        catch
        {
            await asset.DisposeAsync().ConfigureAwait(false);
            throw;
        }
    }

    public DocumentBaseline Retain()
    {
        return new DocumentBaseline(_asset.Retain(), _ownerId, _generationId, _byteLength, _sha256);
    }

    /// <summary>The returned stream owns a separate lease until it is disposed.</summary>
    public Task<Stream> OpenReadStreamAsync(CancellationToken cancellationToken = default)
    {
        return _asset.OpenReadStreamAsync(_byteLength, cancellationToken);
    }

    /// <summary>
    /// Transfer file ownership to recovery only after its referencing manifest
    /// commits. Persistent files are subsequently removed by recovery GC.
    /// </summary>
    public void PreserveForRecovery()
    {
        _asset.PreserveForRecovery();
    }

    public void Dispose() => _asset.Dispose();

    /// <summary>Await deletion when this is the last temporary lease.</summary>
    public Task DisposeAsync()
    {
        return _asset.DisposeAsync();
    }

    private sealed class CanonicalWriteStream : Stream
    {
        [SuppressMessage("Usage", "CA2213:Disposable fields should be disposed",
            Justification = "The baseline creation transaction owns this borrowed output stream and closes it after validation and flush.")]
        private readonly Stream _stream;
        private readonly CancellationToken _cancellationToken;
        private readonly SHA256 _hash = SHA256.Create();
        private readonly Decoder _decoder = new UTF8Encoding(false, true).GetDecoder();
        private readonly char[] _characters = new char[16384];
        private long _length;
        private bool _completed;
        private bool _faulted;

        public CanonicalWriteStream(Stream stream, CancellationToken cancellationToken)
        {
            _stream = stream;
            _cancellationToken = cancellationToken;
        }

        public override bool CanRead => false;
        public override bool CanSeek => false;
        public override bool CanWrite => !_completed;
        public override long Length => _length;
        public override long Position { get => _length; set => throw new NotSupportedException(); }

        public override void Write(byte[] buffer, int offset, int count)
        {
            try
            {
                ValidateWrite(buffer, offset, count);
                _stream.Write(buffer, offset, count);
            }
            catch
            {
                _faulted = true;
                throw;
            }
        }

        public override async Task WriteAsync(byte[] buffer, int offset, int count, CancellationToken cancellationToken)
        {
            try
            {
                cancellationToken.ThrowIfCancellationRequested();
                ValidateWrite(buffer, offset, count);
                if (!cancellationToken.CanBeCanceled)
                {
                    await _stream.WriteAsync(buffer, offset, count, _cancellationToken).ConfigureAwait(false);
                }
                else
                {
                    using (var cancellation = CancellationTokenSource.CreateLinkedTokenSource(
                        cancellationToken, _cancellationToken))
                    {
                        await _stream.WriteAsync(buffer, offset, count, cancellation.Token).ConfigureAwait(false);
                    }
                }
            }
            catch
            {
                _faulted = true;
                throw;
            }
        }

        private void ValidateWrite(byte[] buffer, int offset, int count)
        {
            if (_completed) throw new ObjectDisposedException(nameof(CanonicalWriteStream));
            if (_faulted) throw new InvalidOperationException("A failed canonical writer cannot publish a generation.");
            _cancellationToken.ThrowIfCancellationRequested();
            if (buffer == null) throw new ArgumentNullException(nameof(buffer));
            if (offset < 0 || count < 0 || offset > buffer.Length - count) throw new ArgumentOutOfRangeException();
            if (count > MaximumByteLength - _length)
                throw new NotSupportedException("The document exceeds the supported UTF-8 byte length.");
            ValidateUtf8(buffer, offset, count, flush: false);
            _hash.TransformBlock(buffer, offset, count, buffer, offset);
            _length += count;
        }

        private void ValidateUtf8(byte[] buffer, int offset, int count, bool flush)
        {
            bool completed;
            do
            {
                _decoder.Convert(buffer, offset, count, _characters, 0, _characters.Length, flush,
                    out var bytesUsed, out var charactersUsed, out completed);
                for (var i = 0; i < charactersUsed; i++)
                {
                    if (_characters[i] == '\n')
                        throw new InvalidDataException("Canonical document baselines require CR line endings.");
                }
                offset += bytesUsed;
                count -= bytesUsed;
            } while (!completed);
        }

        public string Complete()
        {
            if (_completed) throw new ObjectDisposedException(nameof(CanonicalWriteStream));
            if (_faulted) throw new InvalidOperationException("A failed canonical writer cannot publish a generation.");
            _cancellationToken.ThrowIfCancellationRequested();
            ValidateUtf8([], 0, 0, flush: true);
            _hash.TransformFinalBlock([], 0, 0);
            _completed = true;
            return BitConverter.ToString(_hash.Hash).Replace("-", string.Empty).ToLowerInvariant();
        }

        public override void Flush() => _stream.Flush();

        public override Task FlushAsync(CancellationToken cancellationToken) => _stream.FlushAsync(cancellationToken);

        public override int Read(byte[] buffer, int offset, int count) => throw new NotSupportedException();

        public override long Seek(long offset, SeekOrigin origin) => throw new NotSupportedException();

        public override void SetLength(long value) => throw new NotSupportedException();

        protected override void Dispose(bool disposing)
        {
            if (disposing) _hash.Dispose();
            _completed = true;
            base.Dispose(disposing);
        }
    }

}
