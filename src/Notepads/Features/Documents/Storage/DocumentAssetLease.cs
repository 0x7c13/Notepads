// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.IO;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Infrastructure.Diagnostics;
using Windows.Storage;

namespace Notepads.Features.Documents.Storage;

/// <summary>
/// Shared ownership for document assets. Acquisition and deletion reservation
/// use the same gate, so GC cannot race a new baseline or journal reader.
/// </summary>
internal sealed class DocumentAssetLease : IDisposable
{
    private static readonly object RegistrySync = new();
    private static readonly Dictionary<string, SharedAsset> Registry =
        new(StringComparer.OrdinalIgnoreCase);

    private readonly SharedAsset _asset;
    private bool _disposed;

    private DocumentAssetLease(SharedAsset asset)
    {
        _asset = asset;
    }

    public StorageFile File => _asset.File;

    public static async Task<DocumentAssetLease> CreateNewAsync(StorageFolder folder, string fileName,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        var path = Path.Combine(folder.Path, fileName);
        SharedAsset asset;
        lock (RegistrySync)
        {
            if (Registry.ContainsKey(path)) throw new IOException("The document generation already has an owner.");
            asset = new SharedAsset(null, path) { LeaseCount = 1 };
            Registry.Add(path, asset);
        }
        var lease = new DocumentAssetLease(asset);
        try
        {
            var file = await folder.CreateFileAsync(fileName, CreationCollisionOption.FailIfExists);
            lock (RegistrySync) asset.File = file;
            cancellationToken.ThrowIfCancellationRequested();
            return lease;
        }
        catch
        {
            await lease.DisposeAsync().ConfigureAwait(false);
            throw;
        }
    }

    public static DocumentAssetLease Acquire(StorageFile file, bool persistent = false)
    {
        lock (RegistrySync)
        {
            SharedAsset asset;
            if (file == null)
            {
                asset = new SharedAsset(null, null);
            }
            else if (!Registry.TryGetValue(file.Path, out asset))
            {
                asset = new SharedAsset(file, file.Path);
                Registry.Add(file.Path, asset);
            }
            if (asset.DeletionReserved)
                throw new IOException("The document generation is being removed and cannot be acquired.");
            if (file != null && asset.File == null)
                throw new IOException("The document generation has not completed creation.");
            asset.LeaseCount++;
            asset.Persistent |= persistent;
            return new DocumentAssetLease(asset);
        }
    }

    public DocumentAssetLease Retain()
    {
        lock (RegistrySync)
        {
            ThrowIfDisposed();
            _asset.LeaseCount++;
            return new DocumentAssetLease(_asset);
        }
    }

    public async Task<Stream> OpenReadStreamAsync(long byteLength, CancellationToken cancellationToken)
    {
        if (byteLength < 0) throw new ArgumentOutOfRangeException(nameof(byteLength));
        cancellationToken.ThrowIfCancellationRequested();
        var lease = Retain();
        Stream stream = null;
        try
        {
            if (lease.File == null)
            {
                stream = new MemoryStream([], writable: false);
            }
            else
            {
                stream = await lease.File.OpenStreamForReadAsync().ConfigureAwait(false);
            }

            cancellationToken.ThrowIfCancellationRequested();
            if (stream.Length < byteLength) throw new EndOfStreamException("The document generation is shorter than its committed length.");
            return new LeasedReadStream(stream, lease, byteLength);
        }
        catch
        {
            try { stream?.Dispose(); }
            finally { await lease.DisposeAsync().ConfigureAwait(false); }
            throw;
        }
    }

    public void PreserveForRecovery()
    {
        lock (RegistrySync)
        {
            ThrowIfDisposed();
            _asset.Persistent = true;
        }
    }

    public void Dispose() => Release();

    public Task DisposeAsync() => Release();

    private Task Release()
    {
        SharedAsset delete = null;
        Task completion;
        lock (RegistrySync)
        {
            if (!_disposed)
            {
                _disposed = true;
                _asset.LeaseCount--;
                if (_asset.File != null && _asset.LeaseCount == 0 && !_asset.Persistent)
                {
                    ReserveDeletion(_asset);
                    delete = _asset;
                }
                else if (_asset.File == null && _asset.Path != null && _asset.LeaseCount == 0)
                {
                    Registry.Remove(_asset.Path);
                }
            }
            completion = _asset.Deletion?.Task ?? Task.CompletedTask;
        }
        if (delete != null) _ = DeleteReservedAsync(delete);
        return completion;
    }

