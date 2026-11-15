// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Threading;
using System.Threading.Tasks;
using FileAttributes = System.IO.FileAttributes;
using Notepads.Infrastructure.Threading;
using Windows.ApplicationModel;
using Windows.ApplicationModel.DataTransfer;
using Windows.Data.Json;
using Windows.Storage;
using Windows.Storage.Streams;

namespace Notepads.Infrastructure.DocumentAssets;

internal static partial class DocumentAssetClient
{
    private const string Operation = "Files.ReadDocumentAsset";
    private const string Protocol = Operation + "\n";
    private static readonly TimeSpan PollInterval = TimeSpan.FromMilliseconds(25);
    private static readonly SemaphoreSlim Gate = new(1, 1);
    private static readonly Lazy<bool> Available = new(() => File.Exists(Path.Combine(
        Package.Current.InstalledLocation.Path, "Notepads.DocumentAssets.exe")));
    private static string _directory;
    private static (string Directory, bool Connected) _retiringSession;
    private static bool _connected;
    private static SessionChanges _changes;
    private static long _lastUse;

    static partial void OverrideReadForTest(StorageFile document, string relative, ulong maximumBytes,
        CancellationToken cancellation, ref Task<IRandomAccessStream> read);
    static partial void OverrideLaunchForTest(string directory, CancellationToken cancellation, ref Task launch);

    internal static async Task<IRandomAccessStream> ReadDocumentAssetAsync(StorageFile document, string relative,
        ulong maximumBytes, CancellationToken cancellation)
    {
        Task<IRandomAccessStream> testRead = null;
        OverrideReadForTest(document, relative, maximumBytes, cancellation, ref testRead);
        if (testRead != null) return await testRead.ConfigureAwait(false);

        if (document == null || !Available.Value) return null;

        return await ReadDocumentAssetCoreAsync(document, relative, maximumBytes, cancellation).ConfigureAwait(false);
    }

    private static async Task<IRandomAccessStream> ReadDocumentAssetCoreAsync(StorageFile document, string relative,
        ulong maximumBytes, CancellationToken cancellation)
    {
        await Gate.WaitAsync(cancellation).ConfigureAwait(false);
        string staging = null, request = null, reply = null, refused = null, documentToken = null;
        DocumentAssetRandomAccessStream pendingOutput = null;
        try
        {
            using var deadline = CancellationTokenSource.CreateLinkedTokenSource(cancellation);
            deadline.CancelAfter(TimeSpan.FromSeconds(5));
            var token = deadline.Token;
            if (_directory != null && (Environment.TickCount64 - _lastUse >= 25000 ||
                !File.Exists(Path.Combine(_directory, "ready"))))
                CloseSession();

            // A previous cancellation or idle rotation may still own the only worker lease.
            await WaitForRetirementAsync(token).ConfigureAwait(false);
            token.ThrowIfCancellationRequested();
            documentToken = TryShareDocument(document);
            if (documentToken == null) return null;

            if (_directory == null)
                await ConnectAsync(token).ConfigureAwait(false);
            _lastUse = Environment.TickCount64;
            string name = Path.Combine(_directory, Guid.NewGuid().ToString("N"));
            staging = name + ".partial";
            request = name + ".request";
            reply = name + ".reply";
            refused = name + ".refused";
            var message = new JsonObject
            {
                { "operation", JsonValue.CreateStringValue(Operation) },
                { "documentToken", JsonValue.CreateStringValue(documentToken) },
                { "relative", JsonValue.CreateStringValue(relative) },
                { "maximumBytes", JsonValue.CreateNumberValue(maximumBytes) }
            };
            await File.WriteAllTextAsync(staging, message.Stringify(), token).ConfigureAwait(false);
            File.Move(staging, request);
            if (!await _changes.WaitForReplyAsync(reply, refused, token).ConfigureAwait(false)) return null;

            // A closed payload is published by rename, including a valid empty
            // note. Only the separate refusal marker reports failure.
            token.ThrowIfCancellationRequested();
            pendingOutput = OpenPublishedReply(reply, maximumBytes);
            if (pendingOutput == null) return null;
            token.ThrowIfCancellationRequested();
            // The returned stream owns physical deletion, including clones, input
            // views and reads still retiring after their view has been closed.
            var output = pendingOutput;
            pendingOutput = null;
            reply = null;
            return output;
        }
        catch (Exception error) when (error is OperationCanceledException or IOException or InvalidDataException or UnauthorizedAccessException or
            InvalidOperationException or System.Runtime.InteropServices.COMException)
        {
            CloseSession();
            cancellation.ThrowIfCancellationRequested();
            return null;
        }
        finally
        {
            try
            {
                DeleteFile(staging);
                DeleteFile(request);
                try { pendingOutput?.Dispose(); }
                finally
                {
                    DeleteFile(reply);
                    DeleteFile(refused);
                    RevokeDocumentToken(documentToken);
                }
            }
            finally { Gate.Release(); }
        }
    }

