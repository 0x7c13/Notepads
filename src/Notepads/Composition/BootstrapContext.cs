// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Threading;
using Windows.ApplicationModel;

namespace Notepads.Composition;

internal sealed class BootstrapContext(bool discardPrelaunch = false)
{
    public const string ApplicationName = "Notepads";
    public Guid InstanceId { get; } = Guid.NewGuid();
    public bool DiscardPrelaunch { get; } = discardPrelaunch;
    private Mutex _instanceOwnership;
    public bool IsPrimaryInstance { get; private set; }

    public void AcquireInstanceOwnership()
    {
        var ownership = new Mutex(true, ApplicationName + "." + Package.Current.Id.FamilyName, out var isNew);
        IsPrimaryInstance = isNew;
        if (isNew) _instanceOwnership = ownership;
        else ownership.Dispose();
    }

    public void ReleaseInstanceOwnership() => Interlocked.Exchange(ref _instanceOwnership, null)?.Dispose();
}
