namespace Notepads.Services
{
    using Notepads.AdminService;
    using Notepads.Extensions;
    using Notepads.Settings;
    using System;
    using System.Threading.Tasks;
    using Windows.ApplicationModel;
    using Windows.ApplicationModel.AppService;
    using Windows.ApplicationModel.Core;
    using Windows.ApplicationModel.Resources;
    using Windows.Foundation.Collections;
    using Windows.UI.ViewManagement;
    using Windows.UI.Xaml;

    public class AdminstratorAccessException : Exception
    {
        public AdminstratorAccessException():base("Failed to save due to no Adminstration access") { }
    }

    public static class InteropService
    {
        public static AppServiceConnection InteropServiceConnection = null;
        public static AdminServiceClient AdminServiceClient = new AdminServiceClient();
        private static bool _isAdminExtensionAvailable = false;
        private static readonly ResourceLoader _resourceLoader = ResourceLoader.GetForCurrentView();

        private static readonly string _commandLabel = "Command";
        private static readonly string _failedLabel = "Failed";
        private static readonly string _adminCreatedLabel = "AdminCreated";

        public static async Task Initialize()
        {
            InteropServiceConnection = new AppServiceConnection()
            {
                AppServiceName = "InteropServiceConnection",
                PackageFamilyName = Package.Current.Id.FamilyName
            };

            InteropServiceConnection.RequestReceived += InteropServiceConnection_RequestReceived;
            InteropServiceConnection.ServiceClosed += InteropServiceConnection_ServiceClosed;

            AppServiceConnectionStatus status = await InteropServiceConnection.OpenAsync();
            if (status != AppServiceConnectionStatus.Success && !await ApplicationView.GetForCurrentView().TryConsolidateAsync()) 
                Application.Current.Exit();
        }

        private static async void InteropServiceConnection_RequestReceived(AppServiceConnection sender, AppServiceRequestReceivedEventArgs args)
        {
            var message = args.Request.Message;
            if (!message.ContainsKey(_commandLabel) || !Enum.TryParse(typeof(CommandArgs), (string)message[_commandLabel], out var commandObj) ||
                (CommandArgs)commandObj != CommandArgs.CreateElevetedExtension) return;

            await DispatcherExtensions.CallOnUIThreadAsync(CoreApplication.GetCurrentView().Dispatcher, () =>
            {
                if (message.ContainsKey(_adminCreatedLabel) && (bool)message[_adminCreatedLabel])
                {
                    NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("TextEditor_NotificationMsg_AdminExtensionCreated"), 1500);
                    _isAdminExtensionAvailable = true;
                }
                else
                {
                    NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("TextEditor_NotificationMsg_AdminExtensionCreationFailed"), 1500);
                }
            });
        }

        private static void InteropServiceConnection_ServiceClosed(AppServiceConnection sender, AppServiceClosedEventArgs args)
        {
            return;
        }

        public static async Task SaveFileAsAdmin(string filePath, byte[] data)
        {
            if (InteropServiceConnection == null) await Initialize();

            bool failedFromAdminExtension = false;
            if (_isAdminExtensionAvailable)
            {
                try
                {
                    ApplicationSettingsStore.Write(SettingsKey.AdminAuthenticationTokenStr, Guid.NewGuid().ToString());
                    failedFromAdminExtension = !await AdminServiceClient.SaveFileAsync(filePath, data);
                }
                catch
                {
                    _isAdminExtensionAvailable = false;
                }
                finally
                {
                    ApplicationSettingsStore.Remove(SettingsKey.AdminAuthenticationTokenStr);
                }
            }
            else
            {
                throw new AdminstratorAccessException();
            }

            if (failedFromAdminExtension)
            {
                throw new UnauthorizedAccessException();
            }
        }

        public static async Task CreateElevetedExtension()
        {
            if (InteropServiceConnection == null) await Initialize();

            var message = new ValueSet();
            message.Add(_commandLabel, CommandArgs.CreateElevetedExtension.ToString());
            var response = await InteropServiceConnection.SendMessageAsync(message);
            message = response.Message;

            if (message.ContainsKey(_failedLabel) && (bool)message[_failedLabel])
            {
                await FullTrustProcessLauncher.LaunchFullTrustProcessForCurrentAppAsync("LaunchAdmin");
            }
        }
    }
}