    internal static DocumentAssetRandomAccessStream OpenPublishedReply(string reply, ulong maximumBytes)
    {
        // Leave oversized replies in place until the request has retired.
        var length = new FileInfo(reply).Length;
        return (ulong)length <= maximumBytes
            ? DocumentAssetRandomAccessStream.Open(reply, maximumBytes)
            : null;
    }

    private static async Task ConnectAsync(CancellationToken cancellation)
    {
        string id = Guid.NewGuid().ToString("N");
        _directory = Path.Combine(ApplicationData.Current.LocalFolder.Path, "DocumentAssets", id);
        Directory.CreateDirectory(_directory);
        _changes = new SessionChanges(_directory);
        Task testLaunch = null;
        OverrideLaunchForTest(_directory, cancellation, ref testLaunch);
        if (testLaunch != null) await testLaunch.ConfigureAwait(false);
        else
        {
            var launch = await FullTrustProcessLauncher.LaunchFullTrustProcessForCurrentAppWithArgumentsAsync(
                FormattableString.Invariant($"--notepads-desktop-bridge {id} {Environment.ProcessId}"))
                .AsCompletionTask(cancellation).ConfigureAwait(false);
            // The failure handler closes the watcher before retiring the directory.
            if (launch.LaunchResult != FullTrustLaunchResult.Success)
                throw new IOException("Desktop bridge activation failed.");
        }

        string ready = Path.Combine(_directory, "ready");
        await _changes.WaitForFileAsync(ready, cancellation).ConfigureAwait(false);
        if (await File.ReadAllTextAsync(ready, cancellation).ConfigureAwait(false) != Protocol)
            throw new InvalidDataException("Incompatible desktop bridge protocol.");
        _connected = true;
    }

    internal sealed partial class SessionChanges : IDisposable
    {
        private readonly object _gate = new();
        private readonly FileSystemWatcher _watcher;
        private TaskCompletionSource _changed = NewCompletion();
        private Exception _error;

        internal SessionChanges(string directory)
        {
            _watcher = new FileSystemWatcher(directory)
            {
                NotifyFilter = NotifyFilters.FileName,
                IncludeSubdirectories = false
            };
            _watcher.Created += OnChanged;
            _watcher.Deleted += OnChanged;
            _watcher.Renamed += OnChanged;
            _watcher.Error += OnError;
            try { _watcher.EnableRaisingEvents = true; }
            catch { _watcher.Dispose(); throw; }
        }

        internal Task WaitForFileAsync(string path, CancellationToken cancellation) =>
            WaitForPublicationAsync(path, null, cancellation);

        internal Task<bool> WaitForReplyAsync(string reply, string refused, CancellationToken cancellation) =>
            WaitForPublicationAsync(reply, refused, cancellation);

        private async Task<bool> WaitForPublicationAsync(string path, string refused, CancellationToken cancellation)
        {
            for (;;)
            {
                Task changed;
                lock (_gate)
                {
                    if (_error != null) throw new IOException("Desktop bridge notification failed.", _error);
                    changed = _changed.Task;
                }
                // Capture the notification first: a rename between this check
                // and the await completes that captured task instead of being lost.
                if (refused != null && File.Exists(refused))
                {
                    cancellation.ThrowIfCancellationRequested();
                    return false;
                }
                if (File.Exists(path))
                {
                    cancellation.ThrowIfCancellationRequested();
                    return true;
                }
                await changed.WaitAsync(cancellation).ConfigureAwait(false);
            }
        }

        private static TaskCompletionSource NewCompletion() =>
            new(TaskCreationOptions.RunContinuationsAsynchronously);

