// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2020-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using Windows.System;
using Windows.UI.Core;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Input;

namespace Notepads.Presentation.Controls.FindAndReplace;

public sealed partial class FindAndReplaceTextBox : TextBox
{
    protected override void OnApplyTemplate()
    {
        base.OnApplyTemplate();

        // The current WinUI TextBox template adds a clear button whenever
        // text is present. It overlaps our search-options button and also
        // consumes its space inside the input. Keep the platform text and
        // focus template, but collapse that one built-in affordance.
        if (GetTemplateChild("DeleteButton") is Button clearButton)
        {
            clearButton.MinWidth = 0;
            clearButton.MaxWidth = 0;
            clearButton.Width = 0;
            clearButton.Padding = new Thickness(0);
            clearButton.Margin = new Thickness(0);
            clearButton.IsHitTestVisible = false;
            clearButton.Visibility = Visibility.Collapsed;
        }
    }

    protected override void OnKeyDown(KeyRoutedEventArgs e)
    {
        CoreVirtualKeyStates ctrl = Window.Current.CoreWindow.GetKeyState(VirtualKey.Control);
        CoreVirtualKeyStates alt = Window.Current.CoreWindow.GetKeyState(VirtualKey.Menu);
        CoreVirtualKeyStates shift = Window.Current.CoreWindow.GetKeyState(VirtualKey.Shift);

        // By default, TextBox toggles case when user hit "Shift + F3"
        // This should be restricted
        if (!ctrl.HasFlag(CoreVirtualKeyStates.Down) &&
            !alt.HasFlag(CoreVirtualKeyStates.Down) &&
            shift.HasFlag(CoreVirtualKeyStates.Down)
            && e.Key == VirtualKey.F3)
        {
            return;
        }

        if (!e.Handled)
        {
            base.OnKeyDown(e);
        }
    }
}
