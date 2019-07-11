
namespace Notepads.Services
{
    using System;
    using System.Collections.Generic;
    using System.Linq;
    using System.Text;
    using System.Threading.Tasks;
    using Windows.ApplicationModel.Activation;
    using Windows.Storage;
    using Windows.UI.Xaml.Controls;
    using Notepads.Utilities;

    public static class ActivationService
    {
        public static async Task ActivateAsync(Frame rootFrame, IActivatedEventArgs e)
        {
            if (e is LaunchActivatedEventArgs launchActivatedEventArgs && launchActivatedEventArgs.PrelaunchActivated == false)
            {
                LaunchActivated(rootFrame, launchActivatedEventArgs);
            }
            else if (e is FileActivatedEventArgs fileActivatedEventArgs)
            {
                await FileActivated(rootFrame, fileActivatedEventArgs);
            }
            else
            {
                await CommandActivated(rootFrame, e);
            }
        }

        private static void LaunchActivated(Frame rootFrame, LaunchActivatedEventArgs launchActivatedEventArgs)
        {
            if (launchActivatedEventArgs.PrelaunchActivated == false)
            {
                // On Windows 10 version 1607 or later, this code signals that this app wants to participate in prelaunch
                Windows.ApplicationModel.Core.CoreApplication.EnablePrelaunch(true);
            }

            if (rootFrame.Content == null)
            {
                rootFrame.Navigate(typeof(MainPage), launchActivatedEventArgs.Arguments);
            }
        }

        private static async Task FileActivated(Frame rootFrame, FileActivatedEventArgs fileActivatedEventArgs)
        {
            if (rootFrame.Content == null)
            {
                rootFrame.Navigate(typeof(MainPage), fileActivatedEventArgs);
            }
            else if (rootFrame.Content is MainPage mainPage)
            {
                await mainPage.OpenFiles(fileActivatedEventArgs.Files);
            }
        }

        private static async Task CommandActivated(Frame rootFrame, IActivatedEventArgs e)
        {
            if (rootFrame.Content == null)
            {
                rootFrame.Navigate(typeof(MainPage), e);
            }
            else if (rootFrame.Content is MainPage mainPage)
            {
                if (e.Kind == ActivationKind.CommandLineLaunch)
                {
                    if (e is CommandLineActivatedEventArgs commandLine)
                    {
                        var file = await FileSystemUtility.OpenFileFromCommandLine(
                            commandLine.Operation.CurrentDirectoryPath, commandLine.Operation.Arguments);
                        if (file != null)
                        {
                            await mainPage.OpenFile(file);
                        }
                    }
                }
            }
        }
    }
}