        private void OnChanged(object sender, FileSystemEventArgs args) => Signal(null);
        private void OnError(object sender, ErrorEventArgs args) => Signal(args.GetException());

        private void Signal(Exception error)
        {
            TaskCompletionSource changed;
            lock (_gate)
            {
                _error ??= error;
                changed = _changed;
                _changed = NewCompletion();
            }
            changed.TrySetResult();
        }

        public void Dispose()
        {
            _watcher.Dispose();
            Signal(new ObjectDisposedException(nameof(SessionChanges)));
        }
    }

    private static async Task WaitForRetirementAsync(CancellationToken cancellation)
    {
        while (!CanStartSession())
            await Task.Delay(PollInterval, cancellation).ConfigureAwait(false);
    }

    private static bool CanStartSession()
    {
        var (directory, connected) = _retiringSession;
        if (directory == null) return true;

        try
        {
            string leasePath = Path.Combine(directory, "lease");
            if ((File.GetAttributes(directory) & FileAttributes.ReparsePoint) != 0)
            {
                _retiringSession = default;
                return true;
            }

            try
            {
                if ((File.GetAttributes(leasePath) & FileAttributes.ReparsePoint) != 0)
                {
                    _retiringSession = default;
                    return true;
                }

                // The worker holds this file exclusively while servicing requests.
                using var lease = File.Open(leasePath, FileMode.Open, FileAccess.Read, FileShare.Delete);
                RemoveRetiredSessionFiles(directory);
                DeleteFile(leasePath);
            }
            catch (FileNotFoundException)
            {
                // A connected worker already acquired this lease. If it removed
                // the lease during retirement, reclaim any close marker sent late.
                // A worker that never connected may still be starting.
                if (connected) RemoveRetiredSessionFiles(directory);
            }
        }
        catch (FileNotFoundException) { }
        catch (DirectoryNotFoundException) { }
        catch (IOException) { return false; }
        catch (UnauthorizedAccessException) { return false; }

        try { Directory.Delete(directory); }
        catch (IOException) { }
        catch (UnauthorizedAccessException) { }
        _retiringSession = default;
        return true;
    }

    private static void RemoveRetiredSessionFiles(string directory)
    {
        var options = new EnumerationOptions { AttributesToSkip = FileAttributes.ReparsePoint };
        foreach (string path in Directory.EnumerateFiles(directory, "*", options))
        {
            string name = Path.GetFileName(path);
            if (name is "ready" or "ready.partial" or "close")
            {
                DeleteFile(path);
                continue;
            }

            var stem = Path.GetFileNameWithoutExtension(name).AsSpan();
            if (Path.GetExtension(name) is ".partial" or ".request" or ".reply" or ".refused" &&
                stem.Length == 32 && stem.IndexOfAnyExcept("0123456789abcdef") < 0)
                DeleteFile(path);
        }
    }

    private static void CloseSession()
    {
        string directory = _directory;
        _directory = null;
        _changes?.Dispose();
        _changes = null;
        if (directory == null) return;
        _retiringSession = (directory, _connected);
        _connected = false;
        try
        {
            if (!Directory.Exists(directory)) return;
            if (!File.Exists(Path.Combine(directory, "ready")))
            {
                // If activation failed before the worker initialized, no process owns this directory.
                try { Directory.Delete(directory); return; }
                catch (IOException) { }
            }
            File.WriteAllText(Path.Combine(directory, "close"), string.Empty);
        }
        catch (IOException) { }
        catch (UnauthorizedAccessException) { }
    }

    private static string TryShareDocument(StorageFile document)
    {
        // Windows can grant a token only for a file the caller can already access.
        try { return SharedStorageAccessManager.AddFile(document); }
        catch (Exception error) when (IsDocumentTokenError(error)) { return null; }
    }

    private static void RevokeDocumentToken(string token)
    {
        if (token == null) return;
        try { SharedStorageAccessManager.RemoveFile(token); }
        catch (Exception error) when (IsDocumentTokenError(error)) { }
    }

    private static bool IsDocumentTokenError(Exception error) =>
        error is ArgumentException or IOException or UnauthorizedAccessException or NotSupportedException or
            InvalidOperationException or System.Runtime.InteropServices.COMException;

    private static void DeleteFile(string path)
    {
        if (path == null) return;
        try { File.Delete(path); }
        catch (IOException) { }
        catch (UnauthorizedAccessException) { }
    }
}
