namespace Notepads.Services
{
    using Microsoft.AppCenter;
    using Microsoft.AppCenter.Analytics;
    using Microsoft.AppCenter.Crashes;
    using Notepads.Settings;
    using System;
    using System.Collections.Generic;
    using System.Threading.Tasks;
    using Windows.ApplicationModel;
    using Windows.ApplicationModel.AppService;
    using Windows.ApplicationModel.Background;

    public sealed class DesktopExtensionConnectionService : IBackgroundTask
    {
        private BackgroundTaskDeferral backgroundTaskDeferral;
        private AppServiceConnection appServiceConnection;
        private static AppServiceConnection extensionAppServiceConnection = null;
        private static readonly IList<AppServiceConnection> appServiceConnections = new List<AppServiceConnection>();

        public void Run(IBackgroundTaskInstance taskInstance)
        {
            // Get a deferral so that the service isn't terminated.
            this.backgroundTaskDeferral = taskInstance.GetDeferral();

            TaskScheduler.UnobservedTaskException += OnUnobservedException;

            var services = new Type[] { typeof(Crashes), typeof(Analytics) };
            AppCenter.Start(SettingsKey.AppCenterSecret, services);

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
            if (!message.ContainsKey(SettingsKey.InteropCommandLabel) ||
                !(message[SettingsKey.InteropCommandLabel] is string command)) return;

            switch (command)
            {
                case SettingsKey.RegisterExtensionCommandStr:
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
                case SettingsKey.CreateElevetedExtensionCommandStr:
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
                            message.Add(SettingsKey.InteropCommandFailedLabel, true);
                        }
                        else
                        {
                            var response = await extensionAppServiceConnection.SendMessageAsync(message);
                            if (response.Status != AppServiceResponseStatus.Success)
                            {
                                message.Clear();
                                message.Add(SettingsKey.InteropCommandFailedLabel, true);
                                extensionAppServiceConnection = null;
                            }
                            else
                            {
                                message = response.Message;
                            }
                        }

                        var res = await args.Request.SendResponseAsync(message);
                        if (res != AppServiceResponseStatus.Success)
                        {
                            var info = new Dictionary<string, string>()
                            {
                                { "Response", res.ToString() }
                            };

                            Analytics.TrackEvent("OnAppServiceConnectionResponseToUWPFailed", info);
                        }
                    }
                    break;
            }

            messageDeferral.Complete();
        }

        private void OnTaskCanceled(IBackgroundTaskInstance sender, BackgroundTaskCancellationReason reason)
        {
            var deferral = sender.GetDeferral();

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
                }
            }

            deferral.Complete();
        }

        // Occurs when an exception is not handled on a background thread.
        // ie. A task is fired and forgotten Task.Run(() => {...})
        private static void OnUnobservedException(object sender, UnobservedTaskExceptionEventArgs e)
        {
            //LoggingService.LogError($"[{nameof(App)}] OnUnobservedException: {e.Exception}");

            var diagnosticInfo = new Dictionary<string, string>()
            {
                { "Message", e.Exception?.Message },
                { "Exception", e.Exception?.ToString() },
                { "InnerException", e.Exception?.InnerException?.ToString() },
                { "InnerExceptionMessage", e.Exception?.InnerException?.Message }
            };

            var attachment = ErrorAttachmentLog.AttachmentWithText(
                $"Exception: {e.Exception}, " +
                $"Message: {e.Exception?.Message}, " +
                $"InnerException: {e.Exception?.InnerException}, " +
                $"InnerExceptionMessage: {e.Exception?.InnerException?.Message}",
                "AppServiceUnobservedException");

            Analytics.TrackEvent("OnAppServiceUnobservedException", diagnosticInfo);
            Crashes.TrackError(e.Exception, diagnosticInfo, attachment);

            // suppress and handle it manually.
            e.SetObserved();
        }
    }
}
