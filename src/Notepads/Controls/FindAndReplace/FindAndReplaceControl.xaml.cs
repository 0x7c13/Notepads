﻿namespace Notepads.Controls.FindAndReplace
{
    using System;
    using Notepads.Services;
    using Windows.System;
    using Windows.UI.Core;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Input;
    using Windows.UI.Xaml.Media;

    public sealed partial class FindAndReplaceControl : UserControl
    {
        public event EventHandler<RoutedEventArgs> OnDismissKeyDown;

        public event EventHandler<FindAndReplaceEventArgs> OnFindAndReplaceButtonClicked;

        public event EventHandler<bool> OnToggleReplaceModeButtonClicked;

        public event EventHandler<KeyRoutedEventArgs> OnFindReplaceControlKeyDown;

        //When enter key is pressed focus is returned to control
        //This variable is used to remove flicker in text selection
        private bool _enterPressed = false;

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
                return FindBarPlaceHolder.Height + ReplaceBarPlaceHolder.Height;
            }
            else
            {
                return FindBarPlaceHolder.Height;
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
            if(mode == FindAndReplaceMode.FindOnly)
                FindBar.Focus(FocusState.Programmatic);
            else
                ReplaceBar.Focus(FocusState.Programmatic);
        }

        public void ShowReplaceBar(bool showReplaceBar)
        {
            if (showReplaceBar)
            {
                ToggleReplaceModeButtonGrid.SetValue(Grid.RowSpanProperty, 2);
                ToggleReplaceModeButton.Content = new FontIcon { Glyph = "\xE011" };
                ReplaceBarPlaceHolder.Visibility = Visibility.Visible;
                if (!string.IsNullOrEmpty(FindBar.Text))
                {
                    ReplaceButton.IsEnabled = true;
                    ReplaceAllButton.IsEnabled = true;
                }
            }
            else
            {
                ToggleReplaceModeButtonGrid.SetValue(Grid.RowSpanProperty, 1);
                ToggleReplaceModeButton.Content = new FontIcon { Glyph = "\xE00F" };
                ReplaceBarPlaceHolder.Visibility = Visibility.Collapsed;
                ReplaceButton.IsEnabled = false;
                ReplaceAllButton.IsEnabled = false;
            }
        }

        private void DismissButton_OnClick(object sender, RoutedEventArgs e)
        {
            OnDismissKeyDown?.Invoke(sender, e);
        }

        private void FindBar_OnTextChanged(object sender, TextChangedEventArgs e)
        {
            if (!string.IsNullOrEmpty(FindBar.Text))
            {
                SearchForwardButton.IsEnabled = true;
                SearchBackwardButton.IsEnabled = true;
                if (ReplaceBarPlaceHolder.Visibility == Visibility.Visible)
                {
                    ReplaceButton.IsEnabled = true;
                    ReplaceAllButton.IsEnabled = true;
                }
            }
            else
            {
                SearchForwardButton.IsEnabled = false;
                SearchBackwardButton.IsEnabled = false;
                ReplaceButton.IsEnabled = false;
                ReplaceAllButton.IsEnabled = false;
            }
        }

        private void SearchForwardButton_OnClick(object sender, RoutedEventArgs e)
        {
            if (sender is MenuFlyout) return;

            OnFindAndReplaceButtonClicked?.Invoke(sender, new FindAndReplaceEventArgs(FindBar.Text, null, MatchCaseToggle.IsChecked, MatchWholeWordToggle.IsChecked, UseRegexToggle.IsChecked, FindAndReplaceMode.FindOnly, SearchDirection.Next));
        }

        private void SearchBackwardButton_OnClick(object sender, RoutedEventArgs e)
        {
            if (sender is MenuFlyout) return;

            OnFindAndReplaceButtonClicked?.Invoke(sender, new FindAndReplaceEventArgs(FindBar.Text, null, MatchCaseToggle.IsChecked, MatchWholeWordToggle.IsChecked, UseRegexToggle.IsChecked, FindAndReplaceMode.FindOnly, SearchDirection.Previous));
        }

        private void FindBar_OnKeyDown(object sender, KeyRoutedEventArgs e)
        {
            if (e.Key == VirtualKey.Enter && !string.IsNullOrEmpty(FindBar.Text))
            {
                _enterPressed = true;
                SearchForwardButton_OnClick(sender, e);
            }

            if (e.Key == VirtualKey.Tab)
            {
                e.Handled = true;
                if (ReplaceBarPlaceHolder.Visibility == Visibility.Visible) ReplaceBar.Focus(FocusState.Programmatic);
            }
        }

        private void FindBar_GotFocus(object sender, RoutedEventArgs e)
        {
            _enterPressed = false;
            FindBar.SelectionStart = 0;
            FindBar.SelectionLength = FindBar.Text.Length;
        }

        private void FindBar_LostFocus(object sender, RoutedEventArgs e)
        {
            if (_enterPressed) return;
            FindBar.SelectionStart = FindBar.Text.Length;
        }

        private void ReplaceBar_OnKeyDown(object sender, KeyRoutedEventArgs e)
        {
            if (e.Key == VirtualKey.Enter && !string.IsNullOrEmpty(FindBar.Text))
            {
                _enterPressed = true;
                ReplaceButton_OnClick(sender, e);
            }

            if (e.Key == VirtualKey.Tab)
            {
                e.Handled = true;
                if (ReplaceBarPlaceHolder.Visibility == Visibility.Visible) FindBar.Focus(FocusState.Programmatic);
            }
        }

        private void ReplaceBar_GotFocus(object sender, RoutedEventArgs e)
        {
            _enterPressed = false;
            ReplaceBar.SelectionStart = 0;
            ReplaceBar.SelectionLength = ReplaceBar.Text.Length;
        }

        private void ReplaceBar_LostFocus(object sender, RoutedEventArgs e)
        {
            if (_enterPressed) return;
            ReplaceBar.SelectionStart = ReplaceBar.Text.Length;
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

        private void RootGrid_KeyDown(object sender, KeyRoutedEventArgs e)
        {
            var ctrl = Window.Current.CoreWindow.GetKeyState(VirtualKey.Control);
            var alt = Window.Current.CoreWindow.GetKeyState(VirtualKey.Menu);
            var shift = Window.Current.CoreWindow.GetKeyState(VirtualKey.Shift);

            if (!(!ctrl.HasFlag(CoreVirtualKeyStates.Down) && !alt.HasFlag(CoreVirtualKeyStates.Down) && !shift.HasFlag(CoreVirtualKeyStates.Down) && e.Key == VirtualKey.Escape) &&
                !(!ctrl.HasFlag(CoreVirtualKeyStates.Down) && !alt.HasFlag(CoreVirtualKeyStates.Down) && !shift.HasFlag(CoreVirtualKeyStates.Down) && e.Key == VirtualKey.F3) &&
                !(!ctrl.HasFlag(CoreVirtualKeyStates.Down) && !alt.HasFlag(CoreVirtualKeyStates.Down) && shift.HasFlag(CoreVirtualKeyStates.Down) && e.Key == VirtualKey.F3) &&
                !(!ctrl.HasFlag(CoreVirtualKeyStates.Down) && alt.HasFlag(CoreVirtualKeyStates.Down) && !shift.HasFlag(CoreVirtualKeyStates.Down) && (e.Key == VirtualKey.A || e.Key == VirtualKey.E || e.Key == VirtualKey.R || e.Key == VirtualKey.W)) &&
                !(ctrl.HasFlag(CoreVirtualKeyStates.Down) && alt.HasFlag(CoreVirtualKeyStates.Down) && !shift.HasFlag(CoreVirtualKeyStates.Down) && e.Key == VirtualKey.Enter) &&
                !e.Handled)
            {
                OnFindReplaceControlKeyDown?.Invoke(sender, e);
            }
        }

        private void ToggleReplaceModeButton_OnClick(object sender, RoutedEventArgs e)
        {
            OnToggleReplaceModeButtonClicked?.Invoke(sender, ReplaceBarPlaceHolder.Visibility == Visibility.Collapsed ? true : false);
        }
    }
}