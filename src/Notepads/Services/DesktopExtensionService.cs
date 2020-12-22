namespace Notepads.Services
{
    using Microsoft.AppCenter;
    using Notepads.Extensions;
    using Notepads.Settings;
    using System;
    using System.IO;
    using System.IO.MemoryMappedFiles;
    using System.IO.Pipes;
    using System.Security.Principal;
    using System.Threading.Tasks;
    using Windows.ApplicationModel;
    using Windows.ApplicationModel.AppService;
    using Windows.ApplicationModel.Core;
    using Windows.ApplicationModel.Resources;
    using Windows.Foundation.Collections;
    using Windows.Foundation.Metadata;
    using Windows.Security.Authentication.Web;
    using Windows.UI.ViewManagement;
    using Windows.UI.Xaml;

    public class AdminstratorAccessException : Exception
    {
        public AdminstratorAccessException():base("Failed to save due to no Adminstration access") { }

        public AdminstratorAccessException(string message) : base(message) { }
        public AdminstratorAccessException(string message, Exception innerException) : base(message, innerException) { }
    }

    public static class DesktopExtensionService
    {
        public static AppServiceConnection InteropServiceConnection = null;
        public static readonly bool ShouldUseDesktopExtension = ApiInformation.IsApiContractPresent("Windows.ApplicationModel.FullTrustAppContract", 1, 0) &&
            !new WindowsPrincipal(WindowsIdentity.GetCurrent()).IsInRole(WindowsBuiltInRole.Administrator);
        private static readonly ResourceLoader _resourceLoader = ResourceLoader.GetForCurrentView();

        public static async void InitializeDesktopExtension()
        {
            if (!ApiInformation.IsApiContractPresent("Windows.ApplicationModel.FullTrustAppContract", 1, 0) ||
            new WindowsPrincipal(WindowsIdentity.GetCurrent()).IsInRole(WindowsBuiltInRole.Administrator)) return;

            ApplicationSettingsStore.Write(SettingsKey.PackageSidStr, WebAuthenticationBroker.GetCurrentApplicationCallbackUri().Host.ToUpper());
            var appcenterInstallidstr = (await AppCenter.GetInstallIdAsync())?.ToString();
            ApplicationSettingsStore.Write(SettingsKey.AppCenterInstallIdStr, appcenterInstallidstr);
            await FullTrustProcessLauncher.LaunchFullTrustProcessForCurrentAppAsync();
        }

        public static async Task<bool> Initialize()
        {
            if (!ShouldUseDesktopExtension) return false;

            InteropServiceConnection = new AppServiceConnection()
            {
                AppServiceName = SettingsKey.InteropServiceName,
                PackageFamilyName = Package.Current.Id.FamilyName
            };

            InteropServiceConnection.RequestReceived += InteropServiceConnection_RequestReceived;
            InteropServiceConnection.ServiceClosed += InteropServiceConnection_ServiceClosed;

            AppServiceConnectionStatus status = await InteropServiceConnection.OpenAsync();
            if (status != AppServiceConnectionStatus.Success && !await ApplicationView.GetForCurrentView().TryConsolidateAsync())
            {
                Application.Current.Exit();
            }

            return true;
        }

        private static async void InteropServiceConnection_RequestReceived(AppServiceConnection sender, AppServiceRequestReceivedEventArgs args)
        {
            var message = args.Request.Message;
            if (!message.ContainsKey(SettingsKey.InteropCommandLabel) ||
                !SettingsKey.CreateElevetedExtensionCommandStr.Equals(message[SettingsKey.InteropCommandLabel])) return;

            await CoreApplication.MainView.CoreWindow.Dispatcher.CallOnUIThreadAsync(() =>
            {
                if (message.ContainsKey(SettingsKey.InteropCommandAdminCreatedLabel) && (bool)message[SettingsKey.InteropCommandAdminCreatedLabel])
                {
                    NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("TextEditor_NotificationMsg_AdminExtensionCreated"), 1500);
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

        /// <summary>
        /// Write data to system file.
        /// </summary>
        /// <remarks>
        /// Only available for legacy Windows 10 desktop.
        /// </remarks>
        /// <param name="filePath">Absolute path of the file to write.</param>
        /// <param name="data">Data to write.</param>
        public static async Task SaveFileAsAdmin(string filePath, byte[] data)
        {
            if (InteropServiceConnection == null && !(await Initialize())) return;

            using (var adminConnectionPipeStream = new NamedPipeServerStream(
                $"Local\\{Package.Current.Id.FamilyName}\\{SettingsKey.AdminPipeConnectionNameStr}",
                PipeDirection.InOut,
                NamedPipeServerStream.MaxAllowedServerInstances,
                PipeTransmissionMode.Message,
                PipeOptions.Asynchronous))
            {
                // Wait for 250 ms for desktop extension to accept request.
                if (!adminConnectionPipeStream.WaitForConnectionAsync().Wait(250))
                {
                    // If the connection fails, desktop extension is not launched with elevated privilages.
                    // In that case, prompt user to launch desktop extension with elevated privilages.
                    throw new AdminstratorAccessException();
                }

                var pipeReader = new StreamReader(adminConnectionPipeStream, System.Text.Encoding.Unicode);
                var pipeWriter = new StreamWriter(adminConnectionPipeStream);

                var mapName = filePath.Replace(Path.DirectorySeparatorChar, '-');

                // Create the memory-mapped file and write original file data to be written.
                using (var mmf = MemoryMappedFile.CreateOrOpen(mapName, data.Length > 0 ? data.Length : 1, MemoryMappedFileAccess.ReadWrite))
                {
                    using (var writer = new BinaryWriter(mmf.CreateViewStream()))
                    {
                        writer.Write(data);
                        writer.Flush();
                    }

                    await pipeWriter.WriteAsync($"{filePath}|{mapName}|{data.Length}");
                    await pipeWriter.FlushAsync();

                    // Wait for desktop extension to send response.
                    if ("Success".Equals(await pipeReader.ReadLineAsync()))
                    {
                        return;
                    }
                    else
                    {
                        // Promt user to "Save As" if extension failed to save data.
                        throw new UnauthorizedAccessException();
                    }
                }
            }
        }

        /// <summary>
        /// Launch desktop extension with elevated privilages.
        /// </summary>
        /// <remarks>
        /// Only available for legacy Windows 10 desktop.
        /// </remarks>
        public static async Task CreateElevetedExtension()
        {
            if (InteropServiceConnection == null && !(await Initialize())) return;

            var message = new ValueSet { { SettingsKey.InteropCommandLabel, SettingsKey.CreateElevetedExtensionCommandStr } };
            var response = await InteropServiceConnection.SendMessageAsync(message);
            message = response.Message;

            if (message?.ContainsKey(SettingsKey.InteropCommandFailedLabel) ?? false && (bool)message[SettingsKey.InteropCommandFailedLabel])
            {
                // Pass the group id that describes the correct parameter for prompting admin process launch
                await FullTrustProcessLauncher.LaunchFullTrustProcessForCurrentAppAsync("LaunchAdmin");
            }
        }
    }
}
