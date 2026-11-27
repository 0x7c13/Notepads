// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Net.Http;
using System.Runtime.InteropServices;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Markdown;
using Notepads.Infrastructure.DocumentAssets;
using Notepads.Infrastructure.Storage;
using Notepads.Infrastructure.Threading;
using Windows.Foundation;
using Windows.Storage;
using Windows.Storage.Streams;

namespace Notepads.Presentation.Controls.Markdown;

[WinRT.GeneratedWinRTExposedType]
internal sealed partial class MarkdownImageProvider : IMarkdownImageProvider, IDisposable
{
    // Host buffering is separately bounded; it is not part of the engine's ledger.
    private const ulong MaximumHostEncodedBytes = MarkdownDataImage.MaximumEncodedBytes;
    private static readonly SemaphoreSlim HostImageSlots = new(2, 2);
    private static readonly TimeSpan DownloadTimeout = TimeSpan.FromSeconds(30);
    // The holder is initialized only for HTTP images. Provider/request cancellation
    // and payload ownership stay local while connections are reused process-wide.
    private static class ImageHttpTransport
    {
        internal static readonly HttpClient Client = new(new SocketsHttpHandler
        {
            UseCookies = false,
            MaxConnectionsPerServer = 2,
            PooledConnectionIdleTimeout = TimeSpan.FromMinutes(1),
            PooledConnectionLifetime = TimeSpan.FromMinutes(5)
        }) { Timeout = Timeout.InfiniteTimeSpan };
    }

    private readonly object _gate = new();
    private readonly StorageFile _documentFile;
    private readonly string _grantDirectory;
    private readonly string _documentDirectory;
    private readonly CancellationTokenSource _lifetime = new();
    private readonly CancellationToken _lifetimeToken;
    private readonly TaskCompletionSource _completion = new(TaskCreationOptions.RunContinuationsAsynchronously);
    private int _pending;
    private bool _disposed;
    private bool _cancellationCompleted;
    private bool _completed;

    internal MarkdownImageProvider(StorageFile documentFile, string brokerRelativeDocument = null)
    {
        _documentFile = documentFile;
        string documentPath = documentFile?.Path;
        _grantDirectory = string.IsNullOrEmpty(documentPath) ? null : Path.GetDirectoryName(documentPath);
        _documentDirectory = brokerRelativeDocument == null ? _grantDirectory :
            Path.GetDirectoryName(Path.Combine(_grantDirectory, brokerRelativeDocument));
        _lifetimeToken = _lifetime.Token;
    }

    // Completes after Dispose has canceled and all actual provider IO has retired.
    internal Task Completion => _completion.Task;

    public IAsyncOperation<IRandomAccessStream> OpenAsync(MarkdownImageRequest request)
    {
        ArgumentNullException.ThrowIfNull(request);
        string destination = request.Destination;
        ulong maximumBytes = Math.Min(request.MaximumEncodedBytes, MaximumHostEncodedBytes);
        lock (_gate)
        {
            ObjectDisposedException.ThrowIf(_disposed, this);
            _pending = checked(_pending + 1);
        }

        try
        {
            return AsyncInfo.Run<IRandomAccessStream>(cancellation =>
                OpenTrackedAsync(destination, maximumBytes, cancellation));
        }
        catch
        {
            RetireOperation();
            throw;
        }
    }

    private async Task<IRandomAccessStream> OpenTrackedAsync(string destination, ulong maximumBytes,
        CancellationToken cancellation)
    {
        IRandomAccessStream stream = null;
        try
        {
            using var linked = CancellationTokenSource.CreateLinkedTokenSource(cancellation, _lifetimeToken);
            CancellationToken token = linked.Token;
            token.ThrowIfCancellationRequested();
            if (maximumBytes == 0 || string.IsNullOrWhiteSpace(destination))
            {
                return null;
            }

            if (destination.StartsWith("data:", StringComparison.OrdinalIgnoreCase))
            {
                stream = await OpenDataImageAsync(destination, maximumBytes, token).ConfigureAwait(false);
            }
            else if (TryGetDestination(destination, out string localPath, out Uri remoteUri))
            {
                stream = remoteUri != null
                    ? await DownloadAsync(remoteUri, maximumBytes, token).ConfigureAwait(false)
                    : await OpenLocalAsync(localPath, maximumBytes, token).ConfigureAwait(false);
            }

            token.ThrowIfCancellationRequested();
            lock (_gate)
            {
                if (_disposed)
                {
                    throw new OperationCanceledException(_lifetimeToken);
                }
            }

            // The control owns Close only after this successful handoff.
            IRandomAccessStream result = stream;
            stream = null;
            return result;
        }
        finally
        {
            stream?.Dispose();
            RetireOperation();
        }
    }

