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
            Focus(FindAndReplaceMode.FindOnly);
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

        public void Focus(FindAndReplaceMode mode)
        {
            if (mode == FindAndReplaceMode.FindOnly)
            {
                FindBar.Focus(FocusState.Programmatic);
            }
            else
            {
                if (!string.IsNullOrEmpty(FindBar.Text))
                {
                    FindBar.SelectionStart = FindBar.Text.Length;
                }
                if (!string.IsNullOrEmpty(ReplaceBar.Text))
                {
                    ReplaceBar.SelectionStart = ReplaceBar.Text.Length;
                }
                ReplaceBar.Focus(FocusState.Programmatic);
            }
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

            OnFindAndReplaceButtonClicked?.Invoke(sender, new FindAndReplaceEventArgs(FindBar.Text, null, MatchCaseToggle.IsChecked, MatchWholeWordToggle.IsChecked, FindAndReplaceMode.FindOnly));
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
            }
        }

        private void ReplaceBar_OnKeyDown(object sender, KeyRoutedEventArgs e)
        {
        }

        private void ReplaceBar_OnTextChanged(object sender, TextChangedEventArgs e)
        {
        }

        private void ReplaceButton_OnClick(object sender, RoutedEventArgs e)
        {
            OnFindAndReplaceButtonClicked?.Invoke(sender, new FindAndReplaceEventArgs(FindBar.Text, ReplaceBar.Text, MatchCaseToggle.IsChecked, MatchWholeWordToggle.IsChecked, FindAndReplaceMode.Replace));
        }

        private void ReplaceAllButton_OnClick(object sender, RoutedEventArgs e)
        {
            OnFindAndReplaceButtonClicked?.Invoke(sender, new FindAndReplaceEventArgs(FindBar.Text, ReplaceBar.Text, MatchCaseToggle.IsChecked, MatchWholeWordToggle.IsChecked, FindAndReplaceMode.ReplaceAll));
        }
    }
}