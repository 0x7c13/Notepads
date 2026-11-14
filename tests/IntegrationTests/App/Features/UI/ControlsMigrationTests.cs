// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Linq;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Text;
using System.Threading.Tasks;
using Notepads.Controls;
using Windows.Foundation;
using Windows.Graphics.Imaging;
using Windows.Storage;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Media;
using Windows.UI.Xaml.Media.Imaging;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal static class ControlsMigrationTests
{
    internal static async Task RunAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var unlocated = new InAppNotification
        {
            Style = (Style)Application.Current.Resources["InAppNotificationNoDismissButton"]
        };
        unlocated.ClearValue(Control.DefaultStyleResourceUriProperty);
        unlocated.ApplyTemplate();
        unlocated.Measure(new Size(320, 160));
        log.AppendLine($"OBS: Controls without relocated theme URI: background={unlocated.Background?.GetType().Name ?? "null"}, padding={unlocated.Padding}, margin={unlocated.Margin}, font={unlocated.FontSize}, alignment={unlocated.VerticalAlignment}.");
        foreach (var theme in new[] { ElementTheme.Dark, ElementTheme.Light })
        {
            host.Page.RequestedTheme = theme;
            host.Page.PostNotification("File already opened!", 0);
            var notification = FeatureUI.Named<InAppNotification>(host.Page, "StatusNotification");
            await UntilAsync(() => notification.IsLoaded && notification.ActualHeight > 0,
                "The production status notification is visible.");
            log.AppendLine($"OBS: Controls {theme} notification: background={notification.Background?.GetType().Name ?? "null"}, border={notification.BorderBrush?.GetType().Name ?? "null"}, padding={notification.Padding}, margin={notification.Margin}, font={notification.FontSize}, alignment={notification.VerticalAlignment}.");
            Check(notification.Background != null && notification.BorderBrush != null &&
                notification.Foreground != null,
                "An explicit notification template retains default themed chrome in " + theme + ".");
            Check(notification.Padding == new Thickness(24, 12, 24, 12) &&
                notification.Margin == new Thickness(24, 12, 24, 12) &&
                notification.MinHeight == 55 && notification.FontSize == 16 &&
                notification.VerticalAlignment == VerticalAlignment.Bottom && notification.MaxWidth == 960,
                "The migrated notification retains its default spacing, type size and bottom alignment.");
            Check(notification.BorderThickness == new Thickness(1) &&
                notification.AnimationDuration == TimeSpan.FromMilliseconds(80) && !notification.ShowDismissButton,
                "The application overrides still take precedence over default theme setters.");
            Check(Descendants(notification).OfType<Grid>().Any(grid =>
                ReferenceEquals(grid.Background, notification.Background) && grid.Padding == notification.Padding &&
                grid.BorderThickness == notification.BorderThickness && grid.CornerRadius == new CornerRadius(4)),
                "The real notification template paints the inherited background, padding and rounded border.");
            await Task.Delay(notification.AnimationDuration + TimeSpan.FromMilliseconds(50));
            Check(notification.TransformToVisual(host.Page).TransformPoint(default).Y > host.Page.ActualHeight / 2,
                "The toast is laid out in the bottom half of the real page.");
            await SaveRasterAsync(host, "notification-" + theme + ".png", log);
            var root = Descendants(notification).OfType<Grid>().Single(grid => grid.Name == "RootGrid");
            var closed = 0;
            var kind = InAppNotificationDismissKind.Programmatic;
            void OnClosed(object sender, InAppNotificationClosedEventArgs args)
            {
                closed++;
                kind = args.DismissKind;
            }
            notification.Closed += OnClosed;
            try
            {
                notification.Dismiss();
                await UntilAsync(() => closed == 1 && root.Visibility == Visibility.Collapsed,
                    "The styled notification completes its close event and template animation.");
                Check(kind == InAppNotificationDismissKind.Programmatic, "The explicit dismissal retains its reason.");
                host.Page.PostNotification("Timed notification", 150);
                await UntilAsync(() => closed == 2 && root.Visibility == Visibility.Collapsed,
                    "A reopened notification still dismisses on its real timer.");
                Check(kind == InAppNotificationDismissKind.Timeout, "The timer retains its dismissal reason.");
            }
            finally { notification.Closed -= OnClosed; }
        }

        // Explicit application styles must compose with each control's theme
        // style, just as they did when the controls lived in their own assembly.
        var parent = host.Page.Content as Panel;
        Check(parent != null, "The production page exposes the controls' visual host.");
        var probeLayer = new Grid { Width = 320, Height = 160, IsHitTestVisible = false };
        parent.Children.Add(probeLayer);
        try
        {
            foreach (var (control, part) in new (Control Control, string Part)[]
            {
                (new DropShadowPanel(), "ShadowElement"),
                (new GridSplitter(), "RootGrid"),
                (new InAppNotification(), "PART_DismissButton"),
                (new SetsView(), "SetsContentPresenter"),
                (new SetsViewItem(), "LayoutRoot")
            })
            {
                control.Style = new Style { TargetType = control.GetType() };
                control.Visibility = Visibility.Visible;
                probeLayer.Children.Add(control);
                await UntilAsync(() => control.IsLoaded, "The migrated template probe is loaded.");
                control.ApplyTemplate();
                parent.UpdateLayout();
                Check(Descendants(control).OfType<FrameworkElement>().Any(element => element.Name == part),
                    "An explicit style preserves the default template for migrated " + control.GetType().Name + ".");
                probeLayer.Children.Remove(control);
            }
        }
        finally { parent.Children.Remove(probeLayer); }
        log.AppendLine("PASS: Controls.Migration — Dark/Light status chrome, spacing, position, local overrides, close/reopen/timer and all five migrated controls' default-template fallback.");
    }

    private static async Task SaveRasterAsync(FeatureTestHost host, string name, StringBuilder log)
    {
        var bitmap = new RenderTargetBitmap();
        await bitmap.RenderAsync(host.Page);
        var pixels = await bitmap.GetPixelsAsync();
        var file = await host.Folder.CreateFileAsync(name, CreationCollisionOption.FailIfExists);
        using var stream = await file.OpenAsync(FileAccessMode.ReadWrite);
        var encoder = await BitmapEncoder.CreateAsync(BitmapEncoder.PngEncoderId, stream);
        encoder.SetPixelData(BitmapPixelFormat.Bgra8, BitmapAlphaMode.Premultiplied,
            (uint)bitmap.PixelWidth, (uint)bitmap.PixelHeight, 96, 96, pixels.ToArray());
        await encoder.FlushAsync();
        log.AppendLine("OBS: Controls notification raster: " + file.Path);
    }
}
