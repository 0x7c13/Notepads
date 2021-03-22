namespace Notepads
{
    using System;
    using System.Linq;
    using System.Threading.Tasks;
    using Notepads.Services;
    using Notepads.Settings;
    using Windows.ApplicationModel;
    using Windows.ApplicationModel.Activation;

    public static class Program
    {
        static void Main(string[] args)
        {
#if DEBUG
            Task.Run(LoggingService.InitializeFileSystemLoggingAsync);
#endif

            switch (AppInstance.GetActivatedEventArgs())
            {
                case FileActivatedEventArgs _:
                case CommandLineActivatedEventArgs _:
                    RedirectOrCreateNewInstance();
                    break;
                case ProtocolActivatedEventArgs protocolActivatedEventArgs:
                    LoggingService.LogInfo($"[{nameof(Main)}] [ProtocolActivated] Protocol: {protocolActivatedEventArgs.Uri}");
                    var protocol = NotepadsProtocolService.GetOperationProtocol(protocolActivatedEventArgs.Uri, out _);
                    if (protocol == NotepadsOperationProtocol.OpenNewInstance)
                    {
                        OpenNewInstance();
                    }
                    else
                    {
                        RedirectOrCreateNewInstance();
                    }
                    break;
                case LaunchActivatedEventArgs launchActivatedEventArgs:
                    bool handled = false;
                    if (!string.IsNullOrEmpty(launchActivatedEventArgs.Arguments))
                    {
                        protocol = NotepadsProtocolService.GetOperationProtocol(new Uri(launchActivatedEventArgs.Arguments), out _);
                        if (protocol == NotepadsOperationProtocol.OpenNewInstance)
                        {
                            handled = true;
                            OpenNewInstance();
                        }
                    }

                    if (!handled)
                    {
                        RedirectOrCreateNewInstance();
                    }
                    break;
                //case null:
                //    // No activated event args, so this is not an activation via the multi-instance ID
                //    // Just create a new instance and let App OnActivated resolve the launch
                //    App.IsGameBarWidget = true;
                //    App.IsPrimaryInstance = true;
                //    Windows.UI.Xaml.Application.Start(p => new App());
                //    break;
                default:
                    RedirectOrCreateNewInstance();
                    break;
            }
        }

        private static void OpenNewInstance()
        {
            AppInstance.FindOrRegisterInstanceForKey(App.Id.ToString());
            Windows.UI.Xaml.Application.Start(p => new App());
        }

        private static void RedirectOrCreateNewInstance()
        {
            var instance = (GetLastActiveInstance() ?? AppInstance.FindOrRegisterInstanceForKey(App.Id.ToString()));

            if (instance.IsCurrentInstance)
            {
                Windows.UI.Xaml.Application.Start(p => new App());
            }
            else
            {
                // open new instance if user prefers to
                if (ApplicationSettingsStore.Read(SettingsKey.AlwaysOpenNewWindowBool) is bool alwaysOpenNewWindowBool && alwaysOpenNewWindowBool)
                {
                    OpenNewInstance();
                }
                else
                {
                    instance.RedirectActivationTo();
                }
            }
        }

        private static AppInstance GetLastActiveInstance()
        {
            var instances = AppInstance.GetInstances();

            switch (instances.Count)
            {
                case 0:
                    return null;
                case 1:
                    return instances.FirstOrDefault();
            }

            if (!(ApplicationSettingsStore.Read(SettingsKey.ActiveInstanceIdStr) is string activeInstance))
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
}