// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
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

internal sealed class SessionController : ISessionController, ISessionPersistenceParticipant
{
    private static readonly TimeSpan SaveInterval = TimeSpan.FromSeconds(7);
    private static readonly TimeSpan MaintenanceInterval = TimeSpan.FromSeconds(60);
    private readonly INotepadsCore _notepadsCore;
    private readonly CoreDispatcher _dispatcher;
    // Each step starts on the UI thread and runs its SessionService call on the pool with one awaited
    // Task.Run, so session file I/O never blocks the UI. Editor capture, restoration and capture disposal
    // stay on the UI thread around that hop. Steps serialize every SessionService call and mutation; UI
    // reads outside steps are limited to single-field snapshots (stamp, flags, counts).
    [SuppressMessage("Usage", "CA2213:Disposable fields should be disposed",
        Justification = "Dispose stops the coordinator; DrainAsync waits for its Completion before releasing session ownership.")]
    private readonly DocumentOperationCoordinator _sessionOperations = new();
    private readonly ConcurrentDictionary<Guid, long> _editorRevisions = new();
    private readonly HashSet<ITextEditor> _trackedEditors = new();
    private readonly SessionService _service;
    private bool _loaded;
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
        _loaded = false;

        foreach (var editor in _notepadsCore.GetAllTextEditors())
        {
            BindEditorContentStateChangeEvent(this, editor);
        }

