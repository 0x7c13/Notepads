namespace Notepads.Services
{
    using System;
    using System.Collections.Generic;
    using Windows.ApplicationModel;
    using Windows.ApplicationModel.AppService;
    using Windows.ApplicationModel.Background;

    public enum CommandArgs
    {
        SyncSettings,
        SyncRecentList
    }

    public sealed class InteropConnectionService : IBackgroundTask
    {
        private BackgroundTaskDeferral backgroundTaskDeferral;
        private AppServiceConnection appServiceConnection;
        private static IList<AppServiceConnection> appServiceConnections = new List<AppServiceConnection>();

        private static readonly string _commandLabel = "Command";

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
            var messagedeferral = args.GetDeferral();
            var message = args.Request.Message;
            if (!message.ContainsKey(_commandLabel) || !Enum.TryParse(typeof(CommandArgs), (string)message[_commandLabel], out var result)) return;
            var command = (CommandArgs)result;
            switch (command)
            {
                case CommandArgs.SyncSettings:
                    foreach (var serviceConnection in appServiceConnections)
                    {
                        if (serviceConnection != appServiceConnection) await serviceConnection.SendMessageAsync(args.Request.Message);
                    }
                    break;
                case CommandArgs.SyncRecentList:
                    foreach (var serviceConnection in appServiceConnections)
                    {
                        if (serviceConnection != appServiceConnection) await serviceConnection.SendMessageAsync(args.Request.Message);
                    }
                    break;
            }
            messagedeferral.Complete();
        }

        private void OnTaskCanceled(IBackgroundTaskInstance sender, BackgroundTaskCancellationReason reason)
        {
            if (this.backgroundTaskDeferral != null)
            {
                // Complete the service deferral.
                this.backgroundTaskDeferral.Complete();
                var details = sender.TriggerDetails as AppServiceTriggerDetails;
                appServiceConnections.Remove(details.AppServiceConnection);
            }
        }
    }
}
