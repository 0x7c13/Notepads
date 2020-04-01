namespace Notepads.Controls.GoTo
{
    using System;
    using Notepads.Services;
    using Windows.ApplicationModel.Resources;
    using Windows.System;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Input;
    using Windows.UI.Xaml.Media;

    public sealed partial class GoToControl : UserControl
    {
        public event EventHandler<RoutedEventArgs> OnDismissKeyDown;
        public event EventHandler<GoToEventArgs> OnGoToButtonClicked;

        public event EventHandler<KeyRoutedEventArgs> OnGoToControlKeyDown;

        private int _currentLine;
        private int _maxLine;
        private readonly ResourceLoader _resourceLoader = ResourceLoader.GetForCurrentView();

        public void SetLineData(int currentLine, int maxLine)
        {
            _currentLine = currentLine;
            _maxLine = maxLine;
        }

        public GoToControl()
        {
            InitializeComponent();

            SetSelectionHighlightColor();

            Loaded += GoToControl_Loaded;
            Unloaded += GoToControl_Unloaded;
        }

        public void Dispose()
        {
            ThemeSettingsService.OnAccentColorChanged -= ThemeSettingsService_OnAccentColorChanged;
        }

        private void GoToControl_Loaded(object sender, RoutedEventArgs e)
        {
            Focus();
            ThemeSettingsService.OnAccentColorChanged += ThemeSettingsService_OnAccentColorChanged;
        }

        private void GoToControl_Unloaded(object sender, RoutedEventArgs e)
        {
            ThemeSettingsService.OnAccentColorChanged -= ThemeSettingsService_OnAccentColorChanged;
        }

        private void ThemeSettingsService_OnAccentColorChanged(object sender, Windows.UI.Color e)
        {
            SetSelectionHighlightColor();
        }

        public double GetHeight()
        {
            return GoToRootGrid.Height;
        }

        private void SetSelectionHighlightColor()
        {
            GoToBar.SelectionHighlightColor =
                Application.Current.Resources["SystemControlForegroundAccentBrush"] as SolidColorBrush;
            GoToBar.SelectionHighlightColorWhenNotFocused =
                Application.Current.Resources["SystemControlForegroundAccentBrush"] as SolidColorBrush;
        }

        public void Focus()
        {
            GoToBar.Text = _currentLine.ToString();
            GoToBar.Focus(FocusState.Programmatic);
        }

        private void GoToBar_OnTextChanged(object sender, TextChangedEventArgs e)
        {
            SearchButton.Visibility = !string.IsNullOrEmpty(GoToBar.Text) ? Visibility.Visible : Visibility.Collapsed;
        }

        private void SearchButton_OnClick(object sender, RoutedEventArgs e)
        {
            if (sender is MenuFlyout) return;

            OnGoToButtonClicked?.Invoke(sender, new GoToEventArgs(GoToBar.Text));
        }

        private void GoToBar_OnKeyDown(object sender, KeyRoutedEventArgs e)
        {
            if (e.Key == VirtualKey.Enter && !string.IsNullOrEmpty(GoToBar.Text))
            {
                SearchButton_OnClick(sender, e);
            }

            if (e.Key == VirtualKey.Tab)
            {
                e.Handled = true;
            }
        }

        private void GoToBar_GotFocus(object sender, RoutedEventArgs e)
        {
            GoToBar.SelectionStart = 0;
            GoToBar.SelectionLength = GoToBar.Text.Length;
        }

        private void GoToBar_LostFocus(object sender, RoutedEventArgs e)
        {
            GoToBar.SelectionStart = GoToBar.Text.Length;
        }

        private void DismissButton_OnClick(object sender, RoutedEventArgs e)
        {
            OnDismissKeyDown?.Invoke(sender, e);
        }

        private void GoToBar_BeforeTextChanging(TextBox sender, TextBoxBeforeTextChangingEventArgs args)
        {
            if (string.IsNullOrEmpty(args.NewText)) return;

            if (!int.TryParse(args.NewText, out var line) || args.NewText.Contains(" "))
            {
                NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("GoTo_NotificationMsg_InputError_InvalidInput"), 1500);
                args.Cancel = true;
            }
            else if (line > _maxLine || line <= 0)
            {
                NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("GoTo_NotificationMsg_InputError_ExceedInputLimit"), 1500);
                args.Cancel = true;
            }
        }

        private void GoToRootGrid_KeyDown(object sender, KeyRoutedEventArgs e)
        {
            if (!e.Handled)
            {
                OnGoToControlKeyDown?.Invoke(sender, e);
            }
        }
    }
}
