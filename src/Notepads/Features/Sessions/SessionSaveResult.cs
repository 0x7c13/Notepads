// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using Notepads.Features.Sessions.Contracts;

namespace Notepads.Features.Sessions;

internal sealed class SessionSaveResult
{
    public static SessionSaveResult Failed { get; } = new SessionSaveResult(false, false);

    public SessionSaveResult(bool succeeded, bool changed,
        IReadOnlyDictionary<Guid, TextEditorSessionDataV2> descriptors = null)
    {
        Succeeded = succeeded;
        Changed = changed;
        Descriptors = descriptors ?? new Dictionary<Guid, TextEditorSessionDataV2>();
    }

    public bool Succeeded { get; }
    public bool Changed { get; }
    public IReadOnlyDictionary<Guid, TextEditorSessionDataV2> Descriptors { get; }
}
