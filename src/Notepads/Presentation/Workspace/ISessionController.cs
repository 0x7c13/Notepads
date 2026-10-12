// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Sessions.Contracts;
using Notepads.Presentation.Controls.TextEditor;
using Windows.Storage;

namespace Notepads.Presentation.Workspace;

internal interface ISessionController : IDisposable
{
    event EventHandler RecoveryStateChanged;

    event EventHandler RestorationBackupFailed;

    bool IsBackupEnabled { get; set; }

    int UnrecoveredEditorCount { get; }

    SessionRecoveryOutcome RecoveryOutcome { get; }

    /// <summary>
    /// Whether the latest authority read (startup, save, adoption, close or clear) found recovery blocked by files it cannot verify.
    /// </summary>
    bool IsRecoveryBlocked { get; }

    Task InitializeAuthorityAsync();

    /// <summary>Record startup restore intent before activation can open files.</summary>
    void RequestRestoration();

    Task<int> LoadLastSessionAsync();

    /// <summary>The current recovery lifetime, captured when a file operation starts.</summary>
    CancellationToken FileOperationToken { get; }

    /// <summary>Resolve matching recovery and return its current actual file association.</summary>
    Task<WorkspaceTab> ResolveFileAsync(StorageFile file, CancellationToken cancellationToken = default);

    /// <summary>Publish durable transfer ownership and install it before reset can retire the epoch.</summary>
    Task PublishTransferredEditorAsync(Func<Task> publishReceipt, Action install, CancellationToken cancellationToken);

    Task<ITextEditor> EnsureLoadedAsync(WorkspaceTab tab, bool allowWhileClosing = false);

    /// <summary>Pause automatic loading and visual attachment during a terminal close.</summary>
    void PauseRestoration();

    void ResumeRestoration();

    /// <summary>
    /// Queue one recovery maintenance pass (retention, then orphaned-asset collection) after the initial load,
    /// whatever the snapshot preference. Runs once per controller; failures are logged, never thrown.
    /// </summary>
    Task RunStartupMaintenanceAsync();

    Task<bool> SaveSessionAsync(Action actionAfterSaving = null, CancellationToken cancellationToken = default);

    void StartSessionBackup(bool startImmediately = false);

    void StopSessionBackup();

    /// <summary>Finish queued session operations without canceling demand restoration.</summary>
    Task WaitForSessionOperationsAsync();

    Task DrainAsync();

    Task ClearSessionDataAsync();

    Task<bool> PrepareExplicitCloseAsync(IReadOnlyCollection<Guid> editorIds, ICollection<Guid> closed = null);

    Task OpenSessionBackupFolderAsync();
}
