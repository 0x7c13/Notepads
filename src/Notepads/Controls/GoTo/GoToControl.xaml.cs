namespace Notepads.Controls.GoTo
{
    using System;
    using Notepads.Services;
    using Windows.System;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Input;
    using Windows.UI.Xaml.Media;

    public sealed partial class GoToControl : UserControl
    {
        public event EventHandler<RoutedEventArgs> OnDismissKeyDown;

        public event EventHandler<GoToEventArgs> OnGoToButtonClicked;

        public GoToControl()
        {
            InitializeComponent();

            SetSelectionHighlightColor();

            ThemeSettingsService.OnAccentColorChanged += (sender, color) =>
            {
                SetSelectionHighlightColor();
            };

            Loaded += (sender, args) => { Focus(); };
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
            if (!string.IsNullOrEmpty(GoToBar.Text))
            {
                GoToBar.SelectionStart = 0;
                GoToBar.SelectionLength = GoToBar.Text.Length;
            }
            GoToBar.Focus(FocusState.Programmatic);
        }

        private void GoToBar_onTextChanged(object sender, TextChangedEventArgs e)
        {
            SearchButton.Visibility = !string.IsNullOrEmpty(GoToBar.Text) ? Visibility.Visible : Visibility.Collapsed;
        }

        private void SearchButton_OnClick(object sender, RoutedEventArgs e)
        {
            if (sender is MenuFlyout) return;

            OnGoToButtonClicked?.Invoke(sender, new GoToEventArgs(GoToBar.Text));
        }

        private void GoToBar_onKeyDown(object sender, KeyRoutedEventArgs e)
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

        private void GoToBar_LostFocus(object sender, RoutedEventArgs e)
        {
            OnDismissKeyDown?.Invoke(sender, e);
        }
    }
}