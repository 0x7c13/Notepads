namespace Notepads.Services
{
    using Microsoft.AppCenter;
    using Notepads.Controls.Dialog;
    using Notepads.Settings;
    using Notepads.Utilities;
    using System;
    using System.IO;
    using System.IO.MemoryMappedFiles;
    using System.IO.Pipes;
    using System.Security.Principal;
    using System.Threading;
    using System.Threading.Tasks;
    using Windows.ApplicationModel;
    using Windows.ApplicationModel.DataTransfer;
    using Windows.ApplicationModel.Resources;
    using Windows.Foundation.Metadata;
    using Windows.Security.Authentication.Web;
    using Windows.Storage;

    public enum AdminOperationType
    {
        Save,
        Rename
    }

    public class AdminstratorAccessException : Exception
    {
        public AdminstratorAccessException():base("Failed to save due to no Adminstration access") { }

        public AdminstratorAccessException(string message) : base(message) { }
        public AdminstratorAccessException(string message, Exception innerException) : base(message, innerException) { }
    }

    public static class DesktopExtensionService
    {
        public static readonly bool ShouldUseDesktopExtension = 
            ApiInformation.IsApiContractPresent("Windows.ApplicationModel.FullTrustAppContract", 1, 0) &&
            !new WindowsPrincipal(WindowsIdentity.GetCurrent()).IsInRole(WindowsBuiltInRole.Administrator);

        public static Mutex ExtensionLifetimeObject = null;

        private static readonly ResourceLoader _resourceLoader = ResourceLoader.GetForCurrentView();

        private static EventWaitHandle _adminWriteEvent = null;
        private static EventWaitHandle _adminRenameEvent = null;

        public static void Initialize()
        {
            if (!ShouldUseDesktopExtension) return;

            if (ExtensionLifetimeObject == null)
            {
                ExtensionLifetimeObject = new Mutex(false, SettingsKey.DesktopExtensionLifetimeObjNameStr);
            }

            if (_adminWriteEvent == null)
            {
                _adminWriteEvent = new EventWaitHandle(false, EventResetMode.ManualReset, SettingsKey.AdminWriteEventNameStr);
            }

            if (_adminRenameEvent == null)
            {
                _adminRenameEvent = new EventWaitHandle(false, EventResetMode.ManualReset, SettingsKey.AdminRenameEventNameStr);
            }
        }

        /// <summary>
        /// Launch desktop extension with elevated privilages.
        /// </summary>
        /// <remarks>
        /// Only available for legacy Windows 10 desktop.
        /// </remarks>
        public static async Task LaunchElevetedProcess()
        {
            if (!ShouldUseDesktopExtension) return;

            ApplicationSettingsStore.Write(SettingsKey.PackageSidStr, WebAuthenticationBroker.GetCurrentApplicationCallbackUri().Host.ToUpper());
            ApplicationSettingsStore.Write(SettingsKey.AppCenterInstallIdStr, (await AppCenter.GetInstallIdAsync())?.ToString());
            await FullTrustProcessLauncher.LaunchFullTrustProcessForCurrentAppAsync();
        }

        /// <summary>
        /// Notify user whether launching of eleveted process succeeded.
        /// </summary>
        /// <remarks>
        /// Only available for legacy Windows 10 desktop.
        /// </remarks>
        /// <param name="isLaunched">Is eleveted process launched.</param>
        public static void OnElevetedProcessLaunchRequested(bool isLaunched)
        {
            if (isLaunched)
            {
                NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("TextEditor_NotificationMsg_AdminExtensionCreated"), 1500);
            }
            else
            {
                NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("TextEditor_NotificationMsg_AdminExtensionCreationFailed"), 1500);
            }
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
            if (!ShouldUseDesktopExtension) return;

            using (var adminConnectionPipeStream = new NamedPipeServerStream(
                $"Local\\{SettingsKey.AdminWritePipeConnectionNameStr}",
                PipeDirection.InOut,
                NamedPipeServerStream.MaxAllowedServerInstances,
                PipeTransmissionMode.Message,
                PipeOptions.Asynchronous))
            {
                _adminWriteEvent.Set();
                // Wait for 250 ms for desktop extension to accept request.
                if (!adminConnectionPipeStream.WaitForConnectionAsync().Wait(100))
                {
                    // If the connection fails, desktop extension is not launched with elevated privilages.
                    // In that case, prompt user to launch desktop extension with elevated privilages.
                    _adminWriteEvent.Reset();
                    throw new AdminstratorAccessException();
                }

                var pipeReader = new StreamReader(adminConnectionPipeStream, new System.Text.UnicodeEncoding(!BitConverter.IsLittleEndian, false));
                var pipeWriter = new StreamWriter(adminConnectionPipeStream, new System.Text.UnicodeEncoding(!BitConverter.IsLittleEndian, false));

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
        /// Rename system file.
        /// </summary>
        /// <remarks>
        /// Only available for legacy Windows 10 desktop.
        /// </remarks>
        /// <param name="file">File to rename</param>
        /// <param name="newName">New name</param>
        public static async Task<StorageFile> RenameFileAsAdmin(StorageFile file, string newName)
        {
            if (!ShouldUseDesktopExtension) return null;

            using (var adminConnectionPipeStream = new NamedPipeServerStream(
                $"Local\\{SettingsKey.AdminRenamePipeConnectionNameStr}",
                PipeDirection.InOut,
                NamedPipeServerStream.MaxAllowedServerInstances,
                PipeTransmissionMode.Message,
                PipeOptions.Asynchronous))
            {
                _adminRenameEvent.Set();
                // Wait for 250 ms for desktop extension to accept request.
                if (!adminConnectionPipeStream.WaitForConnectionAsync().Wait(100))
                {
                    // If the connection fails, desktop extension is not launched with elevated privilages.
                    // In that case, prompt user to launch desktop extension with elevated privilages.
                    _adminRenameEvent.Reset();
                    var launchElevatedExtensionDialog = new LaunchElevatedExtensionDialog(
                        AdminOperationType.Rename,
                        file.Path,
                        async () => { await DesktopExtensionService.LaunchElevetedProcess(); },
                        null);

                    var dialogResult = await DialogManager.OpenDialogAsync(launchElevatedExtensionDialog, awaitPreviousDialog: false);

                    return null;
                }

                var pipeReader = new StreamReader(adminConnectionPipeStream, new System.Text.UnicodeEncoding(!BitConverter.IsLittleEndian, false));
                var pipeWriter = new StreamWriter(adminConnectionPipeStream, new System.Text.UnicodeEncoding(!BitConverter.IsLittleEndian, false));

                var token = SharedStorageAccessManager.AddFile(file);
                await pipeWriter.WriteAsync($"{token}|{newName}");
                await pipeWriter.FlushAsync();

                // Wait for desktop extension to send response.
                token = await pipeReader.ReadLineAsync();
                file = await SharedStorageAccessManager.RedeemTokenForFileAsync(token);
                SharedStorageAccessManager.RemoveFile(token);
                return file;
            }
        }
    }
}
