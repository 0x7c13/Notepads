// ---------------------------------------------------------------------------------------------
// Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
// See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Preferences;
using Notepads.Features.Sessions;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Recovery;
using Notepads.Features.Sessions.Validation;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Presentation.Controls.TextEditor;
using Windows.UI.Core;

namespace Notepads.Presentation.Workspace;

internal sealed partial class SessionController
{
    private SessionRecoveryPlan _restorePlan;
    private readonly List<WorkspaceTab> _imports = [];
    private readonly HashSet<WorkspaceTab> _demands = [];
    private WorkspaceTab _priorityTab;
    private CancellationTokenSource _restoreLifetime = new();
    private Task _restoreWorker = Task.CompletedTask;
    private Task<int> _initialRestore;
    private bool _workerRunning;
    private bool _restorationPaused;
    private TaskCompletionSource<object> _restorationChanged = NewSignal();
    private bool _restoreCheckpointRequested;
    private Task _restoreCheckpointWork = Task.CompletedTask;
    private bool _unclaimedRecoveryArchived;
    private Exception _restoreDrainFailure;
    private SessionRecoveryOutcome _reportedRecoveryOutcome;
    private int _reportedUnrecoveredCount;
    private bool _restorationBackupFailureReported;

    public event EventHandler RecoveryStateChanged;
    public event EventHandler RestorationBackupFailed;

    // A deterministic boundary for the source-linked integration hosts. It runs
    // before payload I/O, outside session serialization, and owns no production state.
    internal Func<RecoveryDocumentSource, CancellationToken, Task> BeforeRestorePreparation { get; set; }
    internal Func<CancellationToken, Task> BeforeRecoveryDiscovery { get; set; }
    internal Func<RecoveryDocumentSource, ITextEditor, CancellationToken, Task> BeforeRestorePromotion { get; set; }

    private static TaskCompletionSource<object> NewSignal() => new(TaskCreationOptions.RunContinuationsAsynchronously);

    private void ThrowIfRestoreDrainFailed()
    {
        if (_restoreDrainFailure != null)
            throw new InvalidOperationException("A recovery editor did not release its native work; session ownership must remain held.", _restoreDrainFailure);
    }

    public Task<int> LoadLastSessionAsync()
    {
        if (_initialRestore != null) return _initialRestore;
        if (_loaded) return Task.FromResult(0);
        return _initialRestore = LoadWorkspaceAsync();
    }

    private async Task<int> LoadWorkspaceAsync()
    {
        await _sessionOperations.RunAsync(async cancellation =>
        {
            if (_loaded) return;
            if (BeforeRecoveryDiscovery != null) await BeforeRecoveryDiscovery(cancellation);
            _restorePlan = await Task.Run(() => _service.DiscoverRecoveryAsync(cancellation));
            var owned = _restorePlan.Sources.Where(source => source.IsOwned).Select(source => new WorkspaceTab(source)).ToArray();
            _imports.AddRange(_restorePlan.Sources.Where(source => !source.IsOwned).Select(source => new WorkspaceTab(source)));
            if (_disposed) return;
            // Index membership is required even if close began during discovery.
            // Native promotion stays paused, but terminal capture must retain these rows.
            _notepadsCore.OpenTabs(owned, _restorePlan.SelectedEditorId, allowWhileClosing: true);
            _notepadsCore.SetTabScrollViewerHorizontalOffset(_restorePlan.TabScrollOffset);
            _priorityTab = _notepadsCore.GetSelectedTab() ??
                _imports.FirstOrDefault(tab => tab.Id == _restorePlan.SelectedEditorId) ?? _imports.FirstOrDefault();
            _loaded = true;
        });

        if (_disposed || !_loaded) return 0;
        var selected = _notepadsCore.GetSelectedTab() ?? _priorityTab;
        StartRestoreWorker();
        try
        {
            if (selected != null) await selected.LoadCompletion.WaitAsync(_restoreLifetime.Token);
            else await WaitForRestorationAsync();
            if (_notepadsCore.GetTabCount() == 0) await WaitForRestorationAsync();
        }
        catch (OperationCanceledException) when (_disposed || _notepadsCore.IsClosing) { }
        if (!_disposed && IsBackupEnabled && !_notepadsCore.IsClosing)
        {
            StartSessionBackup();
            RequestRestorationCheckpoint();
        }
        return _notepadsCore.GetTabCount();
    }

