namespace Notepads.DesktopExtension
{
    using System;
    using System.Diagnostics;
    using System.Reflection;
    using System.Security.Principal;
    using System.ServiceModel;
    using System.Threading;
    using System.Threading.Tasks;
    using System.Windows;
    using Windows.ApplicationModel;
    using Windows.ApplicationModel.AppService;
    using Windows.Foundation.Collections;
    using Windows.Storage;
    using Windows.System;

    public enum CommandArgs
    {
        SyncSettings,
        SyncRecentList,
        RegisterExtension,
        CreateElevetedExtension,
        ExitApp
    }

    /// <summary>
    /// Interaction logic for App.xaml
    /// </summary>
    public partial class App : Application
    {
#if DEBUG
        private const string TargetPackageFamilyName = "Notepads_ezhh5fms182ha";
#else
        private const string TargetPackageFamilyName = "19282JackieLiu.Notepads-Beta_echhpq9pdbte8";
#endif
        private static readonly Uri uri = new Uri("notepads:");
        private static string DesktopExtensionMutexIdStr = "DesktopExtensionMutexIdStr";
        private static string AdminExtensionMutexIdStr = "AdminExtensionMutexIdStr";
        private static Mutex mutex;

        private static AppServiceConnection connection = null;
        private static ServiceHost selfHost = null;
        private static readonly string _commandLabel = "Command";
        private static readonly string _adminCreatedLabel = "AdminCreated";

        public App()
        {
            this.Startup += App_Startup;
        }

        private async void App_Startup(object sender, StartupEventArgs e)
        {
            var args = e.Args;
            if (new WindowsPrincipal(WindowsIdentity.GetCurrent()).IsInRole(WindowsBuiltInRole.Administrator))
            {
                try
                {
                    CheckInstance(AdminExtensionMutexIdStr);
                    InitializeExtensionService();
                }
                catch (InvalidOperationException)
                {
                    if (args.Length > 0) await LaunchWithhNotepads(args);
                    Application.Current.Dispatcher.InvokeShutdown();
                }
            }
            else
            {
                try
                {
                    CheckInstance(DesktopExtensionMutexIdStr);
                    InitializeAppServiceConnection();
                }
                catch (InvalidOperationException)
                {
                    if (args.Length > 0) await LaunchWithhNotepads(args);
                    Application.Current.Dispatcher.InvokeShutdown();
                }
            }
        }

        private async Task LaunchWithhNotepads(string[] filePaths)
        {
            var options = new LauncherOptions { TargetApplicationPackageFamilyName = TargetPackageFamilyName };
            foreach (var filePath in filePaths)
            {
                try
                {
                    var file = await StorageFile.GetFileFromPathAsync(filePath);
                    await Launcher.LaunchFileAsync(file, options);
                }
                catch { }
            }
        }

        private static async void InitializeAppServiceConnection()
        {
            connection = new AppServiceConnection()
            {
                AppServiceName = "InteropServiceConnection",
                PackageFamilyName = Package.Current.Id.FamilyName
            };

            connection.RequestReceived += Connection_RequestReceived;
            connection.ServiceClosed += Connection_ServiceClosed;

            AppServiceConnectionStatus status = await connection.OpenAsync();

            if (status != AppServiceConnectionStatus.Success)
            {
                Application.Current.Dispatcher.InvokeShutdown();
            }

            var message = new ValueSet();
            message.Add(_commandLabel, CommandArgs.RegisterExtension.ToString());
            await connection.SendMessageAsync(message);
        }

        private static void InitializeExtensionService()
        {
            selfHost = new ServiceHost(typeof(AdminService));
            selfHost.Open();
        }

        private static void CheckInstance(string key)
        {
            mutex = new Mutex(true, ReadOrInitializeMutexId(key), out var isFirstInstance);
            if (!isFirstInstance)
            {
                mutex.ReleaseMutex();
                Application.Current.Dispatcher.InvokeShutdown();
            }
        }

        private static void Application_OnApplicationExit(object sender, EventArgs e)
        {
            mutex.Close();
        }

        private static void Connection_ServiceClosed(AppServiceConnection sender, AppServiceClosedEventArgs args)
        {
            // connection to the UWP lost, so we shut down the desktop process
            Application.Current.Dispatcher.InvokeShutdown();
        }

        private static void Connection_RequestReceived(AppServiceConnection sender, AppServiceRequestReceivedEventArgs args)
        {
            // Get a deferral because we use an awaitable API below to respond to the message
            // and we don't want this call to get canceled while we are waiting.
            var messageDeferral = args.GetDeferral();

            var message = args.Request.Message;
            if (!message.ContainsKey(_commandLabel) || !Enum.TryParse<CommandArgs>((string)message[_commandLabel], out var command)) return;
            switch (command)
            {
                case CommandArgs.CreateElevetedExtension:
                    CreateElevetedExtension();
                    break;
                case CommandArgs.ExitApp:
                    Application.Current.Dispatcher.InvokeShutdown();
                    break;
            }

            messageDeferral.Complete();
        }

        private static async void CreateElevetedExtension()
        {
            var message = new ValueSet();
            message.Add(_commandLabel, CommandArgs.CreateElevetedExtension.ToString());
            try
            {
                string result = Assembly.GetExecutingAssembly().Location;
                int index = result.LastIndexOf("\\");
                string rootPath = $"{result.Substring(0, index)}\\..\\";
                string aliasPath = rootPath + @"\Notepads.DesktopExtension\Notepads.DesktopExtension.exe";

                ProcessStartInfo info = new ProcessStartInfo();
                info.Verb = "runas";
                info.UseShellExecute = true;
                info.FileName = aliasPath;
                var process = Process.Start(info);
                AppDomain.CurrentDomain.ProcessExit += (a, b) => process.Kill();
                message.Add(_adminCreatedLabel, true);
            }
            catch
            {
                message.Add(_adminCreatedLabel, false);
            }
            finally
            {
                await connection.SendMessageAsync(message);
            }
        }

        private static string ReadOrInitializeMutexId(string key)
        {
            ApplicationDataContainer localSettings = ApplicationData.Current.LocalSettings;
            if (!localSettings.Values.ContainsKey(key) || !(localSettings.Values[key] is string mutexId) || string.IsNullOrEmpty(mutexId))
            {
                mutexId = Guid.NewGuid().ToString();
                WriteMutexId(key, mutexId);
            }
            return mutexId;
        }

        private static void WriteMutexId(string key, object obj)
        {
            ApplicationDataContainer localSettings = ApplicationData.Current.LocalSettings;
            localSettings.Values[key] = obj;
        }
    }
}
