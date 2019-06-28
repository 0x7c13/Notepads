
namespace Notepads.Controls.FindAndReplace
{
    using System;
    using Windows.System;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Input;
    using Windows.UI.Xaml.Media;
    using Notepads.EventArgs;
    using Notepads.Services;

    public sealed partial class FindAndReplaceControl : UserControl
    {
        public event EventHandler<RoutedEventArgs> OnDismissKeyDown;

        public event EventHandler<FindAndReplaceEventArgs> OnFindAndReplaceButtonClicked;

        public FindAndReplaceControl()
        {
            InitializeComponent();

            SetSelectionHighlightColor();

            ThemeSettingsService.OnAccentColorChanged += (sender, color) =>
            {
                SetSelectionHighlightColor();
            };
        }

        public double GetHeight(bool showReplaceBar)
        {
            if (showReplaceBar)
            {
                return SearchBarPlaceHolder.Height + ReplaceBarPlaceHolder.Height;
            }
            else
            {
                return SearchBarPlaceHolder.Height;
            }
        }

        private void SetSelectionHighlightColor()
        {
            SearchBar.SelectionHighlightColor =
                Application.Current.Resources["SystemControlForegroundAccentBrush"] as SolidColorBrush;
            SearchBar.SelectionHighlightColorWhenNotFocused =
                Application.Current.Resources["SystemControlForegroundAccentBrush"] as SolidColorBrush;
            ReplaceBar.SelectionHighlightColor =
                Application.Current.Resources["SystemControlForegroundAccentBrush"] as SolidColorBrush;
            ReplaceBar.SelectionHighlightColorWhenNotFocused =
                Application.Current.Resources["SystemControlForegroundAccentBrush"] as SolidColorBrush;
        }

        public void Focus()
        {
            SearchBar.Focus(FocusState.Programmatic);
        }

        public void ShowReplaceBar(bool showReplaceBar)
        {
            ReplaceBarPlaceHolder.Visibility = showReplaceBar ? Visibility.Visible : Visibility.Collapsed;
        }

        private void DismissButton_OnClick(object sender, RoutedEventArgs e)
        {
            OnDismissKeyDown?.Invoke(sender, e);
        }

        private void SearchBar_OnTextChanged(object sender, TextChangedEventArgs e)
        {
            SearchButton.Visibility = !string.IsNullOrEmpty(SearchBar.Text) ? Visibility.Visible : Visibility.Collapsed;

            if (!string.IsNullOrEmpty(SearchBar.Text))
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

            OnFindAndReplaceButtonClicked?.Invoke(sender, new FindAndReplaceEventArgs(SearchBar.Text, null, MatchCaseToggle.IsChecked, MatchWholeWordToggle.IsChecked, FindAndReplaceMode.FindOnly));
        }

        private void SearchBar_OnKeyDown(object sender, KeyRoutedEventArgs e)
        {
            if (e.Key == VirtualKey.Enter && !string.IsNullOrEmpty(SearchBar.Text))
            {
                SearchButton_OnClick(sender, e);
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
            OnFindAndReplaceButtonClicked?.Invoke(sender, new FindAndReplaceEventArgs(SearchBar.Text, ReplaceBar.Text, MatchCaseToggle.IsChecked, MatchWholeWordToggle.IsChecked, FindAndReplaceMode.Replace));
        }

        private void ReplaceAllButton_OnClick(object sender, RoutedEventArgs e)
        {
            OnFindAndReplaceButtonClicked?.Invoke(sender, new FindAndReplaceEventArgs(SearchBar.Text, ReplaceBar.Text, MatchCaseToggle.IsChecked, MatchWholeWordToggle.IsChecked, FindAndReplaceMode.ReplaceAll));
        }
    }
}
