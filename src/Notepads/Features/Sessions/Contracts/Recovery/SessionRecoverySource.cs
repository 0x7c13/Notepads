// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using Notepads.Features.Sessions.Contracts;

namespace Notepads.Features.Sessions.Contracts.Recovery;

/// <summary>An immutable recovery root retained until the importing session durably adopts its records.</summary>
internal sealed class SessionRecoverySource : IDisposable
{
    public NotepadsSessionDataV2 Session { get; set; }
    public RecoveryStamp ExpectedStamp { get; set; }
    public RecoveryAddress RootAddress { get; set; }
    public string RootSha256 { get; set; }
    public HashSet<Guid> RescueEditorIds { get; } = [];
    public Dictionary<Guid, IReadOnlyList<TextEditorSessionDataV2>> EarlierDescriptors { get; } = [];
    public Dictionary<Guid, IReadOnlyList<TextEditorSessionDataV2>> UnselectedNewerDescriptors { get; } = [];
    public Dictionary<Guid, long> ConsumptionCutoffs { get; } = [];
    public long CaptureRevisionFloor { get; set; }
    public SessionRecoveryOutcome Outcome { get; set; }
    public IDisposable WriterLease { get; set; }
    public IDisposable ReaderPin { get; set; }
    public void Dispose()
    {
        try { ReaderPin?.Dispose(); }
        finally { WriterLease?.Dispose(); ReaderPin = null; WriterLease = null; }
    }
}
