namespace Notepads
{
    using System;
    using System.Linq;
    using Notepads.Services;
    using Notepads.Settings;
    using Windows.ApplicationModel;
    using Windows.ApplicationModel.Activation;

    public static class Program
    {
        public static bool IsFirstInstance { get; set; }

        static void Main(string[] args)
        {
            IActivatedEventArgs activatedArgs = AppInstance.GetActivatedEventArgs();

            if (activatedArgs == null)
            {
                // No activated event args, so this is not an activation via the multi-instance ID
                // Just create a new instance and let App OnActivated resolve the launch
                App.IsGameBarWidget = true;
                App.IsFirstInstance = true;
                Windows.UI.Xaml.Application.Start(p => new App());
            }

            var instances = AppInstance.GetInstances();

            if (instances.Count == 0)
            {
                IsFirstInstance = true;
                ApplicationSettingsStore.Write(SettingsKey.ActiveInstanceIdStr, null);
            }

            if (activatedArgs is FileActivatedEventArgs)
            {
                RedirectOrCreateNewInstance();
            }
            else if (activatedArgs is CommandLineActivatedEventArgs)
            {
                RedirectOrCreateNewInstance();
            }
            else if (activatedArgs is ProtocolActivatedEventArgs protocolActivatedEventArgs)
            {
                LoggingService.LogInfo($"[Main] [ProtocolActivated] Protocol: {protocolActivatedEventArgs.Uri}");
                var protocol = NotepadsProtocolService.GetOperationProtocol(protocolActivatedEventArgs.Uri, out _);
                if (protocol == NotepadsOperationProtocol.OpenNewInstance)
                {
                    OpenNewInstance();
                }
                else
                {
                    RedirectOrCreateNewInstance();
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
                        OpenNewInstance();
                    }
                }

                if (!handled)
                {
                    RedirectOrCreateNewInstance();
                }
            }
            else
            {
                RedirectOrCreateNewInstance();
            }
        }

        private static void OpenNewInstance()
        {
            AppInstance.FindOrRegisterInstanceForKey(App.Id.ToString());
            App.IsFirstInstance = IsFirstInstance;
            Windows.UI.Xaml.Application.Start(p => new App());
            IsFirstInstance = false;
        }

        private static void RedirectOrCreateNewInstance()
        {
            var instance = (GetLastActiveInstance() ?? AppInstance.FindOrRegisterInstanceForKey(App.Id.ToString()));

            if (instance.IsCurrentInstance)
            {
                App.IsFirstInstance = IsFirstInstance;
                Windows.UI.Xaml.Application.Start(p => new App());
                IsFirstInstance = false;
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

            if (instances.Count == 0)
            {
                return null;
            }
            else if (instances.Count == 1)
            {
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