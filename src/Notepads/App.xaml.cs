namespace Notepads
{
    using System;
    using System.Collections.Generic;
    using System.Linq;
    using System.Threading.Tasks;
    using Microsoft.AppCenter;
    using Microsoft.AppCenter.Analytics;
    using Microsoft.AppCenter.Crashes;
    using Microsoft.Toolkit.Uwp.Helpers;
    using Notepads.Services;
    using Notepads.Settings;
    using Notepads.Utilities;
    using Windows.ApplicationModel;
    using Windows.ApplicationModel.Activation;
    using Windows.ApplicationModel.Core;
    using Windows.UI;
    using Windows.UI.ViewManagement;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Navigation;

    sealed partial class App : Application
    {
        public static string ApplicationName = "Notepads";

        public static Guid Id { get; } = Guid.NewGuid();

        public static bool IsFirstInstance = false;
        public static bool IsGameBarWidget = false;

        private const string AppCenterSecret = null;

        /// <summary>
        /// Initializes the singleton application object.  This is the first line of authored code
        /// executed, and as such is the logical equivalent of main() or WinMain().
        /// </summary>
        public App()
        {
            UnhandledException += OnUnhandledException;
            TaskScheduler.UnobservedTaskException += OnUnobservedException;

            var services = new Type[] { typeof(Crashes), typeof(Analytics) };
            AppCenter.Start(AppCenterSecret, services);

            //await LoggingService.InitializeAsync();
            LoggingService.LogInfo($"[App Started] Instance = {Id} IsFirstInstance: {IsFirstInstance}");

            ApplicationSettingsStore.Write(SettingsKey.ActiveInstanceIdStr, App.Id.ToString());

            InitializeComponent();

            Suspending += OnSuspending;
        }

        private static void OnUnhandledException(object sender, Windows.UI.Xaml.UnhandledExceptionEventArgs e)
        {
            // Occurs when an exception is not handled on the UI thread.

            LoggingService.LogError($"OnUnhandledException: {e.Exception}");

            var diagnosticInfo = new Dictionary<string, string>()
            {
                { "Message", e.Message },
                { "Exception", e.Exception.ToString() },
                { "Culture", SystemInformation.Culture.EnglishName },
                { "AvailableMemory", SystemInformation.AvailableMemory.ToString("F0") },
                { "IsFirstRun", SystemInformation.IsFirstRun.ToString() },
                { "IsFirstRunAfterUpdate", SystemInformation.IsAppUpdated.ToString() },
                { "FirstVersionInstalled", $"{SystemInformation.ApplicationVersion.Major}.{SystemInformation.ApplicationVersion.Minor}.{SystemInformation.ApplicationVersion.Build}.{SystemInformation.ApplicationVersion.Revision}" },
                { "FirstUseTimeUTC", SystemInformation.FirstUseTime.ToUniversalTime().ToString("MM/dd/yyyy HH:mm:ss") },
                { "LastLaunchTimeUTC", SystemInformation.LastLaunchTime.ToUniversalTime().ToString("MM/dd/yyyy HH:mm:ss") },
                { "LaunchTimeUTC", SystemInformation.LaunchTime.ToUniversalTime().ToString("MM/dd/yyyy HH:mm:ss") },
                { "CurrentLaunchCount", SystemInformation.LaunchCount.ToString() },
                { "TotalLaunchCount", SystemInformation.TotalLaunchCount.ToString() },
                { "AppUptime", SystemInformation.AppUptime.ToString() },
                { "OSArchitecture", SystemInformation.OperatingSystemArchitecture.ToString() },
                { "OSVersion", SystemInformation.OperatingSystemVersion.ToString() },
                { "IsShadowWindow", (!IsFirstInstance && !IsGameBarWidget).ToString() },
                { "IsGameBarWidget", IsGameBarWidget.ToString() }
            };

            Analytics.TrackEvent("OnUnhandledException", diagnosticInfo);
            Crashes.TrackError(e.Exception, diagnosticInfo);

            e.Handled = true; // !e.Message.Contains("0x80040154", StringComparison.InvariantCultureIgnoreCase);
        }

        private static void OnUnobservedException(object sender, UnobservedTaskExceptionEventArgs e)
        {
            // Occurs when an exception is not handled on a background thread.
            // ie. A task is fired and forgotten Task.Run(() => {...})

            LoggingService.LogError($"OnUnobservedException: {e.Exception}");

            Analytics.TrackEvent("OnUnobservedException", new Dictionary<string, string>()
            {
                { "Message", e.Exception.Message },
                { "Exception", e.Exception.ToString() }
            });

            // suppress and handle it manually.
            e.SetObserved();
        }

        /// <summary>
        /// Invoked when the application is launched normally by the end user.  Other entry points
        /// will be used such as when the application is launched to open a specific file.
        /// </summary>
        /// <param name="e">Details about the launch request and process.</param>
        protected override async void OnLaunched(LaunchActivatedEventArgs e)
        {
            await ActivateAsync(e);
        }

        protected override async void OnFileActivated(FileActivatedEventArgs args)
        {
            await ActivateAsync(args);
            base.OnFileActivated(args);
        }

        protected override async void OnActivated(IActivatedEventArgs args)
        {
            await ActivateAsync(args);
            base.OnActivated(args);
        }

        private async System.Threading.Tasks.Task ActivateAsync(IActivatedEventArgs e)
        {
            bool rootFrameCreated = false;

            if (!(Window.Current.Content is Frame rootFrame))
            {
                rootFrame = CreateRootFrame(e);
                Window.Current.Content = rootFrame;
                rootFrameCreated = true;

                ThemeSettingsService.Initialize();
                EditorSettingsService.Initialize();
            }

            var appLaunchSettings = new Dictionary<string, string>()
            {
                { "OSArchitecture", SystemInformation.OperatingSystemArchitecture.ToString() },
                { "UseWindowsTheme", ThemeSettingsService.UseWindowsTheme.ToString() },
                { "ThemeMode", ThemeSettingsService.ThemeMode.ToString() },
                { "UseWindowsAccentColor", ThemeSettingsService.UseWindowsAccentColor.ToString() },
                { "AppBackgroundTintOpacity", $"{(int) (ThemeSettingsService.AppBackgroundPanelTintOpacity * 100.0)}" },
                { "ShowStatusBar", EditorSettingsService.ShowStatusBar.ToString() },
                { "EditorDefaultLineEnding", EditorSettingsService.EditorDefaultLineEnding.ToString() },
                { "EditorDefaultEncoding", EncodingUtility.GetEncodingName(EditorSettingsService.EditorDefaultEncoding) },
                { "EditorDefaultTabIndents", EditorSettingsService.EditorDefaultTabIndents.ToString() },
                { "EditorDefaultDecoding", EditorSettingsService.EditorDefaultDecoding == null ? "Auto" : EncodingUtility.GetEncodingName(EditorSettingsService.EditorDefaultDecoding) },
                { "EditorFontFamily", EditorSettingsService.EditorFontFamily },
                { "EditorFontSize", EditorSettingsService.EditorFontSize.ToString() },
                { "IsSessionSnapshotEnabled", EditorSettingsService.IsSessionSnapshotEnabled.ToString() },
                { "IsShadowWindow", (!IsFirstInstance && !IsGameBarWidget).ToString() },
                { "IsGameBarWidget", IsGameBarWidget.ToString() },
                { "AlwaysOpenNewWindow", EditorSettingsService.AlwaysOpenNewWindow.ToString() },
                { "IsHighlightMisspelledWordsEnabled", EditorSettingsService.IsHighlightMisspelledWordsEnabled.ToString() },
                { "IsLineHighlighterEnabled", EditorSettingsService.IsLineHighlighterEnabled.ToString() },
                { "EditorDefaultSearchEngine", EditorSettingsService.EditorDefaultSearchEngine.ToString() }
            };

            LoggingService.LogInfo($"AppLaunchSettings: {string.Join(";", appLaunchSettings.Select(x => x.Key + "=" + x.Value).ToArray())}");
            Analytics.TrackEvent("AppLaunch_Settings", appLaunchSettings);

            try
            {
                await ActivationService.ActivateAsync(rootFrame, e);
            }
            catch (Exception ex)
            {
                throw new Exception("AppFailedToActivate", ex);
            }

            if (rootFrameCreated)
            {
                Window.Current.Activate();
                ExtendAcrylicIntoTitleBar();
            }
        }

        private Frame CreateRootFrame(IActivatedEventArgs e)
        {
            Frame rootFrame = new Frame();
            rootFrame.NavigationFailed += OnNavigationFailed;

            if (e.PreviousExecutionState == ApplicationExecutionState.Terminated)
            {
                //TODO: Load state from previously suspended application
            }

            return rootFrame;
        }

        /// <summary>
        /// Invoked when Navigation to a certain page fails
        /// </summary>
        /// <param name="sender">The Frame which failed navigation</param>
        /// <param name="e">Details about the navigation failure</param>
        void OnNavigationFailed(object sender, NavigationFailedEventArgs e)
        {
            var exception = new Exception($"Failed to load Page: {e.SourcePageType.FullName} Exception: {e.Exception.Message}");
            LoggingService.LogException(exception);
            Analytics.TrackEvent("FailedToLoadPage", new Dictionary<string, string>()
            {
                { "Page", e.SourcePageType.FullName },
                { "Exception", e.Exception.Message }
            });
            throw exception;
        }

        /// <summary>
        /// Invoked when application execution is being suspended.  Application state is saved
        /// without knowing whether the application will be terminated or resumed with the contents
        /// of memory still intact.
        /// </summary>
        /// <param name="sender">The source of the suspend request.</param>
        /// <param name="e">Details about the suspend request.</param>
        private void OnSuspending(object sender, SuspendingEventArgs e)
        {
            var deferral = e.SuspendingOperation.GetDeferral();
            //TODO: Save application state and stop any background activity
            deferral.Complete();
        }

        private void ExtendAcrylicIntoTitleBar()
        {
            if (!IsGameBarWidget)
            {
                CoreApplication.GetCurrentView().TitleBar.ExtendViewIntoTitleBar = true;
                ApplicationViewTitleBar titleBar = ApplicationView.GetForCurrentView().TitleBar;
                titleBar.ButtonBackgroundColor = Colors.Transparent;
                titleBar.ButtonInactiveBackgroundColor = Colors.Transparent;
            }
        }

        //private static void UpdateAppVersion()
        //{
        //    var packageVer = Package.Current.Id.Version;
        //    string oldVer = ApplicationSettingsStore.Read(SettingsKey.AppVersionStr) as string ?? "";
        //    string currentVer = $"{packageVer.Major}.{packageVer.Minor}.{packageVer.Build}.{packageVer.Revision}";

        //    if (currentVer != oldVer)
        //    {
        //        JumpListService.IsJumpListOutOfDate = true;
        //        ApplicationSettingsStore.Write(SettingsKey.AppVersionStr, currentVer);
        //    }
        //}

        //private static async Task UpdateJumpList()
        //{
        //    if (JumpListService.IsJumpListOutOfDate)
        //    {
        //        if (await JumpListService.UpdateJumpList())
        //        {
        //            JumpListService.IsJumpListOutOfDate = false;
        //        }
        //    }
        //}
    }
}