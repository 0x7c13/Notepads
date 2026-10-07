// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System.Threading.Tasks;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Presentation.Views.MainPage;
using Notepads.Presentation.Workspace;
using Windows.ApplicationModel.Activation;
using Windows.Storage;
using Windows.UI.Xaml.Controls;

namespace Notepads.Presentation.Workspace.Activation;

public static class ActivationService
{
    public static async Task ActivateAsync(Frame rootFrame, IActivatedEventArgs e, WindowContext context)
    {
        switch (e)
        {
            case ProtocolActivatedEventArgs protocolActivatedEventArgs:
                ProtocolActivated(rootFrame, protocolActivatedEventArgs, context);
                break;
            case FileActivatedEventArgs fileActivatedEventArgs:
                await FileActivatedAsync(rootFrame, fileActivatedEventArgs, context);
                break;
            case CommandLineActivatedEventArgs commandLineActivatedEventArgs:
                await CommandActivatedAsync(rootFrame, commandLineActivatedEventArgs, context);
                break;
            case LaunchActivatedEventArgs launchActivatedEventArgs:
                LaunchActivated(rootFrame, launchActivatedEventArgs, context);
                break;
            // For other types of activated events
            default:
                {
                    if (rootFrame.Content == null) rootFrame.Navigate(typeof(NotepadsMainPage), new WorkspaceNavigation(context, null));
                    break;
                }
        }
    }

    private static void ProtocolActivated(Frame rootFrame, ProtocolActivatedEventArgs protocolActivatedEventArgs, WindowContext context)
    {
        LoggingService.LogInfo($"[{nameof(ActivationService)}] [ProtocolActivated] Protocol: {protocolActivatedEventArgs.Uri}");

        // A notepads:// URI carries no workspace payload; it only selects the instance in Program.Main.
        if (rootFrame.Content == null)
        {
            rootFrame.Navigate(typeof(NotepadsMainPage), new WorkspaceNavigation(context, null));
        }
    }

    private static void LaunchActivated(Frame rootFrame, LaunchActivatedEventArgs launchActivatedEventArgs, WindowContext context)
    {
        LoggingService.LogInfo($"[{nameof(ActivationService)}] [LaunchActivated] Kind: {launchActivatedEventArgs.Kind}");

        if (rootFrame.Content == null)
        {
            rootFrame.Navigate(typeof(NotepadsMainPage), new WorkspaceNavigation(context, launchActivatedEventArgs.Arguments));
        }
    }

    private static async Task FileActivatedAsync(Frame rootFrame, FileActivatedEventArgs fileActivatedEventArgs, WindowContext context)
    {
        LoggingService.LogInfo($"[{nameof(ActivationService)}] [FileActivated]");

        switch (rootFrame.Content)
        {
            case null:
                rootFrame.Navigate(typeof(NotepadsMainPage), new WorkspaceNavigation(context, fileActivatedEventArgs));
                break;
            case NotepadsMainPage mainPage:
                await mainPage.OpenFilesAsync(fileActivatedEventArgs.Files);
                break;
        }
    }

    private static async Task CommandActivatedAsync(Frame rootFrame, CommandLineActivatedEventArgs commandLineActivatedEventArgs, WindowContext context)
    {
        LoggingService.LogInfo($"[{nameof(ActivationService)}] [CommandActivated] CurrentDirectoryPath: {commandLineActivatedEventArgs.Operation.CurrentDirectoryPath} " +
                               $"Arguments: {commandLineActivatedEventArgs.Operation.Arguments}");

        switch (rootFrame.Content)
        {
            case null:
                rootFrame.Navigate(typeof(NotepadsMainPage), new WorkspaceNavigation(context, commandLineActivatedEventArgs));
                break;
            case NotepadsMainPage mainPage:
                {
                    StorageFile file = await CommandLineFiles.OpenFileFromCommandLineAsync(
                        commandLineActivatedEventArgs.Operation.CurrentDirectoryPath,
                        commandLineActivatedEventArgs.Operation.Arguments, context.ApplicationName);

                    if (file != null)
                    {
                        await mainPage.OpenFileAsync(file);
                    }

                    break;
                }
        }
    }
}
