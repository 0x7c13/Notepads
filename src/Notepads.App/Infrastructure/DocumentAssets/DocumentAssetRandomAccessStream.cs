// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Diagnostics.CodeAnalysis;
using System.IO;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Threading;
using System.Threading.Tasks;
using Windows.Foundation;
using Windows.Storage.Streams;

namespace Notepads.Infrastructure.DocumentAssets;

// The pinned Stream adapter has no CloneStream/GetInputStreamAt implementation.
// Supply independent views here while sharing only the physical file owner.
[WinRT.GeneratedWinRTExposedType]
internal sealed partial class DocumentAssetRandomAccessStream : IRandomAccessStream
{
    private readonly DocumentAssetFileView _view;
    private readonly IRandomAccessStream _adapter;

    private DocumentAssetRandomAccessStream(DocumentAssetFileView view)
    {
        _view = view;
        try { _adapter = view.AsRandomAccessStream(); }
        catch { view.Dispose(); throw; }
    }

    internal static DocumentAssetRandomAccessStream Open(string path, ulong maximumBytes)
    {
        var owner = new DocumentAssetFileOwner(path);
        if ((ulong)owner.Length > maximumBytes)
        {
            owner.Release();
            return null;
        }
        return new DocumentAssetRandomAccessStream(new DocumentAssetFileView(owner, 0));
    }

    public bool CanRead => _view.CanRead;
    public bool CanWrite => false;
    public ulong Position => (ulong)_view.Position;
    public ulong Size
    {
        get => (ulong)_view.Length;
        set => throw new NotSupportedException("Document asset streams are read-only.");
    }

    public IRandomAccessStream CloneStream() => new DocumentAssetRandomAccessStream(_view.CreateView(0));
    public IInputStream GetInputStreamAt(ulong position)
    {
        var view = _view.CreateView(CheckedPosition(position));
        try { return view.AsInputStream(); }
        catch { view.Dispose(); throw; }
    }
    public IOutputStream GetOutputStreamAt(ulong position) =>
        throw new NotSupportedException("Document asset streams are read-only.");
    public void Seek(ulong position) => _view.Seek(CheckedPosition(position), SeekOrigin.Begin);
    public IAsyncOperationWithProgress<IBuffer, uint> ReadAsync(IBuffer buffer, uint count, InputStreamOptions options) =>
        _adapter.ReadAsync(buffer, count, options);
    public IAsyncOperationWithProgress<uint, uint> WriteAsync(IBuffer buffer) =>
        throw new NotSupportedException("Document asset streams are read-only.");
    public IAsyncOperation<bool> FlushAsync() => _adapter.FlushAsync();
    public void Dispose() => _adapter.Dispose();

    private static long CheckedPosition(ulong position) => position <= long.MaxValue
        ? (long)position : throw new ArgumentOutOfRangeException(nameof(position));
}

[SuppressMessage("Design", "CA1001:Types that own disposable fields should be disposable",
    Justification = "A shared physical owner closes its file only when Release retires the final view or actual read.")]
internal sealed class DocumentAssetFileOwner
{
    private readonly object _gate = new();
    private readonly FileStream _file;
    private readonly string _directory;
    private int _references = 1;

    internal DocumentAssetFileOwner(string path)
    {
        _directory = Path.GetDirectoryName(path);
        _file = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.Read | FileShare.Delete,
            1, FileOptions.Asynchronous | FileOptions.RandomAccess | FileOptions.DeleteOnClose);
        try { Length = _file.Length; }
        catch { _file.Dispose(); throw; }
    }

    internal long Length { get; }

    internal void AddReference()
    {
        lock (_gate)
        {
            ObjectDisposedException.ThrowIf(_references == 0, this);
            _references = checked(_references + 1);
        }
    }

    internal ValueTask<int> ReadAsync(Memory<byte> buffer, long position, CancellationToken cancellation) =>
        RandomAccess.ReadAsync(_file.SafeFileHandle, buffer, position, cancellation);

    internal void Release()
    {
        lock (_gate)
        {
            if (--_references != 0) return;
        }
        try { _file.Dispose(); }
        finally
        {
            // Worker retirement may have been unable to remove this directory
            // while a returned stream still owned a reply file.
            try { Directory.Delete(_directory); }
            catch (IOException) { }
            catch (UnauthorizedAccessException) { }
        }
    }
}

