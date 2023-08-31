namespace Notepads.Services
{
    using System.Threading.Tasks;
    using Notepads.Utilities;
    using Notepads.Views.MainPage;
    using Windows.ApplicationModel.Activation;
    using Windows.UI.Xaml.Controls;

    public static class ActivationService
    {
        public static async Task ActivateAsync(Frame rootFrame, IActivatedEventArgs e)
        {
            if (e is ProtocolActivatedEventArgs protocolActivatedEventArgs)
            {
                ProtocolActivated(rootFrame, protocolActivatedEventArgs);
            }
            else if (e is FileActivatedEventArgs fileActivatedEventArgs)
            {
                await FileActivatedAsync(rootFrame, fileActivatedEventArgs);
            }
            else if (e is CommandLineActivatedEventArgs commandLineActivatedEventArgs)
            {
                await CommandActivatedAsync(rootFrame, commandLineActivatedEventArgs);
            }
            else if (e is LaunchActivatedEventArgs launchActivatedEventArgs)
            {
                LaunchActivated(rootFrame, launchActivatedEventArgs);
            }
            else // For other types of activated events
            {
                if (rootFrame.Content == null)
                {
                    rootFrame.Navigate(typeof(NotepadsMainPage));
                }
            }
        }

        private static void ProtocolActivated(Frame rootFrame, ProtocolActivatedEventArgs protocolActivatedEventArgs)
        {
            LoggingService.LogInfo($"[{nameof(ActivationService)}] [ProtocolActivated] Protocol: {protocolActivatedEventArgs.Uri}");

            if (rootFrame.Content == null)
            {
                rootFrame.Navigate(typeof(NotepadsMainPage), protocolActivatedEventArgs);
            }
            else if (rootFrame.Content is NotepadsMainPage mainPage)
            {
                mainPage.ExecuteProtocol(protocolActivatedEventArgs.Uri);
            }
        }

        private static void LaunchActivated(Frame rootFrame, LaunchActivatedEventArgs launchActivatedEventArgs)
        {
            LoggingService.LogInfo($"[{nameof(ActivationService)}] [LaunchActivated] Kind: {launchActivatedEventArgs.Kind}");

            if (launchActivatedEventArgs.PrelaunchActivated == false)
            {
                // On Windows 10 version 1607 or later, this code signals that this app wants to participate in prelaunch
                Windows.ApplicationModel.Core.CoreApplication.EnablePrelaunch(true);
            }

            if (rootFrame.Content == null)
            {
                rootFrame.Navigate(typeof(NotepadsMainPage), launchActivatedEventArgs.Arguments);
            }
        }

        private static async Task FileActivatedAsync(Frame rootFrame, FileActivatedEventArgs fileActivatedEventArgs)
        {
            LoggingService.LogInfo($"[{nameof(ActivationService)}] [FileActivated]");

            if (rootFrame.Content == null)
            {
                rootFrame.Navigate(typeof(NotepadsMainPage), fileActivatedEventArgs);
            }
            else if (rootFrame.Content is NotepadsMainPage mainPage)
            {
                await mainPage.OpenFilesAsync(fileActivatedEventArgs.Files);
            }
        }

        private static async Task CommandActivatedAsync(Frame rootFrame, CommandLineActivatedEventArgs commandLineActivatedEventArgs)
        {
            LoggingService.LogInfo($"[{nameof(ActivationService)}] [CommandActivated] CurrentDirectoryPath: {commandLineActivatedEventArgs.Operation.CurrentDirectoryPath} " +
                                   $"Arguments: {commandLineActivatedEventArgs.Operation.Arguments}");

            if (rootFrame.Content == null)
            {
                rootFrame.Navigate(typeof(NotepadsMainPage), commandLineActivatedEventArgs);
            }
            else if (rootFrame.Content is NotepadsMainPage mainPage)
            {
                var file = await FileSystemUtility.OpenFileFromCommandLineAsync(
                    commandLineActivatedEventArgs.Operation.CurrentDirectoryPath,
                    commandLineActivatedEventArgs.Operation.Arguments);

                if (file != null)
                {
                    await mainPage.OpenFileAsync(file);
                }
            }
        }
    }
}