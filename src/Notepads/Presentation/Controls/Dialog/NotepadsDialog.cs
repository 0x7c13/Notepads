// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using Notepads.Presentation.Theming;
using Windows.ApplicationModel.Resources;
using Windows.UI;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;

namespace Notepads.Presentation.Controls.Dialog;

public partial class NotepadsDialog : ContentDialog
{
    public bool IsAborted = false;

    // The background comes from ContentDialogBackground in App.xaml theme resources.
    public NotepadsDialog()
    {
        RequestedTheme = ThemeSettingsService.ThemeMode;
        CornerRadius = new CornerRadius(8);
        PrimaryButtonStyle = CreateRoundedButtonStyle();
        SecondaryButtonStyle = CreateRoundedButtonStyle();
        CloseButtonStyle = CreateRoundedButtonStyle();
    }

    internal readonly ResourceLoader ResourceLoader = ResourceLoader.GetForCurrentView();

    private static Style CreateRoundedButtonStyle()
    {
        var style = new Style(typeof(Button));
        style.Setters.Add(new Setter(Control.CornerRadiusProperty, new CornerRadius(4)));
        return style;
    }

    internal static Style GetButtonStyle(Color backgroundColor)
    {
        var buttonStyle = new Windows.UI.Xaml.Style(typeof(Button));
        buttonStyle.Setters.Add(new Setter(Control.BackgroundProperty, backgroundColor));
        buttonStyle.Setters.Add(new Setter(Control.ForegroundProperty, Colors.White));
        buttonStyle.Setters.Add(new Setter(Control.CornerRadiusProperty, new CornerRadius(4)));
        return buttonStyle;
    }
}