internal sealed partial class DocumentAssetFileView : Stream
{
    private readonly object _gate = new();
    private readonly SemaphoreSlim _reads = new(1, 1);
    private DocumentAssetFileOwner _owner;
    private long _position;
    private int _pending;

    internal DocumentAssetFileView(DocumentAssetFileOwner owner, long position)
    {
        _owner = owner;
        _position = position;
    }

    internal DocumentAssetFileView CreateView(long position)
    {
        lock (_gate)
        {
            ObjectDisposedException.ThrowIf(_owner == null, this);
            _owner.AddReference();
            try { return new DocumentAssetFileView(_owner, position); }
            catch { _owner.Release(); throw; }
        }
    }

    public override bool CanRead { get { lock (_gate) return _owner != null; } }
    public override bool CanSeek => CanRead;
    public override bool CanWrite => false;
    public override long Length
    {
        get
        {
            lock (_gate)
            {
                ObjectDisposedException.ThrowIf(_owner == null, this);
                return _owner.Length;
            }
        }
    }
    public override long Position
    {
        get
        {
            lock (_gate)
            {
                ObjectDisposedException.ThrowIf(_owner == null, this);
                return _position;
            }
        }
        set => Seek(value, SeekOrigin.Begin);
    }

    public override long Seek(long offset, SeekOrigin origin)
    {
        lock (_gate)
        {
            ObjectDisposedException.ThrowIf(_owner == null, this);
            long position = origin switch
            {
                SeekOrigin.Begin => offset,
                SeekOrigin.Current => checked(_position + offset),
                SeekOrigin.End => checked(_owner.Length + offset),
                _ => throw new ArgumentOutOfRangeException(nameof(origin))
            };
            if (position < 0) throw new ArgumentOutOfRangeException(nameof(offset));
            return _position = position;
        }
    }

    public override int Read(byte[] buffer, int offset, int count) =>
        ReadAsync(buffer.AsMemory(offset, count)).AsTask().GetAwaiter().GetResult();
    public override Task<int> ReadAsync(byte[] buffer, int offset, int count, CancellationToken cancellation) =>
        ReadAsync(buffer.AsMemory(offset, count), cancellation).AsTask();

    public override async ValueTask<int> ReadAsync(Memory<byte> buffer, CancellationToken cancellation = default)
    {
        DocumentAssetFileOwner owner;
        lock (_gate)
        {
            ObjectDisposedException.ThrowIf(_owner == null, this);
            owner = _owner;
            // Closing this view cannot close a physical read still retiring.
            int pending = checked(_pending + 1);
            owner.AddReference();
            _pending = pending;
        }
        try
        {
            await _reads.WaitAsync(cancellation).ConfigureAwait(false);
            try
            {
                long position;
                lock (_gate) position = _position;
                int requested = (int)Math.Min(buffer.Length, Math.Max(0, owner.Length - position));
                int read = await owner.ReadAsync(buffer[..requested], position, cancellation).ConfigureAwait(false);
                lock (_gate) _position = checked(position + read);
                return read;
            }
            finally { _reads.Release(); }
        }
        finally
        {
            try { owner.Release(); }
            finally
            {
                lock (_gate)
                {
                    if (--_pending == 0 && _owner == null) _reads.Dispose();
                }
            }
        }
    }

    protected override void Dispose(bool disposing)
    {
        DocumentAssetFileOwner owner;
        lock (_gate)
        {
            owner = _owner;
            _owner = null;
            if (owner != null && _pending == 0) _reads.Dispose();
        }
        owner?.Release();
        base.Dispose(disposing);
    }

    public override void Flush() { }
    public override void SetLength(long value) => throw new NotSupportedException();
    public override void Write(byte[] buffer, int offset, int count) => throw new NotSupportedException();
}
