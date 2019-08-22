
namespace Notepads
{
    using Notepads.Services;
    using Notepads.Settings;
    using Notepads.Utilities;
    using System.Collections.Generic;
    using System.Linq;
    using Windows.ApplicationModel;
    using Windows.ApplicationModel.Activation;
    using Windows.Storage;

    public static class Program
    {
        public static bool IsFirstInstance { get; set; }

        private static IList<AppInstance> _instances;

        static void Main(string[] args)
        {
            _instances = AppInstance.GetInstances();

            if (_instances.Count == 0)
            {
                IsFirstInstance = true;
                ApplicationSettings.Write("ActiveInstance", null);
            }

            IActivatedEventArgs activatedArgs = AppInstance.GetActivatedEventArgs();

            if (activatedArgs is FileActivatedEventArgs fileArgs)
            {
                foreach (var file in fileArgs.Files)
                {
                    if (!(file is StorageFile)) continue;
                    AssignOrCreateInstanceForFile((file as StorageFile).Path);
                }
            }
            else if (activatedArgs is CommandLineActivatedEventArgs cmdActivatedArgs)
            {
                LoggingService.LogInfo($"[Main] [CommandActivated] CurrentDirectoryPath: {cmdActivatedArgs.Operation.CurrentDirectoryPath} Arguments: {cmdActivatedArgs.Operation.Arguments}");

                var file = FileSystemUtility.GetAbsolutePathFromCommandLine(
                    cmdActivatedArgs.Operation.CurrentDirectoryPath, cmdActivatedArgs.Operation.Arguments);
                if (file != null)
                {
                    AssignOrCreateInstanceForFile(file);
                }
                else
                {
                    OpenNewInstance();
                }
            }
            else if (activatedArgs is ProtocolActivatedEventArgs protocolActivatedEventArgs)
            {
                LoggingService.LogInfo($"[Main] [ProtocolActivated] Protocol: {protocolActivatedEventArgs.Uri}");
                var protocol = NotepadsProtocolService.GetOperationProtocol(protocolActivatedEventArgs.Uri, out var context);
                if (protocol == NotepadsOperationProtocol.OpenNewInstance || protocol == NotepadsOperationProtocol.Unrecognized)
                {
                    OpenNewInstance();
                }
            }
            else
            {
                OpenNewInstance();
            }
        }

        private static void OpenNewInstance()
        {
            AppInstance.FindOrRegisterInstanceForKey(App.Id.ToString());
            App.IsFirstInstance = IsFirstInstance;
            global::Windows.UI.Xaml.Application.Start(p => new App());
            IsFirstInstance = false;
        }

        private static void AssignOrCreateInstanceForFile(string filePath)
        {
            var instance = (GetLastActiveInstance() ?? AppInstance.FindOrRegisterInstanceForKey(App.Id.ToString()));

            if (instance.IsCurrentInstance)
            {
                App.IsFirstInstance = IsFirstInstance;
                global::Windows.UI.Xaml.Application.Start(p => new App());
                IsFirstInstance = false;
            }
            else
            {
                instance.RedirectActivationTo();
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

            string activeInstance = ApplicationSettings.Read("ActiveInstance") as string;

            if (activeInstance == null)
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

            // activeInstance might be closed, let's return the first instance
            return instances.FirstOrDefault();
        }
    }
}
