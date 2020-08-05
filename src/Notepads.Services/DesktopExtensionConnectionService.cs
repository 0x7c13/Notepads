﻿namespace Notepads.Services
{
    using System;
    using System.Collections.Generic;
    using System.Threading.Tasks;
    using Windows.ApplicationModel;
    using Windows.ApplicationModel.AppService;
    using Windows.ApplicationModel.Background;
    using Windows.Foundation.Collections;

    public enum CommandArgs
    {
        RegisterExtension,
        CreateElevetedExtension,
        ExitApp
    }

    public sealed class DesktopExtensionConnectionService : IBackgroundTask
    {
        private BackgroundTaskDeferral backgroundTaskDeferral;
        private AppServiceConnection appServiceConnection;
        private static AppServiceConnection extensionAppServiceConnection = null;
        private static IList<AppServiceConnection> appServiceConnections = new List<AppServiceConnection>();

        private static readonly string _commandLabel = "Command";
        private static readonly string _failedLabel = "Failed";

        public void Run(IBackgroundTaskInstance taskInstance)
        {
            // Get a deferral so that the service isn't terminated.
            this.backgroundTaskDeferral = taskInstance.GetDeferral();

            // Retrieve the app service connection and set up a listener for incoming app service requests.
            var details = taskInstance.TriggerDetails as AppServiceTriggerDetails;
            if(details.CallerPackageFamilyName == Package.Current.Id.FamilyName)
            {
                appServiceConnection = details.AppServiceConnection;
                appServiceConnections.Add(appServiceConnection);
                appServiceConnection.RequestReceived += OnRequestReceived;

                // Associate a cancellation handler with the background task.
                taskInstance.Canceled += OnTaskCanceled;
            }
            else
            {
                this.backgroundTaskDeferral.Complete();
            }
        }

        private async void OnRequestReceived(AppServiceConnection sender, AppServiceRequestReceivedEventArgs args)
        {
            // Get a deferral because we use an awaitable API below to respond to the message
            // and we don't want this call to get canceled while we are waiting.
            var messageDeferral = args.GetDeferral();

            var message = args.Request.Message;
            if (!message.ContainsKey(_commandLabel) || !Enum.TryParse(typeof(CommandArgs), (string)message[_commandLabel], out var result)) return;
            var command = (CommandArgs)result;

            switch (command)
            {
                case CommandArgs.RegisterExtension:
                    appServiceConnections.Remove(appServiceConnection);
                    if (extensionAppServiceConnection == null)
                    {
                        extensionAppServiceConnection = appServiceConnection;
                    }
                    else
                    {
                        this.backgroundTaskDeferral.Complete();
                    }
                    break;
                case CommandArgs.CreateElevetedExtension:
                    if (appServiceConnection == extensionAppServiceConnection)
                    {
                        Parallel.ForEach(appServiceConnections, async (serviceConnection) =>
                        {
                            await serviceConnection.SendMessageAsync(args.Request.Message);
                        });
                    }
                    else
                    {
                        if (extensionAppServiceConnection == null)
                        {
                            message.Clear();
                            message.Add(_failedLabel, true);
                        }
                        else
                        {
                            var response = await extensionAppServiceConnection.SendMessageAsync(message);
                            if (response.Status != AppServiceResponseStatus.Success)
                            {
                                message.Clear();
                                message.Add(_failedLabel, true);
                                extensionAppServiceConnection = null;
                            }
                            else
                            {
                                message = response.Message;
                            }
                        }
                        await args.Request.SendResponseAsync(message);
                    }
                    break;
            }

            messageDeferral.Complete();
        }

        private async void OnTaskCanceled(IBackgroundTaskInstance sender, BackgroundTaskCancellationReason reason)
        {
            if (this.backgroundTaskDeferral != null)
            {
                // Complete the service deferral.
                this.backgroundTaskDeferral.Complete();
                var details = sender.TriggerDetails as AppServiceTriggerDetails;
                var serviceConnection = details.AppServiceConnection;
                if (serviceConnection == extensionAppServiceConnection)
                {
                    extensionAppServiceConnection = null;
                }
                else
                {
                    appServiceConnections.Remove(serviceConnection);
                    if (appServiceConnections.Count == 0 && extensionAppServiceConnection != null)
                    {
                        var message = new ValueSet();
                        message.Add(_commandLabel, CommandArgs.ExitApp.ToString());
                        await extensionAppServiceConnection.SendMessageAsync(message);
                    }
                }
            }
        }
    }
}