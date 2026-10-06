// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Sessions.Contracts;

namespace Notepads.Presentation.Workspace;

internal interface ISessionController : IDisposable
{
    bool IsBackupEnabled { get; set; }

    int UnrecoveredEditorCount { get; }

    SessionRecoveryOutcome RecoveryOutcome { get; }

    Task InitializeAuthorityAsync();

    Task<int> LoadLastSessionAsync();

    Task<bool> SaveSessionAsync(Action actionAfterSaving = null, CancellationToken cancellationToken = default);

    void StartSessionBackup(bool startImmediately = false);

    void StopSessionBackup();

    Task DrainAsync();

    Task ClearSessionDataAsync();

    Task<bool> PrepareExplicitCloseAsync(Guid editorId);

    Task<int> RecoverBackupFilesAsync();

    Task OpenSessionBackupFolderAsync();
}