    private static async Task<IRandomAccessStream> OpenDataImageAsync(string destination, ulong maximumBytes,
        CancellationToken cancellation)
    {
        // Inline payloads share the network path's two bounded host-buffer slots.
        await HostImageSlots.WaitAsync(cancellation).ConfigureAwait(false);
        InMemoryRandomAccessStream output = null;
        try
        {
            byte[] bytes = await Task.Run(() => MarkdownDataImage.Decode(destination, maximumBytes, cancellation),
                cancellation).ConfigureAwait(false);
            if (bytes == null) return null;

            output = new InMemoryRandomAccessStream();
            int offset = 0;
            while (offset < bytes.Length)
            {
                cancellation.ThrowIfCancellationRequested();
                int count = Math.Min(32 * 1024, bytes.Length - offset);
                var write = output.WriteAsync(bytes.AsBuffer(offset, count));
                var completion = write.AsTask();
                using var registration = cancellation.Register(write.Cancel);
                uint written = await completion.ConfigureAwait(false);
                cancellation.ThrowIfCancellationRequested();
                if (written == 0 || written > (uint)count)
                    throw new IOException("The Markdown image stream could not be written.");
                offset += (int)written;
            }
            cancellation.ThrowIfCancellationRequested();
            output.Seek(0);
            var result = output;
            output = null;
            return result;
        }
        finally
        {
            try { output?.Dispose(); }
            finally { HostImageSlots.Release(); }
        }
    }

    private static bool TryGetDestination(string destination, out string localPath, out Uri remoteUri) =>
        MarkdownDestination.TryParse(destination, wiki: false, allowMailto: false, out localPath, out remoteUri, out _);

    private async Task<IRandomAccessStream> OpenLocalAsync(string localPath, ulong maximumBytes,
        CancellationToken cancellation)
    {
        IRandomAccessStream stream;
        try
        {
            StorageFile file;
            bool fullyQualified = Path.IsPathFullyQualified(localPath);
            if (fullyQualified || !string.IsNullOrEmpty(_documentDirectory))
            {
                if (!fullyQualified && localPath.Contains(':'))
                {
                    return null;
                }
                string targetPath = fullyQualified
                    ? Path.GetFullPath(localPath)
                    : Path.GetFullPath(Path.Combine(_documentDirectory, localPath));
                string grantRelative = null;
                if (!string.IsNullOrEmpty(_documentDirectory) &&
                    TryGetChildPath(_documentDirectory, targetPath, out _) &&
                    TryGetChildPath(_grantDirectory, targetPath, out string relative))
                {
                    grantRelative = relative;
                    var asset = await DocumentAssetClient.ReadDocumentAssetAsync(_documentFile, relative, maximumBytes,
                        cancellation).ConfigureAwait(false);
                    if (asset != null) return asset;
                }
                file = await ResolveLocalFileAsync(targetPath, grantRelative, cancellation).ConfigureAwait(false);
            }
            else
            {
                file = await ResolveVirtualFileAsync(localPath, cancellation).ConfigureAwait(false);
            }

            cancellation.ThrowIfCancellationRequested();
            if (file == null)
            {
                return null;
            }

            stream = await FileStorage.OpenReadAsync(file, cancellation: cancellation).ConfigureAwait(false);
        }
        catch (Exception exception) when (exception is FileNotFoundException or DirectoryNotFoundException or
            UnauthorizedAccessException or ArgumentException or NotSupportedException or PathTooLongException)
        {
            cancellation.ThrowIfCancellationRequested();
            return null;
        }
        try
        {
            cancellation.ThrowIfCancellationRequested();
            if (stream.Size > maximumBytes)
            {
                throw new InvalidDataException("The Markdown image exceeds its encoded byte limit.");
            }
            stream.Seek(0);
            return stream;
        }
        catch
        {
            stream.Dispose();
            throw;
        }
    }

    private async Task<StorageFile> ResolveLocalFileAsync(string targetPath, string grantRelative,
        CancellationToken cancellation)
    {
        if (grantRelative != null)
        {
            try
            {
                StorageFolder parent = await FileStorage.GetParentAsync(_documentFile, cancellation).ConfigureAwait(false);
                if (parent != null && string.Equals(Path.TrimEndingDirectorySeparator(parent.Path),
                    Path.TrimEndingDirectorySeparator(_grantDirectory), StringComparison.OrdinalIgnoreCase))
                {
                    return await parent.GetFileAsync(grantRelative)
                        .AsCompletionTask(cancellation).ConfigureAwait(false);
                }
            }
            catch (UnauthorizedAccessException)
            {
                // The path can still be readable to the app without a brokered parent.
                cancellation.ThrowIfCancellationRequested();
            }
        }

        return await StorageFile.GetFileFromPathAsync(targetPath)
            .AsCompletionTask(cancellation).ConfigureAwait(false);
    }

