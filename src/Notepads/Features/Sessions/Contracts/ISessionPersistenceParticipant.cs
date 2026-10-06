// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System.Threading;
using System.Threading.Tasks;

namespace Notepads.Features.Sessions.Contracts;

internal interface ISessionPersistenceParticipant
{
    bool IsBackupEnabled { get; }
    Task<bool> SaveForSuspensionAsync(CancellationToken cancellation);
    Task DrainAsync();
}