    public async Task<ITextEditor> EnsureLoadedAsync(WorkspaceTab tab, bool allowWhileClosing = false)
    {
        if (_disposed || tab == null || !_notepadsCore.ContainsTab(tab) ||
            (_notepadsCore.IsClosing && !allowWhileClosing)) return null;
        if (tab.ReadyEditor != null) return tab.ReadyEditor;
        if (tab.State is WorkspaceTabState.Closed or WorkspaceTabState.Failed) return null;
        _priorityTab = tab;
        if (allowWhileClosing) _demands.Add(tab);
        SignalRestoration();
        StartRestoreWorker();
        try { return await tab.LoadCompletion.WaitAsync(_restoreLifetime.Token); }
        catch (OperationCanceledException) { return null; }
        finally { if (allowWhileClosing) _demands.Remove(tab); }
    }

    public void PauseRestoration()
    {
        _restorationPaused = true;
        _notepadsCore.IsEditorPresentationPaused = true;
        SignalRestoration();
    }

    public void ResumeRestoration()
    {
        if (_disposed) return;
        if (_restoreLifetime.IsCancellationRequested && !_workerRunning)
        {
            _restoreLifetime.Dispose();
            _restoreLifetime = new CancellationTokenSource();
        }
        _restorationPaused = false;
        _notepadsCore.IsEditorPresentationPaused = false;
        _notepadsCore.ResumeEditorPresentation();
        SignalRestoration();
        StartRestoreWorker();
    }

    private void SignalRestoration()
    {
        var previous = _restorationChanged;
        _restorationChanged = NewSignal();
        previous.TrySetResult(null);
    }

    private void OnTabSelected(object sender, WorkspaceTab tab)
    {
        if (tab?.State == WorkspaceTabState.Pending) _priorityTab = tab;
        if (_loaded) StartRestoreWorker();
    }

    private WorkspaceTab NextRestoreTab()
    {
        bool Pending(WorkspaceTab tab) => tab != null && tab.State == WorkspaceTabState.Pending && _notepadsCore.ContainsTab(tab);
        if (_restorationPaused || _notepadsCore.IsClosing)
            return _demands.FirstOrDefault(Pending);
        if (Pending(_priorityTab) || (_priorityTab?.State == WorkspaceTabState.Pending && _imports.Contains(_priorityTab))) return _priorityTab;
        return _notepadsCore.GetTabs().FirstOrDefault(Pending) ??
            _imports.FirstOrDefault(tab => tab.State == WorkspaceTabState.Pending);
    }

    private void StartRestoreWorker()
    {
        if (_disposed || !_loaded || _workerRunning || _restoreLifetime.IsCancellationRequested || NextRestoreTab() == null) return;
        _workerRunning = true;
        _restoreWorker = RestoreRowsAsync();
    }

    public async Task WaitForRestorationAsync()
    {
        do
        {
            StartRestoreWorker();
            await _restoreWorker;
        }
        while (!_disposed && !_restoreLifetime.IsCancellationRequested && NextRestoreTab() != null);
    }

    internal async Task WaitForRestorationCheckpointAsync()
    {
        await WaitForRestorationAsync();
        await _restoreCheckpointWork;
    }

    private async Task RestoreRowsAsync()
    {
        // Assign _restoreWorker before a synchronous failure can finish this loop.
        await Task.Yield();
        try
        {
            while (!_disposed && !_restoreLifetime.IsCancellationRequested && NextRestoreTab() is { } tab)
            {
                if (!tab.BeginLoading()) continue;
                await RestoreTabAsync(tab);
                ReportRecoveryState();
                await _dispatcher.RunAsync(CoreDispatcherPriority.Low, () => { });
            }
        }
        catch (OperationCanceledException) when (_restoreLifetime.IsCancellationRequested) { }
        catch (Exception ex) { LoggingService.LogError($"[{nameof(SessionController)}] Restoration stopped: {ex}"); }
        finally
        {
            _workerRunning = false;
            TryReleaseRestorePlan();
            if (!_disposed && !_restoreLifetime.IsCancellationRequested) RequestRestorationCheckpoint();
        }
    }

