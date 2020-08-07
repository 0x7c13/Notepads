namespace Notepads.DesktopExtension
{
    using Notepads.Settings;
    using System;
    using System.Diagnostics;
    using System.IO;
    using System.IO.MemoryMappedFiles;
    using System.IO.Pipes;
    using System.Reflection;
    using System.Security.Principal;
    using System.Threading;
    using Windows.ApplicationModel;
    using Windows.ApplicationModel.AppService;
    using Windows.Foundation.Collections;
    using Windows.Storage;

    public enum CommandArgs
    {
        RegisterExtension,
        CreateElevetedExtension,
        ExitApp
    }

    static class Program
    {
        private const string DesktopExtensionMutexName = "DesktopExtensionMutexName";
        private const string AdminExtensionMutexName = "AdminExtensionMutexName";

        private static AppServiceConnection connection = null;
        private static AutoResetEvent appServiceExit;

        /// <summary>
        ///  The main entry point for the application.
        /// </summary>
        static void Main(string[] args)
        {
            if (new WindowsPrincipal(WindowsIdentity.GetCurrent()).IsInRole(WindowsBuiltInRole.Administrator))
            {
                if (!IsFirstInstance(AdminExtensionMutexName)) return;
                InitializeExtensionService();
            }
            else
            {
                if (!IsFirstInstance(DesktopExtensionMutexName)) return;
                InitializeAppServiceConnection();
                if (args.Length > 2 && args[2] == "/admin") CreateElevetedExtension();
            }

            appServiceExit = new AutoResetEvent(false);
            appServiceExit.WaitOne();
        }

        private static async void InitializeAppServiceConnection()
        {
            connection = new AppServiceConnection()
            {
                AppServiceName = "DesktopExtensionServiceConnection",
                PackageFamilyName = Package.Current.Id.FamilyName
            };

            connection.RequestReceived += Connection_RequestReceived;
            connection.ServiceClosed += Connection_ServiceClosed;

            AppServiceConnectionStatus status = await connection.OpenAsync();

            if (status != AppServiceConnectionStatus.Success)
            {
                appServiceExit.Set();
            }

            var message = new ValueSet { { SettingsKey.InteropCommandLabel, CommandArgs.RegisterExtension.ToString() } };
            await connection.SendMessageAsync(message);
        }

        private static void InitializeExtensionService()
        {
            SaveFileFromPipeData();
        }

        private static bool IsFirstInstance(string mutexName)
        {
            var instanceHandlerMutex = new Mutex(true, mutexName, out var isFirstInstance);

            if (isFirstInstance)
            {
                instanceHandlerMutex.ReleaseMutex();
            }
            else
            {
                instanceHandlerMutex.Close();
            }

            return isFirstInstance;
        }

        private static void Connection_ServiceClosed(AppServiceConnection sender, AppServiceClosedEventArgs args)
        {
            // connection to the UWP lost, so we shut down the desktop process
            appServiceExit.Set();
        }

        private static void Connection_RequestReceived(AppServiceConnection sender, AppServiceRequestReceivedEventArgs args)
        {
            // Get a deferral because we use an awaitable API below to respond to the message
            // and we don't want this call to get canceled while we are waiting.
            var messageDeferral = args.GetDeferral();

            var message = args.Request.Message;
            if (!message.ContainsKey(SettingsKey.InteropCommandLabel) || !Enum.TryParse<CommandArgs>((string)message[SettingsKey.InteropCommandLabel], out var command)) return;
            switch (command)
            {
                case CommandArgs.CreateElevetedExtension:
                    CreateElevetedExtension();
                    break;
                case CommandArgs.ExitApp:
                    appServiceExit.Set();
                    break;
            }

            messageDeferral.Complete();
        }

        private static async void CreateElevetedExtension()
        {
            var message = new ValueSet { { SettingsKey.InteropCommandLabel, CommandArgs.CreateElevetedExtension.ToString() } };

            try
            {
                string result = Assembly.GetExecutingAssembly().Location;
                int index = result.LastIndexOf("\\");
                string rootPath = $"{result.Substring(0, index)}\\..\\";
                string aliasPath = rootPath + @"\Notepads.DesktopExtension\Notepads32.exe";

                ProcessStartInfo info = new ProcessStartInfo
                {
                    Verb = "runas",
                    UseShellExecute = true,
                    FileName = aliasPath
                };

                var process = Process.Start(info);
                AppDomain.CurrentDomain.ProcessExit += (a, b) => process.Kill();
                message.Add(SettingsKey.InteropCommandAdminCreatedLabel, true);
            }
            catch
            {
                message.Add(SettingsKey.InteropCommandAdminCreatedLabel, false);
            }
            finally
            {
                await connection.SendMessageAsync(message);
            }
        }

        private static async void SaveFileFromPipeData()
        {
            using var clientStream = new NamedPipeClientStream(".",
                $"Sessions\\1\\AppContainerNamedObjects\\{ReadSettingsKey(SettingsKey.PackageSidStr)}\\{Package.Current.Id.FamilyName}\\AdminWritePipe",
                PipeDirection.InOut, PipeOptions.Asynchronous);

            // Wait for uwp app to send request to write to file.
            await clientStream.ConnectAsync();

            // Start another thread to accept more write requests.
            new Thread(new ThreadStart(SaveFileFromPipeData)).Start();

            var pipeReader = new StreamReader(clientStream);
            var pipeWriter = new StreamWriter(clientStream);

            var writeData = pipeReader.ReadLine().Split(new string[] { "?:" }, StringSplitOptions.None);

            var memoryMapName = writeData[0];
            var filePath = writeData[1];
            int.TryParse(writeData[2], out int dataArrayLength);

            var result = "Failed";
            try
            {
                // Open the memory-mapped file and read data from it.
                using (var reader = new BinaryReader(MemoryMappedFile.OpenExisting(memoryMapName).CreateViewStream()))
                {
                    var data = reader.ReadBytes(dataArrayLength);

                    await PathIO.WriteBytesAsync(filePath, data);
                }

                result = "Success";
            }
            catch
            {
                // Do nothing
            }
            finally
            {
                pipeWriter.WriteLine(result);
                pipeWriter.Flush();
            }
        }

        public static object ReadSettingsKey(string key)
        {
            object obj = null;

            ApplicationDataContainer localSettings = ApplicationData.Current.LocalSettings;
            if (localSettings.Values.ContainsKey(key))
            {
                obj = localSettings.Values[key];
            }

            return obj;
        }
    }
}
