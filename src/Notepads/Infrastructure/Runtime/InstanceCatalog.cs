// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using Windows.ApplicationModel;

namespace Notepads.Infrastructure.Runtime;

internal static class InstanceCatalog
{
    public static IReadOnlyCollection<Guid> GetLiveInstanceIds()
    {
        var identities = new HashSet<Guid>();
        foreach (var instance in AppInstance.GetInstances())
            if (Guid.TryParse(instance.Key, out var identity)) identities.Add(identity);
        return identities;
    }
}