    /// <summary>
    /// The caller must first prove that no committed manifest references this
    /// owned file. This method additionally prevents deletion of any live lease.
    /// </summary>
    public static async Task<bool> TryDeleteUnusedAsync(StorageFile file)
    {
        if (file == null) throw new ArgumentNullException(nameof(file));
        SharedAsset asset;
        Task completion;
        var startDelete = false;
        lock (RegistrySync)
        {
            if (!Registry.TryGetValue(file.Path, out asset))
            {
                asset = new SharedAsset(file, file.Path);
                Registry.Add(file.Path, asset);
            }
            if (asset.LeaseCount != 0) return false;
            if (!asset.DeletionReserved)
            {
                ReserveDeletion(asset);
                startDelete = true;
            }
            completion = asset.Deletion.Task;
        }
        if (startDelete) _ = DeleteReservedAsync(asset);
        await completion.ConfigureAwait(false);
        return asset.Deleted;
    }

    private static void ReserveDeletion(SharedAsset asset)
    {
        asset.DeletionReserved = true;
        asset.Deletion = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
    }

    private static async Task DeleteReservedAsync(SharedAsset asset)
    {
        var deleted = false;
        try
        {
            await asset.File.DeleteAsync(StorageDeleteOption.PermanentDelete);
            deleted = true;
        }
        catch (FileNotFoundException)
        {
            deleted = true;
        }
        catch (Exception ex)
        {
            LoggingService.LogError($"[{nameof(DocumentAssetLease)}] Failed to remove generation [{asset.File.Name}]: {ex}");
        }
        TaskCompletionSource<bool> completion;
        lock (RegistrySync)
        {
            asset.Deleted = deleted;
            asset.DeletionReserved = false;
            completion = asset.Deletion;
            if (deleted) Registry.Remove(asset.Path);
        }
        completion.TrySetResult(deleted);
    }

    private void ThrowIfDisposed()
    {
        if (_disposed) throw new ObjectDisposedException(nameof(DocumentAssetLease));
    }

    private sealed class SharedAsset
    {
        public StorageFile File;
        public readonly string Path;
        public int LeaseCount;
        public bool Persistent;
        public bool DeletionReserved;
        public bool Deleted;
        public TaskCompletionSource<bool> Deletion;

        public SharedAsset(StorageFile file, string path) { File = file; Path = path; }
    }

    private sealed class LeasedReadStream : Stream
    {
        private readonly Stream _stream;
        private readonly DocumentAssetLease _lease;
        private readonly long _length;
        private long _position;
        private int _disposed;

        public LeasedReadStream(Stream stream, DocumentAssetLease lease, long length)
        {
            _stream = stream;
            _lease = lease;
            _length = length;
        }

        public override bool CanRead => _stream.CanRead;
        public override bool CanSeek => _stream.CanSeek;
        public override bool CanWrite => false;
        public override long Length => _length;
        public override long Position { get => _position; set => Seek(value, SeekOrigin.Begin); }

        public override int Read(byte[] buffer, int offset, int count)
        {
            ValidateBuffer(buffer, offset, count);
            var read = _stream.Read(buffer, offset, (int)Math.Min(count, _length - _position));
            return CompleteRead(read, count);
        }

        public override async Task<int> ReadAsync(byte[] buffer, int offset, int count, CancellationToken cancellationToken)
        {
            ValidateBuffer(buffer, offset, count);
            var read = await _stream.ReadAsync(buffer, offset,
                (int)Math.Min(count, _length - _position), cancellationToken).ConfigureAwait(false);
            return CompleteRead(read, count);
        }

        private int CompleteRead(int read, int requested)
        {
            if (read == 0 && requested != 0 && _position < _length)
                throw new EndOfStreamException("The document generation ended within its committed prefix.");
            _position += read;
            return read;
        }

        private static void ValidateBuffer(byte[] buffer, int offset, int count)
        {
            if (buffer == null) throw new ArgumentNullException(nameof(buffer));
            if (offset < 0 || count < 0 || offset > buffer.Length - count) throw new ArgumentOutOfRangeException();
        }

        public override long Seek(long offset, SeekOrigin origin)
        {
            var position = origin == SeekOrigin.Begin ? offset :
                origin == SeekOrigin.Current ? checked(_position + offset) :
                origin == SeekOrigin.End ? checked(_length + offset) : throw new ArgumentOutOfRangeException(nameof(origin));
            if (position < 0 || position > _length) throw new IOException("Cannot seek outside the committed document prefix.");
            _position = _stream.Seek(position, SeekOrigin.Begin);
            return _position;
        }

        public override void Flush() => _stream.Flush();
        public override void SetLength(long value) => throw new NotSupportedException();
        public override void Write(byte[] buffer, int offset, int count) => throw new NotSupportedException();

        protected override void Dispose(bool disposing)
        {
            if (disposing && Interlocked.Exchange(ref _disposed, 1) == 0)
            {
                try { _stream.Dispose(); }
                finally { _lease.Dispose(); }
            }
            base.Dispose(disposing);
        }
    }
}
