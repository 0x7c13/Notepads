// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Features.Sessions;

internal sealed class SessionSaveResult
{
    public static SessionSaveResult Failed { get; } = new SessionSaveResult(false, false);

    public SessionSaveResult(bool succeeded, bool changed)
    {
        Succeeded = succeeded;
        Changed = changed;
    }

    public bool Succeeded { get; }
    public bool Changed { get; }
}
