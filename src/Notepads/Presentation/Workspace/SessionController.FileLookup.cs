// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Sessions.Recovery;
using Notepads.Features.Sessions.Validation;
using Windows.Storage;

namespace Notepads.Presentation.Workspace;

internal sealed partial class SessionController
{
    public async Task<WorkspaceTab> ResolveFileAsync(StorageFile file, CancellationToken cancellationToken = default)
    {
        using var lifetime = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken, FileOperationToken);
        var cancellation = lifetime.Token;
        CheckFileLookupLifetime(cancellation);
        if (file == null) return null;
        if (_restoreRequested && _recoveryAdmission == RecoveryAdmission.Pending)
            await _sessionOperations.RunAsync(DiscoverWorkspaceAsync, cancellation);
        CheckFileLookupLifetime(cancellation);
        // A lookup can retry failed discovery. Admission alone does not start
        // loading, including when the file belongs to the already selected tab.
        StartRestoreWorker();

        while (await FindRecoveryForFileAsync(file, cancellation) is { } tab)
        {
            CheckFileLookupLifetime(cancellation);
            if (tab.State == WorkspaceTabState.Closed) throw new OperationCanceledException(cancellation);
            _priorityTab = tab;
            SignalRestoration();
            StartRestoreWorker();
            using var rowLifetime = CancellationTokenSource.CreateLinkedTokenSource(cancellation, tab.LifetimeToken);
            while (!tab.LoadCompletion.IsCompleted)
            {
                CheckFileLookupLifetime(cancellation);
                await Task.WhenAny(tab.LoadCompletion, _restorationChanged.Task).WaitAsync(rowLifetime.Token);
            }
            CheckFileLookupLifetime(cancellation);
            rowLifetime.Token.ThrowIfCancellationRequested();
            if (tab.State == WorkspaceTabState.Failed)
                throw new SessionFileRecoveryException(tab.LoadError);
            if (tab.ReadyEditor == null || !_notepadsCore.ContainsTab(tab))
                throw new OperationCanceledException(cancellation);
            // Recovery may use an older Save As association or detach the file.
            // Re-evaluate the live roster instead of treating its descriptor as
            // the actual binding, including any other matching recovery rows.
        }
        CheckFileLookupLifetime(cancellation);
        return _notepadsCore.GetTabs().FirstOrDefault(tab => tab.ReadyEditor?.EditingFile?.IsEqual(file) == true);
    }

    public async Task PublishTransferredEditorAsync(Func<Task> publishReceipt, Action install,
        CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(publishReceipt);
        ArgumentNullException.ThrowIfNull(install);
        using var lifetime = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken, FileOperationToken);
        var expected = _service.CurrentStamp;
        await _sessionOperations.RunAsync(async cancellation =>
        {
            CheckFileLookupLifetime(cancellation);
            if (!SessionRecoveryAuthority.SameEpoch(expected, _service.CurrentStamp))
                throw new OperationCanceledException("The transfer target recovery epoch changed.", cancellation);
            await publishReceipt();
            // Publication owns this transition now. Reset cannot retire the
            // epoch until this synchronous installation releases the coordinator.
            install();
        }, lifetime.Token);
    }

    private void CheckFileLookupLifetime(CancellationToken cancellation)
    {
        cancellation.ThrowIfCancellationRequested();
        if (_disposed || _notepadsCore.IsClosing) throw new OperationCanceledException(cancellation);
    }

    private WorkspaceTab[] RecoveryTabsForFileLookup() => _notepadsCore.GetTabs()
        .Where(tab => tab.State is WorkspaceTabState.Pending or WorkspaceTabState.Loading or WorkspaceTabState.Failed)
        .Concat(_imports).Where(tab => tab.State != WorkspaceTabState.Closed && tab.RecoverySource != null).ToArray();

    private async Task<WorkspaceTab> FindRecoveryForFileAsync(StorageFile file, CancellationToken cancellation)
    {
        while (true)
        {
            CheckFileLookupLifetime(cancellation);
            var tabs = RecoveryTabsForFileLookup();
            // Obtain each shared task before yielding: promotion/reset may retire
            // the frozen source while its file broker is resolving the binding.
            // Paths only prioritize likely matches; identity comes from the same
            // immutable result used by preparation, including a detached null.
            var bindings = tabs.SelectMany(tab => RecoveryFileBindings(tab.RecoverySource)
                .Select(binding => (Tab: tab, Source: tab.RecoverySource, binding.Index, binding.Path)))
                .OrderByDescending(binding => !string.IsNullOrEmpty(file.Path) &&
                    string.Equals(file.Path, binding.Path, StringComparison.OrdinalIgnoreCase))
                .Select(binding => (binding.Tab, File: binding.Source.ResolveEditingFileAsync(binding.Index)))
                .ToArray();
            foreach (var binding in bindings)
            {
                var resolved = await binding.File.WaitAsync(cancellation);
                CheckFileLookupLifetime(cancellation);
                if (resolved?.IsEqual(file) == true) return binding.Tab;
            }
            CheckFileLookupLifetime(cancellation);
            if (tabs.SequenceEqual(RecoveryTabsForFileLookup())) return null;
        }
    }

    private static IEnumerable<(int Index, string Path)> RecoveryFileBindings(RecoveryDocumentSource source)
    {
        yield return (-1, source.GetEditingFilePath());
        // A corrupt newest snapshot may fall back across a Save As. Match that
        // older file too, respecting GetDescriptor's forced-untitled recovery.
        for (var index = 0; index < source.FallbackCount; index++)
            yield return (index, source.GetEditingFilePath(index));
    }
}

// The file-open UI translates this failure using its existing recovery message.
// Keep source errors intact for diagnostics without reading view resources here.
internal sealed class SessionFileRecoveryException(Exception cause)
    : IOException("A matching session document could not be restored; its recovery source is retained.", cause);
