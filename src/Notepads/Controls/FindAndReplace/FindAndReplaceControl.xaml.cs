namespace Notepads.Controls.FindAndReplace
{
    using System;
    using Notepads.Services;
    using Windows.System;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Input;
    using Windows.UI.Xaml.Media;

    public sealed partial class FindAndReplaceControl : UserControl
    {
        public event EventHandler<RoutedEventArgs> OnDismissKeyDown;

        public event EventHandler<FindAndReplaceEventArgs> OnFindAndReplaceButtonClicked;

        public FindAndReplaceControl()
        {
            InitializeComponent();
            SetSelectionHighlightColor();

            Loaded += FindAndReplaceControl_Loaded;
            Unloaded += FindAndReplaceControl_Unloaded;
        }

        public void Dispose()
        {
            ThemeSettingsService.OnAccentColorChanged -= ThemeSettingsService_OnAccentColorChanged;
        }

        private void FindAndReplaceControl_Loaded(object sender, RoutedEventArgs e)
        {
            Focus();
            ThemeSettingsService.OnAccentColorChanged += ThemeSettingsService_OnAccentColorChanged;
        }

        private void FindAndReplaceControl_Unloaded(object sender, RoutedEventArgs e)
        {
            ThemeSettingsService.OnAccentColorChanged -= ThemeSettingsService_OnAccentColorChanged;
        }

        private void ThemeSettingsService_OnAccentColorChanged(object sender, Windows.UI.Color e)
        {
            SetSelectionHighlightColor();
        }

        public double GetHeight(bool showReplaceBar)
        {
            if (showReplaceBar)
            {
                return FindAndReplaceRootGrid.Height + ReplaceBarPlaceHolder.Height;
            }
            else
            {
                return FindAndReplaceRootGrid.Height;
            }
        }

        private void SetSelectionHighlightColor()
        {
            FindBar.SelectionHighlightColor =
                Application.Current.Resources["SystemControlForegroundAccentBrush"] as SolidColorBrush;
            FindBar.SelectionHighlightColorWhenNotFocused =
                Application.Current.Resources["SystemControlForegroundAccentBrush"] as SolidColorBrush;
            FindBar.SelectionHighlightColor =
                Application.Current.Resources["SystemControlForegroundAccentBrush"] as SolidColorBrush;
            FindBar.SelectionHighlightColorWhenNotFocused =
                Application.Current.Resources["SystemControlForegroundAccentBrush"] as SolidColorBrush;
        }

        public void Focus()
        {
            FindBar.Focus(FocusState.Programmatic);
        }

        public void ShowReplaceBar(bool showReplaceBar)
        {
            ReplaceBarPlaceHolder.Visibility = showReplaceBar ? Visibility.Visible : Visibility.Collapsed;
        }

        private void DismissButton_OnClick(object sender, RoutedEventArgs e)
        {
            OnDismissKeyDown?.Invoke(sender, e);
        }

        private void FindBar_OnTextChanged(object sender, TextChangedEventArgs e)
        {
            SearchButton.Visibility = !string.IsNullOrEmpty(FindBar.Text) ? Visibility.Visible : Visibility.Collapsed;

            if (!string.IsNullOrEmpty(FindBar.Text))
            {
                ReplaceButton.IsEnabled = true;
                ReplaceAllButton.IsEnabled = true;
            }
            else
            {
                ReplaceButton.IsEnabled = false;
                ReplaceAllButton.IsEnabled = false;
            }
        }

        private void SearchButton_OnClick(object sender, RoutedEventArgs e)
        {
            if (sender is MenuFlyout) return;

            OnFindAndReplaceButtonClicked?.Invoke(sender, new FindAndReplaceEventArgs(FindBar.Text, null, MatchCaseToggle.IsChecked, MatchWholeWordToggle.IsChecked, UseRegexToggle.IsChecked, FindAndReplaceMode.FindOnly));
        }

        private void FindBar_OnKeyDown(object sender, KeyRoutedEventArgs e)
        {
            if (e.Key == VirtualKey.Enter && !string.IsNullOrEmpty(FindBar.Text))
            {
                SearchButton_OnClick(sender, e);
            }

            if (e.Key == VirtualKey.Tab)
            {
                e.Handled = true;
                if (ReplaceBarPlaceHolder.Visibility == Visibility.Visible) ReplaceBar.Focus(FocusState.Programmatic);
            }
        }

        private void FindBar_GotFocus(object sender, RoutedEventArgs e)
        {
            ReplaceBar.SelectionStart = ReplaceBar.Text.Length;
            FindBar.SelectionStart = 0;
            FindBar.SelectionLength = FindBar.Text.Length;
        }

        private void ReplaceBar_OnKeyDown(object sender, KeyRoutedEventArgs e)
        {
            if (e.Key == VirtualKey.Tab)
            {
                e.Handled = true;
                if (ReplaceBarPlaceHolder.Visibility == Visibility.Visible) FindBar.Focus(FocusState.Programmatic);
            }
        }

        private void ReplaceBar_GotFocus(object sender, RoutedEventArgs e)
        {
            FindBar.SelectionStart = FindBar.Text.Length;
            ReplaceBar.SelectionStart = 0;
            ReplaceBar.SelectionLength = ReplaceBar.Text.Length;
        }

        private void ReplaceBar_OnTextChanged(object sender, TextChangedEventArgs e)
        {
        }

        private void ReplaceButton_OnClick(object sender, RoutedEventArgs e)
        {
            OnFindAndReplaceButtonClicked?.Invoke(sender, new FindAndReplaceEventArgs(FindBar.Text, ReplaceBar.Text, MatchCaseToggle.IsChecked, MatchWholeWordToggle.IsChecked, UseRegexToggle.IsChecked, FindAndReplaceMode.Replace));
        }

        private void ReplaceAllButton_OnClick(object sender, RoutedEventArgs e)
        {
            OnFindAndReplaceButtonClicked?.Invoke(sender, new FindAndReplaceEventArgs(FindBar.Text, ReplaceBar.Text, MatchCaseToggle.IsChecked, MatchWholeWordToggle.IsChecked, UseRegexToggle.IsChecked, FindAndReplaceMode.ReplaceAll));
        }

        private void OptionButtonFlyoutItem_OnClick(object sender, RoutedEventArgs e)
        {
            MatchWholeWordToggle.IsEnabled = !UseRegexToggle.IsChecked;
            UseRegexToggle.IsEnabled = !MatchWholeWordToggle.IsChecked;
        }
    }
}