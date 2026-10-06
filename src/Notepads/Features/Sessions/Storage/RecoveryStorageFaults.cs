// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using Notepads.Features.Sessions.Contracts;

namespace Notepads.Features.Sessions.Storage;

internal enum RecoveryStorageStep
{
    BeforeRead,
    BeforeTemporaryWrite,
    AfterTemporaryFlush,
    BeforeRename,
    AfterRename,
    BeforeDelete
}

/// <summary>Optional deterministic boundary faults; production uses no injector.</summary>
internal interface IRecoveryStorageFaults
{
    void OnStep(RecoveryStorageStep step, RecoveryAddress address, string copy);
}
