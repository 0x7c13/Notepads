// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Composition;
using Notepads.Features.Preferences;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Infrastructure.Settings;
using Notepads.Infrastructure.Storage;
using Notepads.Presentation.Workspace.Activation;
using Windows.ApplicationModel;
using Windows.ApplicationModel.Activation;

namespace Notepads;

public static class Program
{
    static void Main(string[] args)
    {
#if DEBUG
        Task.Run(() => LoggingService.InitializeAsync(new FileLogSink()));
#endif

        var bootstrap = new BootstrapContext();
        IActivatedEventArgs activatedArgs = AppInstance.GetActivatedEventArgs();

        if (activatedArgs is FileActivatedEventArgs)
        {
            RedirectOrCreateNewInstance(bootstrap);
        }
        else if (activatedArgs is CommandLineActivatedEventArgs)
        {
            RedirectOrCreateNewInstance(bootstrap);
        }
        else if (activatedArgs is ProtocolActivatedEventArgs protocolActivatedEventArgs)
        {
            LoggingService.LogInfo($"[{nameof(Main)}] [ProtocolActivated] Protocol: {protocolActivatedEventArgs.Uri}");
            var protocol = NotepadsProtocolService.GetOperationProtocol(protocolActivatedEventArgs.Uri, out _);
            if (protocol == NotepadsOperationProtocol.OpenNewInstance)
            {
                OpenNewInstance(bootstrap);
            }
            else
            {
                RedirectOrCreateNewInstance(bootstrap);
            }
        }
        else if (activatedArgs is LaunchActivatedEventArgs launchActivatedEventArgs)
        {
            bool handled = false;

            if (!string.IsNullOrEmpty(launchActivatedEventArgs.Arguments))
            {
                var protocol = NotepadsProtocolService.GetOperationProtocol(new Uri(launchActivatedEventArgs.Arguments), out _);
                if (protocol == NotepadsOperationProtocol.OpenNewInstance)
                {
                    handled = true;
                    OpenNewInstance(bootstrap);
                }
            }

            if (!handled)
            {
                RedirectOrCreateNewInstance(bootstrap);
            }
        }
        else
        {
            RedirectOrCreateNewInstance(bootstrap);
        }
    }

    private static void OpenNewInstance(BootstrapContext bootstrap)
    {
        AppInstance.FindOrRegisterInstanceForKey(bootstrap.InstanceId.ToString());
        StartApplication(bootstrap);
    }

    private static void RedirectOrCreateNewInstance(BootstrapContext bootstrap)
    {
        var instance = (GetLastActiveInstance() ?? AppInstance.FindOrRegisterInstanceForKey(bootstrap.InstanceId.ToString()));

        if (instance.IsCurrentInstance)
        {
            StartApplication(bootstrap);
        }
        else
        {
            // open new instance if user prefers to
            if (ApplicationSettingsStore.Read(SettingsKey.AlwaysOpenNewWindowBool) is bool alwaysOpenNewWindowBool && alwaysOpenNewWindowBool)
            {
                OpenNewInstance(bootstrap);
            }
            else
            {
                instance.RedirectActivationTo();
            }
        }
    }

    private static void StartApplication(BootstrapContext bootstrap)
    {
        Windows.UI.Xaml.Application.Start(_ =>
        {
            SynchronizationContext.SetSynchronizationContext(new Windows.System.DispatcherQueueSynchronizationContext(
                Windows.System.DispatcherQueue.GetForCurrentThread()));
            new App(bootstrap);
        });
    }

    private static AppInstance GetLastActiveInstance()
    {
        var instances = AppInstance.GetInstances();

        if (instances.Count == 0)
        {
            return null;
        }
        else if (instances.Count == 1)
        {
            return instances.FirstOrDefault();
        }

        if (ApplicationSettingsStore.Read(SettingsKey.ActiveInstanceIdStr) is not string activeInstance)
        {
            return null;
        }

        foreach (var appInstance in instances)
        {
            if (appInstance.Key == activeInstance)
            {
                return appInstance;
            }
        }

        // activeInstance might be closed already, let's return the first instance in this case
        return instances.FirstOrDefault();
    }
}
