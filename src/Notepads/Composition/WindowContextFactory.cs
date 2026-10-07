// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using Notepads.Features.Sessions;
using Notepads.Features.Sessions.Contracts;
using Notepads.Infrastructure.Runtime;
using Notepads.Presentation.Workspace;
using Windows.UI.Core;

namespace Notepads.Composition;

internal static class WindowContextFactory
{
    public static WindowContext Create(BootstrapContext bootstrap, SessionRegistry registry, CoreDispatcher dispatcher)
    {
        var scope = new SessionScopeData
        {
            OwnerId = bootstrap.IsPrimaryInstance ? SessionScopeData.GetStableOwner() : bootstrap.InstanceId,
            InstanceId = bootstrap.InstanceId,
            Kind = bootstrap.IsPrimaryInstance ? SessionScopeData.Primary : SessionScopeData.Secondary
        };
        var prefix = bootstrap.IsPrimaryInstance ? "" : SessionScopeData.GetSecondaryPrefix(scope.OwnerId);
        var service = new SessionService(scope, prefix + "NotepadsSessionData.json", InstanceCatalog.GetLiveInstanceIds);
        return new WindowContext(BootstrapContext.ApplicationName, bootstrap.InstanceId, bootstrap.IsPrimaryInstance,
            scope.OwnerId, dispatcher, service, registry.Register, bootstrap.ReleaseInstanceOwnership);
    }
}
