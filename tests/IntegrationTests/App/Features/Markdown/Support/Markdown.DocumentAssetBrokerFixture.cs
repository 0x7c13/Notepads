// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Concurrent;
using System.IO;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Infrastructure.DocumentAssets;
using Windows.Data.Json;

namespace NotepadsIntegrationTests;

// Replace only activation. The real client still shares/revokes its StorageFile
// token, creates its watcher, serializes requests and owns published file streams.
// This fixture does not claim native handle-containment or process-activation coverage.
internal sealed class DocumentAssetBrokerFixture : IAsyncDisposable
{
    private readonly string _ready;
    private readonly Func<string, byte[]> _read;
    private readonly CancellationTokenSource _lifetime = new();
    private Task _work = Task.CompletedTask;

    internal DocumentAssetBrokerFixture(Func<string, byte[]> read, string ready = "Files.ReadDocumentAsset\n")
    {
        if (DocumentAssetClient.TestRead != null || DocumentAssetClient.TestLaunch != null ||
            DocumentAssetClient.TestHasActiveSession)
            throw new InvalidOperationException("The broker fixture needs an unused client.");
        _read = read;
        _ready = ready;
        DocumentAssetClient.TestRead = DocumentAssetClient.TestReadBrokerAsync;
        DocumentAssetClient.TestLaunch = LaunchAsync;
    }

    internal ConcurrentQueue<(string Directory, DocumentAssetClient.SessionChanges Changes)> Sessions { get; } = new();
    internal ConcurrentQueue<JsonObject> Requests { get; } = new();

    private Task LaunchAsync(string directory, CancellationToken cancellation)
    {
        cancellation.ThrowIfCancellationRequested();
        Sessions.Enqueue((directory, DocumentAssetClient.TestSessionChanges));
        _work = Task.WhenAll(_work, ServeAsync(directory, _lifetime.Token));
        return Task.CompletedTask;
    }

    private async Task ServeAsync(string directory, CancellationToken cancellation)
    {
        try
        {
            using var lease = File.Open(Path.Combine(directory, "lease"), FileMode.CreateNew,
                FileAccess.ReadWrite, FileShare.None);
            string ready = Path.Combine(directory, "ready");
            await File.WriteAllTextAsync(ready + ".partial", _ready, cancellation);
            File.Move(ready + ".partial", ready);
            while (!File.Exists(Path.Combine(directory, "close")))
            {
                cancellation.ThrowIfCancellationRequested();
                foreach (string request in Directory.GetFiles(directory, "*.request"))
                {
                    string reply = Path.ChangeExtension(request, ".reply");
                    string refused = Path.ChangeExtension(request, ".refused");
                    if (File.Exists(reply) || File.Exists(refused)) continue;
                    JsonObject message;
                    try { message = JsonObject.Parse(await File.ReadAllTextAsync(request, cancellation)); }
                    catch (FileNotFoundException) { continue; }
                    Requests.Enqueue(message);
                    byte[] bytes = _read(message.GetNamedString("relative"));
                    string staged = Path.ChangeExtension(request, ".partial");
                    await File.WriteAllBytesAsync(staged, bytes ?? Array.Empty<byte>(), cancellation);
                    File.Move(staged, bytes == null ? refused : reply);
                }
                await Task.Delay(10, cancellation);
            }
        }
        catch (OperationCanceledException) when (cancellation.IsCancellationRequested) { }
        finally
        {
            await RetireDirectoryAsync(directory);
        }
    }

    private static async Task RetireDirectoryAsync(string directory)
    {
        // Seeing the close marker does not mean the client has closed its handle.
        var deadline = Environment.TickCount64 + 5000;
        while (Directory.Exists(directory))
        {
            try
            {
                foreach (string path in Directory.GetFiles(directory)) File.Delete(path);
                Directory.Delete(directory);
                return;
            }
            catch (DirectoryNotFoundException) { return; }
            catch (Exception error) when ((error is IOException or UnauthorizedAccessException) &&
                                          Environment.TickCount64 < deadline)
            {
                await Task.Delay(10);
            }
        }
    }

    public async ValueTask DisposeAsync()
    {
        try
        {
            using var deadline = new CancellationTokenSource(TimeSpan.FromSeconds(5));
            await DocumentAssetClient.TestRetireSessionAsync(deadline.Token);
        }
        finally
        {
            _lifetime.Cancel();
            try { await _work; }
            finally
            {
                DocumentAssetClient.TestRead = null;
                DocumentAssetClient.TestLaunch = null;
                _lifetime.Dispose();
            }
        }
    }
}
