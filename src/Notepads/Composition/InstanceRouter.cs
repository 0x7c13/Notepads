// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Linq;
using System.Threading;
using Notepads.Features.Preferences;
using Notepads.Infrastructure.Settings;
using Notepads.Presentation.Workspace.Activation;
using Windows.ApplicationModel;
using Windows.ApplicationModel.Activation;
using WinRT;

namespace Notepads.Composition;

// Process routing is independent of XAML construction. The selected process alone
// runs the application factory; background warmups never activate a user's window.
internal sealed class InstanceRouter(Action<BootstrapContext> startApplication, string protocolScheme = "notepads")
{
    internal void Activate()
    {
        var args = AppInstance.GetActivatedEventArgs();
        var windowMove = WindowMoveActivation.Read(args, protocolScheme);
        BootstrapContext bootstrap;
        AppInstance target;
        using (var gate = new Mutex(false, "Notepads.Activation." + Package.Current.Id.FamilyName))
        {
            try { gate.WaitOne(); }
            catch (AbandonedMutexException) { /* A terminated entrant no longer owns the gate. */ }
            try
            {
                var existing = GetLastActiveInstance();
                var newWindow = ApplicationSettingsStore.Read(SettingsKey.AlwaysOpenNewWindowBool) is true;
                var prelaunch = args is LaunchActivatedEventArgs { PrelaunchActivated: true };
                var discardPrelaunch = prelaunch && (existing != null || newWindow);
                bootstrap = new BootstrapContext(discardPrelaunch, windowMove, protocolScheme);
                if (windowMove != null)
                {
                    target = AppInstance.FindOrRegisterInstanceForKey(windowMove.TransferId.ToString());
                }
                else if (discardPrelaunch)
                {
                    // Acknowledge activation before exiting, otherwise Windows
                    // reports an activation failure. Do not register or redirect.
                    target = null;
                }
                else
                {
                    // Before XAML startup, Native AOT may expose activation as
                    // IInspectable rather than its concrete managed runtime class.
                    if (!prelaunch && (newWindow || args.Kind == ActivationKind.Protocol &&
                        NotepadsProtocolService.GetOperationProtocol(args.As<IProtocolActivatedEventArgs>().Uri) == NotepadsOperationProtocol.OpenNewInstance))
                        existing = null;
                    // Serialize discovery and registration, so concurrent activations
                    // see the first registered process before choosing another one.
                    target = existing ?? AppInstance.FindOrRegisterInstanceForKey(bootstrap.InstanceId.ToString());
                }
            }
            finally { gate.ReleaseMutex(); }
        }
        // Neither XAML startup nor synchronous redirection may retain the gate:
        // the target needs to dispatch incoming activations during initialization.
        if (target == null || target.IsCurrentInstance) startApplication(bootstrap);
        else target.RedirectActivationTo();
    }

    private static AppInstance GetLastActiveInstance()
    {
        var instances = AppInstance.GetInstances();
        if (instances.Count < 2) return instances.FirstOrDefault();
        // Missing or incorrectly typed last-active metadata must not open a new
        // window while reuse is enabled; it is only a routing hint.
        var active = ApplicationSettingsStore.Read(SettingsKey.ActiveInstanceIdStr) as string;
        return instances.FirstOrDefault(instance => instance.Key == active) ?? instances.FirstOrDefault();
    }
}
