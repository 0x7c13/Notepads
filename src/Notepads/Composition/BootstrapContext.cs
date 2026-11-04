// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Threading;
using Notepads.Features.Sessions.Contracts;
using Windows.ApplicationModel;

namespace Notepads.Composition;

internal sealed class BootstrapContext(bool discardPrelaunch = false, TransferToken windowMove = null, string protocolScheme = "notepads")
{
    public const string ApplicationName = "Notepads";
    public Guid InstanceId { get; } = windowMove?.TransferId ?? Guid.NewGuid();
    internal TransferToken WindowMove { get; } = windowMove?.Clone();
    internal string ProtocolScheme { get; } = protocolScheme;
    public bool DiscardPrelaunch { get; } = discardPrelaunch;
    private Mutex _instanceOwnership;
    public bool IsPrimaryInstance { get; private set; }

    public void AcquireInstanceOwnership()
    {
        // A dedicated receiver is secondary even if the source exits during launch.
        if (WindowMove != null) return;
        var ownership = new Mutex(true, ApplicationName + "." + Package.Current.Id.FamilyName, out var isNew);
        IsPrimaryInstance = isNew;
        if (isNew) _instanceOwnership = ownership;
        else ownership.Dispose();
    }

    public void ReleaseInstanceOwnership() => Interlocked.Exchange(ref _instanceOwnership, null)?.Dispose();
}
