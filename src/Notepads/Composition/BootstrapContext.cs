// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Threading;

namespace Notepads.Composition;

internal sealed class BootstrapContext
{
    public const string ApplicationName = "Notepads";
    public Guid InstanceId { get; } = Guid.NewGuid();
    private Mutex _instanceOwnership;
    public bool IsPrimaryInstance { get; private set; }

    public void AcquireInstanceOwnership()
    {
        var ownership = new Mutex(true, ApplicationName, out var isNew);
        IsPrimaryInstance = isNew;
        if (isNew) _instanceOwnership = ownership;
        else ownership.Dispose();
    }

    public void ReleaseInstanceOwnership() => Interlocked.Exchange(ref _instanceOwnership, null)?.Dispose();
}
