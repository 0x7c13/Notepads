// ---------------------------------------------------------------------------------------------
// Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
// See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Sessions;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Recovery;
using Notepads.Features.Sessions.Validation;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Presentation.Controls.TextEditor;

namespace Notepads.Presentation.Workspace;

internal sealed partial class SessionController
{
    private sealed record CaptureEntry(WorkspaceTab Tab, ITextEditor Editor,
        RecoveryDocumentSource Source, long RepresentationVersion, long Revision, SessionDocumentCapture Document);

    private WorkspaceTab[] LiveTabs() => _notepadsCore.GetTabs().Where(tab => tab.State != WorkspaceTabState.Closed).ToArray();

    private async Task<bool> SaveSessionCoreAsync(Action actionAfterSaving, CancellationToken cancellation)
    {
        var capture = new SessionCapture();
        try
        {
            cancellation.ThrowIfCancellationRequested();
            if (!IsBackupEnabled || _disposed) return false;
            if (_recoveryAdmission is RecoveryAdmission.Pending or RecoveryAdmission.Archiving)
            {
                await DiscoverRecoveryPlanAsync(cancellation);
                if (_disposed) return false;
                if (_recoveryAdmission == RecoveryAdmission.Pending && _restoreRequested)
                {
                    AdmitDiscoveredWorkspace();
                    // Do not await payload loading under the capture/publication gate.
                    StartRestoreWorker();
                }
                else
                {
                    // Snapshots were deliberately enabled without requesting restoration.
                    // Protect the unclaimed old workspace before replacing its index.
                    // This synchronous decision is final even if publication fails:
                    // a later Load cannot restore a partly archived frozen plan.
                    _recoveryAdmission = RecoveryAdmission.Archiving;
                    if (BeforeRecoveryArchive != null) await BeforeRecoveryArchive(cancellation);
                    await Task.Run(() => _service.ArchiveRecoverySourcesAsync(_restorePlan.Sources, cancellation));
                    _recoveryAdmission = RecoveryAdmission.Archived;
                }
            }
            foreach (var editor in LiveTabs().Select(tab => tab.ReadyEditor).Where(editor => editor != null))
                await editor.RepairRecoveryJournalAsync(cancellation);
            cancellation.ThrowIfCancellationRequested();
            if (!IsBackupEnabled || _disposed) return false;

            var tabs = LiveTabs();
            capture.ExpectedStamp = _service.CurrentStamp;
            capture.SelectedEditorId = _notepadsCore.GetSelectedTab()?.Id;
            capture.TabScrollOffset = _notepadsCore.GetTabScrollViewerHorizontalOffset();
            capture.PendingRescues.AddRange(_imports.Select(tab => tab.RecoverySource)
                .Where(source => source?.IsOwnScopeRescue == true));
            var entries = new List<CaptureEntry>(tabs.Length);
            foreach (var tab in tabs)
            {
                var editor = tab.ReadyEditor;
                var source = tab.RecoverySource;
                var document = editor != null ? CaptureEditor(editor, source, tab.FileAccessBinding) :
                    SessionDocumentCapture.Retain(source ?? throw new InvalidOperationException("An unloaded tab has no recovery source."));
                capture.Documents.Add(document);
                entries.Add(new CaptureEntry(tab, editor, source, tab.RepresentationVersion,
                    _editorRevisions.GetOrAdd(tab.Id, 0), document));
            }
            var result = await Task.Run(() => _service.SaveAsync(capture, cancellation));
            if (!result.Succeeded) return false;
            _maintenanceDue |= result.Changed;
            foreach (var entry in entries)
            {
                if (entry.Editor != null && result.Descriptors.ContainsKey(entry.Tab.Id) && _notepadsCore.ContainsTab(entry.Tab))
                    entry.Tab.AcknowledgeNativeCapture(entry.Source, entry.RepresentationVersion, entry.Editor);
            }
            TryReleaseRestorePlan();

            if (actionAfterSaving != null)
            {
                var currentTabs = LiveTabs();
                if (_disposed || !currentTabs.SequenceEqual(tabs) ||
                    _notepadsCore.GetSelectedTab()?.Id != capture.SelectedEditorId ||
                    entries.Any(entry => entry.Tab.RepresentationVersion != entry.RepresentationVersion ||
                        !ReferenceEquals(entry.Tab.ReadyEditor, entry.Editor) ||
                        _editorRevisions.GetOrAdd(entry.Tab.Id, 0) != entry.Revision)) return false;
                foreach (var entry in entries.Where(entry => entry.Editor != null))
                {
                    using var current = entry.Editor.CaptureRecoveryState();
                    var recorded = entry.Document.Recovery;
                    if (current.Checkpoint.CommittedSequence != recorded.Checkpoint.CommittedSequence ||
                        current.DocumentJournal.GenerationId != recorded.DocumentJournal.GenerationId ||
                        current.RecoveryBaseline.GenerationId != recorded.RecoveryBaseline.GenerationId) return false;
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
            foreach (var document in capture.Documents.Where(document => document.Recovery != null))
            {
                try { await document.Recovery.DisposeAsync(); }
                catch (Exception ex) { LoggingService.LogError($"[{nameof(SessionController)}] Failed to release captured recovery state: {ex}"); }
            }
        }
    }

    private static SessionDocumentCapture CaptureEditor(ITextEditor editor, RecoveryDocumentSource source,
        RecoveryFileAccessBinding fileAccessBinding) => new()
    {
        Id = editor.Id,
        EditingFile = editor.EditingFile,
        EditingFileName = editor.EditingFileName,
        EditingFilePath = editor.EditingFilePath,
        Recovery = editor.CaptureRecoveryState(),
        Source = source,
        FileAccessBinding = fileAccessBinding
    };

    public async Task ClearSessionDataAsync()
    {
        var pausedBefore = _restorationPaused;
        try
        {
            if (_restorationPaused || _notepadsCore.IsClosing)
                throw new InvalidOperationException("Recovery settings cannot change while the workspace is closing.");
            if (_restoreRequested) await LoadLastSessionAsync();
            // Resolve outside session serialization: the worker's completion must
            // itself enter that coordinator. Failure cannot erase a pending source.
            foreach (var tab in LiveTabs())
                if (await EnsureLoadedAsync(tab) == null) throw new InvalidOperationException("Recovery cannot be disabled while a document cannot be loaded.");
            PauseRestoration();
            _restoreLifetime.Cancel();
            SignalRestoration();
            await _restoreWorker;
            ThrowIfRestoreDrainFailed();
            await _restoreCheckpointWork;
            await _sessionOperations.RunAsync(async cancellation =>
            {
                if (_disposed || _notepadsCore.IsClosing)
                    throw new InvalidOperationException("The workspace started closing before recovery settings could change.");
                if (LiveTabs().Any(tab => tab.ReadyEditor == null))
                    throw new InvalidOperationException("A document appeared during reset and must be loaded before recovery can be disabled.");
                var unadmitted = _recoveryAdmission is RecoveryAdmission.Pending or RecoveryAdmission.Archiving;
                if (unadmitted) await DiscoverRecoveryPlanAsync(cancellation);
                var sources = unadmitted ? _restorePlan.Sources :
                    (IEnumerable<RecoveryDocumentSource>)_imports.Select(tab => tab.RecoverySource).Where(source => source != null).ToArray();
                var before = _service.CurrentStamp;
                try { await Task.Run(() => _service.ClearSessionDataAsync(sources, cancellation)); }
                finally
                {
                    // The reset intent may have committed even if completing its
                    // mirrored decision failed. Never reuse the obsolete epoch.
                    if (!SessionRecoveryAuthority.SameEpoch(before, _service.CurrentStamp))
                    {
                        foreach (var tab in LiveTabs()) tab.RetireRecoverySourceAfterReset();
                        foreach (var tab in _imports) tab.MarkClosed();
                        _imports.Clear();
                        _restorePlan?.Dispose();
                        _restorePlan = null;
                        _recoveryAdmission = RecoveryAdmission.Archived;
                        _restoreRequested = false;
                    }
                }
            });
        }
        finally
        {
            if (!pausedBefore && !_disposed && !_notepadsCore.IsClosing) ResumeRestoration();
        }
    }
}
