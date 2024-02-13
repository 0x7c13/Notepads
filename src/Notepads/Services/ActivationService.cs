// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2024, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Services
{
    using System.Threading.Tasks;
    using Notepads.Utilities;
    using Notepads.Views.MainPage;
    using Windows.ApplicationModel.Activation;
    using Windows.Storage;
    using Windows.UI.Xaml.Controls;

    public static class ActivationService
    {
        public static async Task ActivateAsync(Frame rootFrame, IActivatedEventArgs e)
        {
            switch (e)
            {
                case ProtocolActivatedEventArgs protocolActivatedEventArgs:
                    ProtocolActivated(rootFrame, protocolActivatedEventArgs);
                    break;
                case FileActivatedEventArgs fileActivatedEventArgs:
                    await FileActivatedAsync(rootFrame, fileActivatedEventArgs);
                    break;
                case CommandLineActivatedEventArgs commandLineActivatedEventArgs:
                    await CommandActivatedAsync(rootFrame, commandLineActivatedEventArgs);
                    break;
                case LaunchActivatedEventArgs launchActivatedEventArgs:
                    LaunchActivated(rootFrame, launchActivatedEventArgs);
                    break;
                // For other types of activated events
                default:
                    {
                        if (rootFrame.Content == null) rootFrame.Navigate(typeof(NotepadsMainPage));
                        break;
                    }
            }
        }

        private static void ProtocolActivated(Frame rootFrame, ProtocolActivatedEventArgs protocolActivatedEventArgs)
        {
            LoggingService.LogInfo($"[{nameof(ActivationService)}] [ProtocolActivated] Protocol: {protocolActivatedEventArgs.Uri}");

            switch (rootFrame.Content)
            {
                case null:
                    rootFrame.Navigate(typeof(NotepadsMainPage), protocolActivatedEventArgs);
                    break;
                case NotepadsMainPage mainPage:
                    mainPage.ExecuteProtocol(protocolActivatedEventArgs.Uri);
                    break;
            }
        }

        private static void LaunchActivated(Frame rootFrame, LaunchActivatedEventArgs launchActivatedEventArgs)
        {
            LoggingService.LogInfo($"[{nameof(ActivationService)}] [LaunchActivated] Kind: {launchActivatedEventArgs.Kind}");

            if (rootFrame.Content == null)
            {
                rootFrame.Navigate(typeof(NotepadsMainPage), launchActivatedEventArgs.Arguments);
            }
        }

        private static async Task FileActivatedAsync(Frame rootFrame, FileActivatedEventArgs fileActivatedEventArgs)
        {
            LoggingService.LogInfo($"[{nameof(ActivationService)}] [FileActivated]");

            switch (rootFrame.Content)
            {
                case null:
                    rootFrame.Navigate(typeof(NotepadsMainPage), fileActivatedEventArgs);
                    break;
                case NotepadsMainPage mainPage:
                    await mainPage.OpenFilesAsync(fileActivatedEventArgs.Files);
                    break;
            }
        }

        private static async Task CommandActivatedAsync(Frame rootFrame, CommandLineActivatedEventArgs commandLineActivatedEventArgs)
        {
            LoggingService.LogInfo($"[{nameof(ActivationService)}] [CommandActivated] CurrentDirectoryPath: {commandLineActivatedEventArgs.Operation.CurrentDirectoryPath} " +
                                   $"Arguments: {commandLineActivatedEventArgs.Operation.Arguments}");

            switch (rootFrame.Content)
            {
                case null:
                    rootFrame.Navigate(typeof(NotepadsMainPage), commandLineActivatedEventArgs);
                    break;
                case NotepadsMainPage mainPage:
                    {
                        StorageFile file = await FileSystemUtility.OpenFileFromCommandLineAsync(
                            commandLineActivatedEventArgs.Operation.CurrentDirectoryPath,
                            commandLineActivatedEventArgs.Operation.Arguments);

                        if (file != null)
                        {
                            await mainPage.OpenFileAsync(file);
                        }

                        break;
                    }
            }
        }
    }
}