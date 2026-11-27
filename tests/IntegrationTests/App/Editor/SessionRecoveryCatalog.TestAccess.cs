// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Sessions.Storage;
using Notepads.Features.Sessions.Validation;

namespace Notepads.Features.Sessions.Recovery;

// Compiled only into the source-linked app test host, so faults can target the owning catalog read.
internal static partial class SessionRecoveryCatalog
{
    internal static Task<RecoveryScopeSnapshot> TestReadControlAsync(RecoveryRecordStore store, Guid scopeId,
        CancellationToken cancellation) => ReadControlAsync(store, scopeId, cancellation);
}
