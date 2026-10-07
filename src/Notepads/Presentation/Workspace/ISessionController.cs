// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Sessions.Contracts;

namespace Notepads.Presentation.Workspace;

internal interface ISessionController : IDisposable
{
    bool IsBackupEnabled { get; set; }

    int UnrecoveredEditorCount { get; }

    SessionRecoveryOutcome RecoveryOutcome { get; }

    /// <summary>
    /// Whether the latest authority read (startup, save, adoption, close or clear) found recovery blocked by files it cannot verify.
    /// </summary>
    bool IsRecoveryBlocked { get; }

    Task InitializeAuthorityAsync();

    Task<int> LoadLastSessionAsync();

    /// <summary>
    /// Queue one recovery maintenance pass (retention, then orphaned-asset collection) after the initial load,
    /// whatever the snapshot preference. Runs once per controller; failures are logged, never thrown.
    /// </summary>
    Task RunStartupMaintenanceAsync();

    Task<bool> SaveSessionAsync(Action actionAfterSaving = null, CancellationToken cancellationToken = default);

    void StartSessionBackup(bool startImmediately = false);

    void StopSessionBackup();

    Task DrainAsync();

    Task ClearSessionDataAsync();

    Task<bool> PrepareExplicitCloseAsync(IReadOnlyCollection<Guid> editorIds, ICollection<Guid> closed = null);

    Task OpenSessionBackupFolderAsync();
}
