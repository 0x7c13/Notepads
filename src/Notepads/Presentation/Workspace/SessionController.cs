// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Diagnostics;
using System.Diagnostics.CodeAnalysis;
using System.IO;
using System.Linq;
using System.Text;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Documents.Operations;
using Notepads.Features.Documents.Storage;
using Notepads.Features.Preferences;
using Notepads.Features.Sessions;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Recovery;
using Notepads.Features.Sessions.Validation;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Presentation.Controls.TextEditor;
using Notepads.Presentation.Helpers;
using Windows.ApplicationModel;
using Windows.Storage;
using Windows.Storage.AccessCache;
using Windows.System;
using Windows.UI.Core;
using Windows.UI.Xaml;
using WinUIEditor;

namespace Notepads.Presentation.Workspace;

internal sealed partial class SessionController : ISessionController, ISessionPersistenceParticipant
{
    private static readonly TimeSpan SaveInterval = TimeSpan.FromSeconds(7);
    private static readonly TimeSpan MaintenanceInterval = TimeSpan.FromSeconds(60);
    private readonly INotepadsCore _notepadsCore;
    private readonly CoreDispatcher _dispatcher;
    // Each step starts on the UI thread and runs its SessionService call on the pool with one awaited
    // Task.Run, so session file I/O never blocks the UI. Native restoration and pure source preparation
    // run outside this gate; only capture, publication and workspace adoption/retirement serialize here.
    [SuppressMessage("Usage", "CA2213:Disposable fields should be disposed",
        Justification = "Dispose stops the coordinator; DrainAsync waits for its Completion before releasing session ownership.")]
    private readonly DocumentOperationCoordinator _sessionOperations = new();
    private readonly ConcurrentDictionary<Guid, long> _editorRevisions = new();
    private readonly HashSet<ITextEditor> _trackedEditors = new();
    private readonly SessionService _service;
    private bool _disposed;
    // Maintenance state is only touched on the UI thread: saves, the startup pass and the timer's UI step.
    private bool _startupMaintenanceQueued;
    private bool _maintenanceDue;
    private long _lastMaintenance;
    private readonly object _timerSync = new();
    private Task _timerWork = Task.CompletedTask;
    private Timer _timer;

    private readonly IDisposable _registration;
    private Task _disposal;

    public SessionController(INotepadsCore notepadsCore, SessionService service, CoreDispatcher dispatcher,
        Func<ISessionPersistenceParticipant, IDisposable> registerSession)
    {
        _notepadsCore = notepadsCore ?? throw new ArgumentNullException(nameof(notepadsCore));
        _dispatcher = dispatcher ?? throw new ArgumentNullException(nameof(dispatcher));
        _service = service ?? throw new ArgumentNullException(nameof(service));
        _registration = registerSession(this);

        foreach (var tab in _notepadsCore.GetTabs())
        {
            BindEditorContentStateChangeEvent(this, tab);
        }

        _notepadsCore.EditorAttached += BindEditorContentStateChangeEvent;
        _notepadsCore.TabClosed += UnbindEditorContentStateChangeEvent;
        _notepadsCore.TabSelected += OnTabSelected;
    }

    public bool IsBackupEnabled
    {
        get => _service.IsBackupEnabled;
        set => _service.IsBackupEnabled = value;
    }

    public int UnrecoveredEditorCount => _imports.Count(tab => tab.State == WorkspaceTabState.Failed) +
        _notepadsCore.GetTabs().Count(tab => tab.State == WorkspaceTabState.Failed);

    public SessionRecoveryOutcome RecoveryOutcome => UnrecoveredEditorCount != 0 && _service.RecoveryOutcome != SessionRecoveryOutcome.Blocked
        ? SessionRecoveryOutcome.Partial : _service.RecoveryOutcome;

    public bool IsRecoveryBlocked => _service.IsRecoveryBlocked;

    public Task InitializeAuthorityAsync() => _sessionOperations.RunAsync(
        cancellation => Task.Run(() => _service.InitializeAuthorityAsync(cancellation)));

    public async Task OpenSessionBackupFolderAsync()
    {
        // The index, journals, baselines and V1 backups are sibling folders.
        // Show their common root so a manual copy contains every recovery asset.
        await Launcher.LaunchFolderAsync(ApplicationData.Current.LocalFolder);
    }