    private async Task<StorageFile> ResolveVirtualFileAsync(string localPath, CancellationToken cancellation)
    {
        if (_documentFile == null || localPath.Contains(':'))
        {
            return null;
        }

        // A virtual StorageFile/Folder may have no filesystem Path.
        // Resolve only clean relative segments through its authorized parent.
        foreach (string segment in localPath.Split('\\'))
        {
            if (segment.Length == 0 || segment is "." or "..")
            {
                return null;
            }
        }
        StorageFolder parent = await FileStorage.GetParentAsync(_documentFile, cancellation).ConfigureAwait(false);
        return parent == null ? null :
            await parent.GetFileAsync(localPath).AsCompletionTask(cancellation).ConfigureAwait(false);
    }

    private static bool TryGetChildPath(string directory, string targetPath, out string relative)
    {
        relative = Path.GetRelativePath(directory, targetPath);
        return relative != "." && relative != ".." && !Path.IsPathRooted(relative) &&
            !relative.StartsWith("..\\", StringComparison.Ordinal);
    }

    private static async Task<IRandomAccessStream> DownloadAsync(Uri uri, ulong maximumBytes,
        CancellationToken cancellation)
    {
        using var timeout = CancellationTokenSource.CreateLinkedTokenSource(cancellation);
        timeout.CancelAfter(DownloadTimeout);
        CancellationToken token = timeout.Token;
        InMemoryRandomAccessStream output = null;
        try
        {
            await HostImageSlots.WaitAsync(token).ConfigureAwait(false);
            try
            {
                using var response = await ImageHttpTransport.Client.GetAsync(uri, HttpCompletionOption.ResponseHeadersRead, token)
                    .ConfigureAwait(false);
                if (!response.IsSuccessStatusCode)
                {
                    // Standard FACILITY_HTTP HRESULTs preserve the status across WinRT.
                    int status = (int)response.StatusCode;
                    throw new COMException($"Markdown image returned HTTP {status}.", unchecked((int)0x80190000) | status);
                }
                if (response.Content.Headers.ContentLength is long length && length > (long)maximumBytes)
                {
                    throw new InvalidDataException("The Markdown image exceeds its encoded byte limit.");
                }

                using Stream input = await response.Content.ReadAsStreamAsync(token).ConfigureAwait(false);
                output = new InMemoryRandomAccessStream();
                byte[] buffer = new byte[32 * 1024];
                ulong total = 0;
                while (true)
                {
                    // Reading one extra byte distinguishes exact limit from excess.
                    int requested = (int)Math.Min((ulong)buffer.Length, maximumBytes - total + 1);
                    int count = await input.ReadAsync(buffer.AsMemory(0, requested), token).ConfigureAwait(false);
                    if (count == 0)
                    {
                        break;
                    }
                    total += (ulong)count;
                    if (total > maximumBytes)
                    {
                        throw new InvalidDataException("The Markdown image exceeds its encoded byte limit.");
                    }

                    int offset = 0;
                    while (offset < count)
                    {
                        var write = output.WriteAsync(buffer.AsBuffer(offset, count - offset));
                        var completion = write.AsTask();
                        using var registration = token.Register(write.Cancel);
                        uint written = await completion.ConfigureAwait(false);
                        token.ThrowIfCancellationRequested();
                        if (written == 0 || written > (uint)(count - offset))
                        {
                            throw new IOException("The Markdown image stream could not be written.");
                        }
                        offset += (int)written;
                    }
                }
            }
            finally
            {
                HostImageSlots.Release();
            }

            token.ThrowIfCancellationRequested();
            output.Seek(0);
            InMemoryRandomAccessStream result = output;
            output = null;
            return result;
        }
        catch (OperationCanceledException) when (!cancellation.IsCancellationRequested && timeout.IsCancellationRequested)
        {
            // HRESULT_FROM_WIN32(ERROR_TIMEOUT) distinguishes a timed-out request from user cancellation.
            throw new COMException("The Markdown image request timed out.", unchecked((int)0x800705B4));
        }
        finally
        {
            // This also covers failures while the response/client are being disposed.
            output?.Dispose();
        }
    }

    public void Dispose()
    {
        lock (_gate)
        {
            if (_disposed)
            {
                return;
            }
            _disposed = true;
        }

        try
        {
            _lifetime.Cancel();
        }
        finally
        {
            lock (_gate)
            {
                _cancellationCompleted = true;
            }
            CompleteIfRetired();
        }
    }

    private void RetireOperation()
    {
        lock (_gate)
        {
            --_pending;
        }
        CompleteIfRetired();
    }

    private void CompleteIfRetired()
    {
        lock (_gate)
        {
            if (!_disposed || !_cancellationCompleted || _pending != 0 || _completed)
            {
                return;
            }
            _completed = true;
        }
        _lifetime.Dispose();
        _completion.TrySetResult();
    }
}