    private async Task WaitForPromotionAsync(CancellationToken cancellation)
    {
        // A closing save may demand another tab while this row has finished. Let
        // it finish offscreen too, so the single worker can reach the demand.
        while ((_restorationPaused || _notepadsCore.IsClosing) && _demands.Count == 0)
            await _restorationChanged.Task.WaitAsync(cancellation);
        cancellation.ThrowIfCancellationRequested();
    }

    private void ReportRecoveryState()
    {
        var outcome = RecoveryOutcome;
        var failed = UnrecoveredEditorCount;
        if (_disposed || (outcome == _reportedRecoveryOutcome && failed == _reportedUnrecoveredCount)) return;
        _reportedRecoveryOutcome = outcome;
        _reportedUnrecoveredCount = failed;
        RecoveryStateChanged?.Invoke(this, EventArgs.Empty);
    }

    private async Task<PreparedRecoveryDocument> PrepareDocumentAsync(RecoveryDocumentSource source, CancellationToken cancellation)
    {
        if (BeforeRestorePreparation != null) await BeforeRestorePreparation(source, cancellation);
        var options = new DocumentLoadOptions(configuredDefaultEncoding: ApplicationPreferences.EditorDefaultDecoding);
        try
        {
            return await Task.Run(() => RecoveryDocumentPreparation.PrepareAsync(source,
                _notepadsCore.DocumentOwnerId, options, cancellation));
        }
        catch (OperationCanceledException) { throw; }
        catch (Exception ex)
        {
            LoggingService.LogError($"[{nameof(SessionController)}] Newest recovery [{source.Id}] is unavailable: {ex}");
            if (source.Descriptor != null)
                await _sessionOperations.RunAsync(token => Task.Run(() => _service.PreserveFailedNewestAsync(source, token)), cancellation);
            for (var index = 0; index < source.FallbackCount; index++)
            {
                try
                {
                    var fallback = index;
                    return await Task.Run(() => RecoveryDocumentPreparation.PrepareAsync(source,
                        _notepadsCore.DocumentOwnerId, options, cancellation, fallback));
                }
                catch (OperationCanceledException) { throw; }
                catch (Exception older) { LoggingService.LogError($"[{nameof(SessionController)}] Older recovery [{source.Id}] is unavailable: {older.Message}"); }
            }
            throw;
        }
    }

