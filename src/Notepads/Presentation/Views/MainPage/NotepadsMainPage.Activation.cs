// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Presentation.Workspace.Activation;
using Windows.Storage;

namespace Notepads.Presentation.Views.MainPage;

public sealed partial class NotepadsMainPage
{
    private readonly TaskCompletionSource<bool> _activationReady = new(TaskCreationOptions.RunContinuationsAsynchronously);
    private Task _activationTail = Task.CompletedTask;
    private readonly CancellationTokenSource _activationLifetime = new();
    private TaskCompletionSource<bool> _windowCloseDecision;

    internal Task ActivateFilesAsync(IReadOnlyList<IStorageItem> files) => QueueActivationAsync(() => OpenFilesAsync(files));

    internal Task ActivateCommandAsync(string directory, string arguments) => QueueActivationAsync(async () =>
    {
        // Resolve inside the queue so a slow command cannot be overtaken by a file activation.
        var file = await CommandLineFiles.OpenFileFromCommandLineAsync(directory, arguments, _context.ApplicationName);
        if (file != null && await CanContinueAfterCloseAsync()) await OpenFileAsync(file);
    });

    private Task QueueActivationAsync(Func<Task> operation)
    {
        if (_viewClosed) return Task.CompletedTask;
        // Activation and shutdown are dispatched on this window's UI thread.
        // Publish this request before invoking any code that could reenter activation.
        var previous = _activationTail;
        var completion = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
        _activationTail = completion.Task;
        _ = RunActivationAsync(previous, operation, completion);
        return completion.Task;
    }

    private async Task RunActivationAsync(Task previous, Func<Task> operation, TaskCompletionSource<bool> completion)
    {
        var cancellation = _activationLifetime.Token;
        try
        {
            await previous.WaitAsync(cancellation);
            if (!await _activationReady.Task.WaitAsync(cancellation) || !await CanContinueAfterCloseAsync()) return;
            // Native/document work has its own shutdown drain. UI presentation and
            // command lookup must not delay releasing the closed window's writer lease.
            await operation().WaitAsync(cancellation);
        }
        catch (OperationCanceledException) when (cancellation.IsCancellationRequested) { }
        catch (Exception ex)
        {
            // Each request settles independently, including the unawaited first navigation.
            // File-open failures already show their own dialog in OpenFileAsync.
            LoggingService.LogError($"[{nameof(NotepadsMainPage)}] External activation failed: {ex}");
        }
        finally { completion.TrySetResult(true); }
    }

    private async Task<bool> CanContinueAfterCloseAsync()
    {
        while (!_viewClosed && _isAppClosing)
        {
            if (_windowCloseDecision == null ||
                !await _windowCloseDecision.Task.WaitAsync(_activationLifetime.Token)) return false;
        }
        return !_viewClosed;
    }
}