    public async Task<bool> SaveSessionAsync(Action actionAfterSaving = null,
        CancellationToken cancellationToken = default)
    {
        if (_disposed) return false;
        try
        {
            // Timer callbacks run on the pool. Native editor state must be captured
            // on its owning UI thread; only the persistence calls hop to the pool.
            if (!_dispatcher.HasThreadAccess)
            {
                var completion = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
                var started = 0;
                var dispatched = _dispatcher.RunAsync(CoreDispatcherPriority.Low, async () =>
                {
                    Interlocked.Exchange(ref started, 1);
                    try { completion.TrySetResult(await SaveSessionAsync(actionAfterSaving, cancellationToken)); }
                    catch (Exception ex) { completion.TrySetException(ex); }
                });
                try { await dispatched.AsTask(cancellationToken); }
                catch (OperationCanceledException) when (Volatile.Read(ref started) != 0)
                {
                    // Once capture has started, await its commit outcome. A
                    // deadline cannot turn a durable publication into failure.
                    return await completion.Task;
                }
                return await completion.Task;
            }
            if (!IsBackupEnabled)
            {
                LoggingService.LogInfo($"[{nameof(SessionController)}] Session backup is disabled.");
                return false;
            }

            var result = false;
            await _sessionOperations.RunAsync(async cancellation =>
                result = await SaveSessionCoreAsync(actionAfterSaving, cancellation), cancellationToken);
            return result;
        }
        catch (OperationCanceledException) { return false; }
        catch (ObjectDisposedException) { return false; }
        catch (Exception ex)
        {
            LoggingService.LogError($"[{nameof(SessionController)}] Failed to schedule session backup: {ex}");
            return false;
        }
    }

    public async Task<bool> PrepareExplicitCloseAsync(IReadOnlyCollection<Guid> editorIds, ICollection<Guid> closed = null)
    {
        if (_disposed) return false;
        var succeeded = false;
        try
        {
            await _sessionOperations.RunAsync(async cancellation =>
            {
                var confirmed = new HashSet<Guid>();
                try { succeeded = await Task.Run(() => _service.PrepareExplicitCloseAsync(editorIds, confirmed, cancellation)); }
                finally
                {
                    foreach (var id in confirmed) closed?.Add(id);
                    var retired = succeeded ? editorIds : (IEnumerable<Guid>)confirmed;
                    foreach (var tab in _notepadsCore.GetTabs().Where(tab => retired.Contains(tab.Id))) tab.MarkClosed();
                    SignalRestoration();
                }
            });
            return succeeded;
        }
        catch (Exception ex)
        {
            LoggingService.LogError($"[{nameof(SessionController)}] Failed to record explicit tab closure: {ex}");
            return false;
        }
    }

    public void StartSessionBackup(bool startImmediately = false)
    {
        if (_disposed) return;
        if (_timer == null)
        {
            LoggingService.LogInfo($"[{nameof(SessionController)}] Session backup process started (StartImmediately = {startImmediately}).");

            Timer timer = new Timer(obj => BeginTimerSave());

            if (Interlocked.CompareExchange(ref _timer, timer, null) == null)
            {
                var delay = startImmediately ? TimeSpan.Zero : SaveInterval;
                timer.Change(delay, SaveInterval);
            }
            else
            {
                timer.Dispose();
            }
        }
    }

    private void BeginTimerSave()
    {
        lock (_timerSync)
        {
            if (_disposed || _timer == null || !_timerWork.IsCompleted) return;
            _timerWork = SaveFromTimerAsync();
        }
    }

    private async Task SaveFromTimerAsync()
    {
        // A periodic tick owns its whole save-and-maintenance chain. Slow
        // storage/compaction must not queue another chain every seven seconds.
        try
        {
            if (await SaveSessionAsync())
            {
                await _dispatcher.CallOnUIThreadAsync(async () =>
                {
                    if (_disposed || !IsBackupEnabled || _notepadsCore.IsClosing) return;
                    foreach (var editor in LiveTabs().Select(tab => tab.ReadyEditor).Where(editor => editor != null))
                    {
                        if (_disposed || !IsBackupEnabled || _notepadsCore.IsClosing) break;
                        // One editor's failed compaction must not stall the others or the maintenance pass.
                        try { await editor.MaintainRecoveryAsync(); }
                        catch (Exception ex) when (ex is not OperationCanceledException)
                        {
                            LoggingService.LogError($"[{nameof(SessionController)}] Journal compaction deferred for [{editor.Id}]: {ex.Message}");
                        }
                    }
                    // Idle windows publish nothing new, so they skip maintenance; busy ones run it once a minute.
                    if (_maintenanceDue && Stopwatch.GetElapsedTime(_lastMaintenance) >= MaintenanceInterval)
                        await RunMaintenanceAsync();
                });
            }
        }
        catch (Exception ex) { LoggingService.LogError($"[{nameof(SessionController)}] Background backup failed: {ex}"); }
    }

    public Task RunStartupMaintenanceAsync()
    {
        if (_startupMaintenanceQueued) return Task.CompletedTask;
        _startupMaintenanceQueued = true;
        return RunMaintenanceAsync();
    }