    private async Task RestoreTabAsync(WorkspaceTab tab)
    {
        var source = tab.RecoverySource;
        using var cancellation = CancellationTokenSource.CreateLinkedTokenSource(_restoreLifetime.Token, tab.LifetimeToken);
        var token = cancellation.Token;
        ITextEditor candidate = null;
        var installed = false;
        try
        {
            using var prepared = await PrepareDocumentAsync(source, token);
            token.ThrowIfCancellationRequested();
            if (prepared == null)
            {
                // A V1 row with neither accessible file nor content has nothing to adopt.
                if (source.IsOwned) throw new InvalidOperationException("An owned recovery document has no content source.");
                _imports.Remove(tab);
                tab.MarkClosed();
                return;
            }
            await WaitForPromotionAsync(token);
            candidate = _notepadsCore.CreateTextEditor(tab.Id, prepared.EditingFile,
                prepared.FileNamePlaceholder, allowWhileClosing: _demands.Count != 0);
            if (prepared.Checkpoint != null)
                await candidate.RestoreV2Async(prepared.SavedSnapshot, prepared.RecoveryBaseline,
                    prepared.Journal, prepared.Checkpoint, prepared.EditingFile, prepared.Metadata, prepared.TextDirty);
            else if (prepared.InitializeFromFile)
            {
                await candidate.InitAsync(prepared.SavedSnapshot, prepared.EditingFile);
                candidate.ResetEditorState(prepared.Metadata);
            }
            else
                await candidate.RestoreAsync(prepared.SavedSnapshot, prepared.EditingFile, prepared.Metadata, prepared.RecoveryBaseline);
            token.ThrowIfCancellationRequested();

            if (BeforeRestorePromotion != null) await BeforeRestorePromotion(source, candidate, token);
            while (!installed)
            {
                await WaitForPromotionAsync(token);
                var rejected = false;
                await _sessionOperations.RunAsync(async lifetime =>
                {
                    token.ThrowIfCancellationRequested();
                    if ((_restorationPaused || _notepadsCore.IsClosing) && _demands.Count == 0) return;
                    if (_restorePlan == null || !SessionRecoveryAuthority.SameEpoch(_restorePlan.ExpectedStamp, _service.CurrentStamp) ||
                        tab.State != WorkspaceTabState.Loading || (source.IsOwned && !_notepadsCore.ContainsTab(tab)))
                    {
                        rejected = true;
                        return;
                    }
                    tab.FileAccessBinding = prepared.FileAccessBinding;
                    if (source.IsOwned)
                    {
                        installed = _notepadsCore.AttachEditor(tab, candidate, showEditor: !_restorationPaused && !_notepadsCore.IsClosing);
                        rejected = !installed;
                    }
                    else
                    {
                        var capture = CaptureEditor(candidate, source, prepared.FileAccessBinding);
                        using var recovery = capture.Recovery;
                        await Task.Run(() => _service.PublishAdoptionAsync(capture, _service.CurrentStamp, lifetime));
                        // Adoption is the durable ownership boundary. Finish membership
                        // even when close began during publication, before final capture.
                        if (!tab.TrySetReady(candidate)) throw new InvalidOperationException("The adopted document cannot be installed.");
                        var preceding = _restorePlan.Sources.TakeWhile(item => item.Id != tab.Id).Select(item => item.Id).ToHashSet();
                        var atIndex = Array.FindLastIndex(_notepadsCore.GetTabs(), existing => preceding.Contains(existing.Id)) + 1;
                        _notepadsCore.OpenTab(tab, atIndex, select: _notepadsCore.GetTabCount() == 0,
                            allowWhileClosing: true);
                        tab.CompleteLoading();
                        _imports.Remove(tab);
                        // Adoption protects the document; the ordinary checkpoint
                        // still has to commit its workspace order and selection.
                        installed = true;
                        _maintenanceDue = true;
                    }
                }, token);
                if (rejected) throw new OperationCanceledException("The recovery document is no longer owned by this workspace.", token);
            }
        }
        catch (OperationCanceledException) { tab.ReturnToPending(); }
        catch (Exception ex)
        {
            tab.FailLoading(ex);
            LoggingService.LogError($"[{nameof(SessionController)}] Failed to restore [{tab.Id}]: {ex}");
        }
        finally
        {
            if (!installed && candidate != null)
            {
                try
                {
                    // Native work and its writers must finish before releasing source pins.
                    await _notepadsCore.DiscardUnattachedEditorAsync(candidate);
                }
                catch (Exception ex)
                {
                    _restoreDrainFailure ??= ex;
                    throw;
                }
            }
        }
    }

    private void RequestRestorationCheckpoint()
    {
        if (_disposed || !IsBackupEnabled || _notepadsCore.IsClosing || _restorationPaused ||
            !_notepadsCore.GetTabs().Any(tab => tab.ReadyEditor != null && tab.RecoverySource != null)) return;
        _restoreCheckpointRequested = true;
        if (_restoreCheckpointWork.IsCompleted) _restoreCheckpointWork = FlushRestorationCheckpointsAsync();
    }

    private async Task FlushRestorationCheckpointsAsync()
    {
        await Task.Yield();
        while (_restoreCheckpointRequested && !_disposed && IsBackupEnabled && !_notepadsCore.IsClosing && !_restorationPaused)
        {
            _restoreCheckpointRequested = false;
            await _dispatcher.RunAsync(CoreDispatcherPriority.Low, () => { });
            if (!_disposed && !_notepadsCore.IsClosing && !_restorationPaused)
            {
                var saved = await SaveSessionAsync();
                if (!saved && !_disposed && IsBackupEnabled && !_notepadsCore.IsClosing &&
                    !_restorationBackupFailureReported)
                {
                    _restorationBackupFailureReported = true;
                    RestorationBackupFailed?.Invoke(this, EventArgs.Empty);
                }
            }
        }
    }

    private void TryReleaseRestorePlan()
    {
        if (_restorePlan == null || _restoreDrainFailure != null || _workerRunning || _imports.Count != 0 ||
            _notepadsCore.GetTabs().Any(tab => tab.State is WorkspaceTabState.Pending or WorkspaceTabState.Loading ||
                (tab.State == WorkspaceTabState.Ready && tab.RecoverySource != null))) return;
        _restorePlan.Dispose();
        _restorePlan = null;
        _maintenanceDue = true;
        _ = RunMaintenanceAsync();
    }
}
