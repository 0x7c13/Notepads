
namespace Notepads
{
    using Microsoft.AppCenter;
    using Microsoft.AppCenter.Analytics;
    using Microsoft.AppCenter.Crashes;
    using Notepads.Services;
    using Notepads.Utilities;
    using System;
    using System.Collections.Generic;
    using System.Threading.Tasks;
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
        /// <summary>
        /// Initializes the singleton application object.  This is the first line of authored code
        /// executed, and as such is the logical equivalent of main() or WinMain().
        /// </summary>
        public App()
        {
            UnhandledException += OnUnhandledException;
            TaskScheduler.UnobservedTaskException += OnUnobservedException;

            var services = new Type[] { typeof(Crashes), typeof(Analytics) };
            AppCenter.Start(null, services);

            InitializeComponent();
            Suspending += OnSuspending;
        }

        private static void OnUnhandledException(object sender, Windows.UI.Xaml.UnhandledExceptionEventArgs e)
        {
            // Occurs when an exception is not handled on the UI thread.

            Analytics.TrackEvent("OnUnhandledException", new Dictionary<string, string>() {
                {
                    "Message", e.Message
                },
                {
                    "Exception", e.Exception.ToString()
                }
            });

            // if you want to suppress and handle it manually, 
            // otherwise app shuts down.
            e.Handled = true;
        }

        private static void OnUnobservedException(object sender, UnobservedTaskExceptionEventArgs e)
        {
            // Occurs when an exception is not handled on a background thread.
            // ie. A task is fired and forgotten Task.Run(() => {...})

            Analytics.TrackEvent("OnUnobservedException", new Dictionary<string, string>() {
                {
                    "Message", e.Exception.Message
                },
                {
                    "Exception", e.Exception.ToString()
                }
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
            if (!(Window.Current.Content is Frame rootFrame))
            {
                rootFrame = CreateRootFrame(e);
                Window.Current.Content = rootFrame;
            }

            ThemeSettingsService.Initialize();
            EditorSettingsService.Initialize();

            Analytics.TrackEvent("AppLaunch_Settings", new Dictionary<string, string>() {
                {
                    "UseWindowsTheme", ThemeSettingsService.UseWindowsTheme.ToString()
                },
                {
                    "ThemeMode", ThemeSettingsService.ThemeMode.ToString()
                },
                {
                    "UseWindowsAccentColor", ThemeSettingsService.UseWindowsAccentColor.ToString()
                },
                {
                    "AppBackgroundTintOpacity", $"{ThemeSettingsService.AppBackgroundPanelTintOpacity:0.0}"
                },
                {
                    "ShowStatusBar", EditorSettingsService.ShowStatusBar.ToString()
                },
                {
                    "EditorDefaultLineEnding", EditorSettingsService.EditorDefaultLineEnding.ToString()
                },
                {
                    "EditorDefaultEncoding", EncodingUtility.GetEncodingBodyName(EditorSettingsService.EditorDefaultEncoding)
                },
                {
                    "EditorDefaultTabIndents", EditorSettingsService.EditorDefaultTabIndents.ToString()
                },
                {
                    "EditorDefaultDecoding", EncodingUtility.GetEncodingBodyName(EditorSettingsService.EditorDefaultDecoding)
                },
            });

            await ActivationService.ActivateAsync(rootFrame, e);
            //await LoggingService.InitializeAsync();

            Window.Current.Activate();
            ExtendAcrylicIntoTitleBar();
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
            throw new Exception("Failed to load Page " + e.SourcePageType.FullName);
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
            CoreApplication.GetCurrentView().TitleBar.ExtendViewIntoTitleBar = true;
            ApplicationViewTitleBar titleBar = ApplicationView.GetForCurrentView().TitleBar;
            titleBar.ButtonBackgroundColor = Colors.Transparent;
            titleBar.ButtonInactiveBackgroundColor = Colors.Transparent;
        }
    }
}