        _notepadsCore.TextEditorOpened += BindEditorContentStateChangeEvent;
        _notepadsCore.TextEditorClosed += UnbindEditorContentStateChangeEvent;
    }

    public bool IsBackupEnabled
    {
        get => _service.IsBackupEnabled;
        set => _service.IsBackupEnabled = value;
    }

    public int UnrecoveredEditorCount => _service.UnrecoveredEditorCount;

    public SessionRecoveryOutcome RecoveryOutcome => _service.RecoveryOutcome;

    public bool IsRecoveryBlocked => _service.IsRecoveryBlocked;

    public Task InitializeAuthorityAsync() => _sessionOperations.RunAsync(
        cancellation => Task.Run(() => _service.InitializeAuthorityAsync(cancellation)));

    public async Task<int> LoadLastSessionAsync()
    {
        var count = 0;
        await _sessionOperations.RunAsync(async cancellation => count = await LoadLastSessionCoreAsync(cancellation));
        return count;
    }

    private async Task<int> LoadLastSessionCoreAsync(CancellationToken cancellation)
    {
        if (_loaded) return 0;
        var options = new DocumentLoadOptions(configuredDefaultEncoding: ApplicationPreferences.EditorDefaultDecoding);
        using (var batch = await Task.Run(() => _service.PrepareRecoveryAsync(options, cancellation)))
        {
            var recovered = new ITextEditor[batch.Documents.Count];
            try
            {
                await RecoveryRestorePipeline.RunAsync(batch.Documents, async (index, document) =>
                {
                    cancellation.ThrowIfCancellationRequested();
                    if (_disposed || _notepadsCore.IsClosing) return;
                    ITextEditor editor = null;
                    try
                    {
                        editor = _notepadsCore.CreateTextEditor(document.Id, document.EditingFile, document.FileNamePlaceholder);
                        if (document.Checkpoint != null)
                        {
                            await editor.RestoreV2Async(document.SavedSnapshot, document.RecoveryBaseline,
                                document.Journal, document.Checkpoint, document.EditingFile, document.Metadata, document.TextDirty);
                        }
                        else if (document.InitializeFromFile)
                        {
                            await editor.InitAsync(document.SavedSnapshot, document.EditingFile);
                            editor.ResetEditorState(document.Metadata);
                        }
                        else
                        {
                            await editor.RestoreAsync(document.SavedSnapshot, document.EditingFile,
                                document.Metadata, document.RecoveryBaseline);
                        }

                        recovered[index] = editor;
                    }
                    catch (Exception ex)
                    {
                        editor?.Dispose();
                        LoggingService.LogError($"[{nameof(SessionController)}] Failed to restore editor [{document.Id}]: {ex}");
                    }
                }, cancellation);
                if (!_disposed && !_notepadsCore.IsClosing)
                {
                    try { _notepadsCore.OpenTextEditors(recovered.Where(editor => editor != null).ToArray(), batch.SelectedEditorId); }
                    catch (Exception ex) { LoggingService.LogError($"[{nameof(SessionController)}] Recovered tabs could not all be attached: {ex}"); }
                }
                var attached = new HashSet<ITextEditor>(_notepadsCore.GetAllTextEditors());
                var attachedIds = recovered.Where(attached.Contains).Select(editor => editor.Id).ToArray();
                if (attachedIds.Length != 0)
                {
                    using var capture = CaptureSession(_notepadsCore.GetAllTextEditors(), batch.ExpectedStamp);
                    await Task.Run(() => _service.CommitAttachedAsync(capture, attachedIds, cancellation));
                }
                if (!_disposed && !_notepadsCore.IsClosing)
                    _notepadsCore.SetTabScrollViewerHorizontalOffset(batch.TabScrollOffset);
                _loaded = true;
                return attachedIds.Length;
            }
            finally
            {
                var attached = new HashSet<ITextEditor>(_notepadsCore.GetAllTextEditors());
                foreach (var editor in recovered.Where(editor => editor != null))
                    if (!attached.Contains(editor)) editor.Dispose();
            }
        }
    }

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

    private async Task<bool> SaveSessionCoreAsync(Action actionAfterSaving, CancellationToken cancellation)
    {
        var capture = new SessionCapture();
        try
        {
            cancellation.ThrowIfCancellationRequested();
            if (!IsBackupEnabled || _disposed) return false;
            await Task.Run(() => _service.EnsureMetadataRetainedAsync(cancellation));
            cancellation.ThrowIfCancellationRequested();
            if (!IsBackupEnabled || _disposed) return false;
            // A journal whose writer failed would fail every capture; rebase it first.
            foreach (var editor in _notepadsCore.GetAllTextEditors() ?? [])
                await editor.RepairRecoveryJournalAsync(cancellation);
            cancellation.ThrowIfCancellationRequested();
            if (!IsBackupEnabled || _disposed) return false;
            var editors = _notepadsCore.GetAllTextEditors() ?? [];
            var revisions = editors.ToDictionary(editor => editor.Id, editor => _editorRevisions.GetOrAdd(editor.Id, 0));
            var selected = _notepadsCore.GetSelectedTextEditor();
            capture.ExpectedStamp = _service.CurrentStamp;
            capture.SelectedEditorId = selected?.Id;
            capture.TabScrollOffset = _notepadsCore.GetTabScrollViewerHorizontalOffset();
            AddCapturedDocuments(capture, editors);
            var result = await Task.Run(() => _service.SaveAsync(capture, cancellation));
            if (!result.Succeeded) return false;
            // Saves never prune or collect, so close, exit and suspension stay fast. The timer
            // runs maintenance after a change, and the next startup pass covers terminal saves.
            _maintenanceDue |= result.Changed;
            if (actionAfterSaving != null)
            {
                var currentEditors = _notepadsCore.GetAllTextEditors();
                if (_disposed || currentEditors == null || currentEditors.Length != revisions.Count ||
                    currentEditors.Any(editor => !revisions.TryGetValue(editor.Id, out var revision) ||
                        _editorRevisions.GetOrAdd(editor.Id, 0) != revision))
                {
                    return false;
                }

                foreach (var document in capture.Documents)
                {
                    var editor = currentEditors.First(current => current.Id == document.Id);
                    using (var current = editor.CaptureRecoveryState())
                    {
                        if (current.Checkpoint.CommittedSequence != document.Recovery.Checkpoint.CommittedSequence ||
                            current.DocumentJournal.GenerationId != document.Recovery.DocumentJournal.GenerationId ||
                            current.RecoveryBaseline.GenerationId != document.Recovery.RecoveryBaseline.GenerationId)
                        {
                            return false;
                        }
                    }
                }
                actionAfterSaving();
            }
            return true;
        }
        catch (Exception ex)
        {
            LoggingService.LogError($"[{nameof(SessionController)}] Failed to save session: {ex}");
            AnalyticsService.TrackEvent("SessionManager_FailedToSaveSessionMetaData",
                new Dictionary<string, string> { { "Exception", ex.Message } });
            return false;
        }
        finally
        {
            // Persistence borrows the capture. It cannot outlive this awaited operation.
            foreach (var document in capture.Documents)
            {
                try { await document.Recovery.DisposeAsync(); }
                catch (Exception ex) { LoggingService.LogError($"[{nameof(SessionController)}] Failed to release captured recovery state: {ex}"); }
            }
        }
    }

    private SessionCapture CaptureSession(IEnumerable<ITextEditor> editors, RecoveryStamp stamp)
    {
        var capture = new SessionCapture
        {
            ExpectedStamp = stamp,
            SelectedEditorId = _notepadsCore.GetSelectedTextEditor()?.Id,
            TabScrollOffset = _notepadsCore.GetTabScrollViewerHorizontalOffset()
        };
        try { AddCapturedDocuments(capture, editors); return capture; }
        catch { capture.Dispose(); throw; }
    }

    private static void AddCapturedDocuments(SessionCapture capture, IEnumerable<ITextEditor> editors)
    {
        // Capture every native prefix on the owning UI thread before yielding.
        foreach (var editor in editors)
        {
            capture.Documents.Add(new SessionDocumentCapture
            {
                Id = editor.Id,
                EditingFile = editor.EditingFile,
                EditingFileName = editor.EditingFileName,
                EditingFilePath = editor.EditingFilePath,
                Recovery = editor.CaptureRecoveryState()
            });
        }
    }

    public async Task<bool> PrepareExplicitCloseAsync(IReadOnlyCollection<Guid> editorIds, ICollection<Guid> closed = null)
    {
        if (_disposed) return false;
        var succeeded = false;
        try
        {
            await _sessionOperations.RunAsync(async cancellation =>
                succeeded = await Task.Run(() => _service.PrepareExplicitCloseAsync(editorIds, closed, cancellation)));
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
                    foreach (var editor in _notepadsCore.GetAllTextEditors())
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

    public async Task ClearSessionDataAsync()
    {
        try
        {
            await _sessionOperations.RunAsync(async cancellation =>
            {
                cancellation.ThrowIfCancellationRequested();
                await Task.Run(() => _service.ClearSessionDataAsync(cancellation));
            });
        }
        catch (Exception ex)
        {
            LoggingService.LogError($"[{nameof(SessionController)}] Failed to delete session meta data: {ex.Message}");
            AnalyticsService.TrackEvent("SessionManager_FailedToDeleteSessionMetaData", new Dictionary<string, string>() { { "Exception", ex.Message } });
            throw;
        }
    }

    private void BindEditorContentStateChangeEvent(object sender, ITextEditor textEditor)
    {
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

    private void UnbindEditorContentStateChangeEvent(object sender, ITextEditor textEditor)
    {
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

    public Task DrainAsync() => _disposal ?? DrainActiveWorkAsync();

    private async Task DrainActiveWorkAsync()
    {
        StopSessionBackup();
        Task timerWork;
        lock (_timerSync) timerWork = _timerWork;
        await timerWork;
        await _sessionOperations.WaitForIdleAsync();
    }

    private async Task CompleteDisposalAsync()
    {
        Task timerWork;
        lock (_timerSync) timerWork = _timerWork;
        await timerWork;
        await _sessionOperations.Completion;
        _registration.Dispose();
    }

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        IsBackupEnabled = false;
        StopSessionBackup();
        _sessionOperations.Dispose();
        _disposal = CompleteDisposalAsync();
        if (_notepadsCore != null)
        {
            _notepadsCore.TextEditorOpened -= BindEditorContentStateChangeEvent;
            _notepadsCore.TextEditorClosed -= UnbindEditorContentStateChangeEvent;
            foreach (var editor in _trackedEditors.ToArray())
            {
                UnbindEditorContentStateChangeEvent(this, editor);
            }
        }

    }
}
