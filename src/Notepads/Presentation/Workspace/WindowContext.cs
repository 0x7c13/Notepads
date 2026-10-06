// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using Notepads.Features.Sessions;
using Notepads.Features.Sessions.Contracts;
using Windows.UI.Core;

namespace Notepads.Presentation.Workspace;

public sealed class WindowContext
{
    internal WindowContext(string applicationName, Guid instanceId, bool isPrimaryInstance, Guid documentOwnerId,
        CoreDispatcher dispatcher, SessionService sessions,
        Func<ISessionPersistenceParticipant, IDisposable> registerSession, Action releaseInstanceOwnership)
    {
        ApplicationName = applicationName;
        InstanceId = instanceId;
        IsPrimaryInstance = isPrimaryInstance;
        DocumentOwnerId = documentOwnerId;
        Dispatcher = dispatcher;
        Sessions = sessions;
        RegisterSession = registerSession;
        ReleaseInstanceOwnership = releaseInstanceOwnership;
    }

    public string ApplicationName { get; }
    public Guid InstanceId { get; }
    public bool IsPrimaryInstance { get; }
    public Guid DocumentOwnerId { get; }
    public CoreDispatcher Dispatcher { get; }
    internal SessionService Sessions { get; }
    internal Func<ISessionPersistenceParticipant, IDisposable> RegisterSession { get; }
    internal Action ReleaseInstanceOwnership { get; }
}

internal sealed class WorkspaceNavigation
{
    public WorkspaceNavigation(WindowContext context, object activation)
    {
        Context = context;
        Activation = activation;
    }
    public WindowContext Context { get; }
    public object Activation { get; }
}