    // A queued step of its own, never part of a save; a pass still queued when the window starts closing is skipped.
    private async Task RunMaintenanceAsync()
    {
        try
        {
            await _sessionOperations.RunAsync(async cancellation =>
            {
                if (_disposed || _notepadsCore.IsClosing) return;
                _maintenanceDue = false;
                _lastMaintenance = Stopwatch.GetTimestamp();
                await Task.Run(() => _service.RunMaintenanceAsync(cancellation));
            });
        }
        catch (Exception ex) when (ex is OperationCanceledException or ObjectDisposedException) { }
        catch (Exception ex) { LoggingService.LogError($"[{nameof(SessionController)}] Recovery maintenance was deferred: {ex}"); }
    }

    public void StopSessionBackup()
    {
        Timer timer;
        lock (_timerSync)
        {
            timer = _timer;
            _timer = null;
        }

        try
        {
            timer?.Dispose();
        }
        catch
        {
            // Best effort
        }
        finally
        {
            Interlocked.CompareExchange(ref _timer, null, timer);
        }
    }

    private void BindEditorContentStateChangeEvent(object sender, WorkspaceTab tab)
    {
        var textEditor = tab.AttachedEditor;
        if (textEditor == null) return;
        if (!_trackedEditors.Add(textEditor)) return;
        _editorRevisions.TryAdd(textEditor.Id, 0);
        // All text or file related events
        textEditor.TextChanging += OnEditorDocumentStateChanged;
        textEditor.ChangeReverted += OnEditorDocumentStateChanged;
        textEditor.FileSaved += OnEditorDocumentStateChanged;
        textEditor.FileReloaded += OnEditorDocumentStateChanged;
        textEditor.EncodingChanged += OnEditorDocumentStateChanged;
        textEditor.LineEndingChanged += OnEditorDocumentStateChanged;
        textEditor.FileRenamed += OnEditorDocumentStateChanged;
        textEditor.LanguageOverrideChanged += OnEditorDocumentStateChanged;
    }

    private void UnbindEditorContentStateChangeEvent(object sender, WorkspaceTab tab)
    {
        UnbindEditor(tab.AttachedEditor);
        SignalRestoration();
        TryReleaseRestorePlan();
    }

    private void UnbindEditor(ITextEditor textEditor)
    {
        if (textEditor == null) return;
        if (!_trackedEditors.Remove(textEditor)) return;
        // All text or file related events
        textEditor.TextChanging -= OnEditorDocumentStateChanged;
        textEditor.ChangeReverted -= OnEditorDocumentStateChanged;
        textEditor.FileSaved -= OnEditorDocumentStateChanged;
        textEditor.FileReloaded -= OnEditorDocumentStateChanged;
        textEditor.EncodingChanged -= OnEditorDocumentStateChanged;
        textEditor.LineEndingChanged -= OnEditorDocumentStateChanged;
        textEditor.FileRenamed -= OnEditorDocumentStateChanged;
        textEditor.LanguageOverrideChanged -= OnEditorDocumentStateChanged;
        _editorRevisions.TryRemove(textEditor.Id, out _);
    }

    private void OnEditorDocumentStateChanged(object sender, EventArgs e)
    {
        if (sender is ITextEditor textEditor)
        {
            _editorRevisions.AddOrUpdate(textEditor.Id, 1, (_, version) => version + 1);
        }
    }

    public Task<bool> SaveForSuspensionAsync(CancellationToken cancellation) => SaveSessionAsync(cancellationToken: cancellation);

    public Task WaitForSessionOperationsAsync() => _sessionOperations.WaitForIdleAsync();

    public Task DrainAsync() => _disposal ?? DrainActiveWorkAsync();

    private async Task DrainActiveWorkAsync()
    {
        StopSessionBackup();
        PauseRestoration();
        _restoreLifetime.Cancel();
        SignalRestoration();
        await _restoreWorker;
        ThrowIfRestoreDrainFailed();
        await _restoreCheckpointWork;
        Task timerWork;
        lock (_timerSync) timerWork = _timerWork;
        await timerWork;
        await _sessionOperations.WaitForIdleAsync();
    }

    private async Task CompleteDisposalAsync()
    {
        await _restoreWorker;
        ThrowIfRestoreDrainFailed();
        await _restoreCheckpointWork;
        Task timerWork;
        lock (_timerSync) timerWork = _timerWork;
        await timerWork;
        await _sessionOperations.Completion;
        _restorePlan?.Dispose();
        _restorePlan = null;
        _restoreLifetime.Dispose();
        _registration.Dispose();
    }

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        IsBackupEnabled = false;
        StopSessionBackup();
        _restoreLifetime.Cancel();
        SignalRestoration();
        _sessionOperations.Dispose();
        _disposal = CompleteDisposalAsync();
        if (_notepadsCore != null)
        {
            _notepadsCore.EditorAttached -= BindEditorContentStateChangeEvent;
            _notepadsCore.TabClosed -= UnbindEditorContentStateChangeEvent;
            _notepadsCore.TabSelected -= OnTabSelected;
            foreach (var editor in _trackedEditors.ToArray())
            {
                UnbindEditor(editor);
            }
        }

    }
}
