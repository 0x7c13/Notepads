// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Notepads.Presentation.Controls.FindAndReplace;
using Notepads.Presentation.Controls.TextEditor;
using Notepads.Presentation.Theming;
using Notepads.Presentation.Views.Settings;
using Windows.UI;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Controls.Primitives;
using Windows.UI.Xaml.Media;
using Windows.UI.Xaml.Shapes;
using static NotepadsIntegrationTests.FeatureUI;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal static class AccentColorFeatureTests
{
    internal static async Task RunAsync(FeatureTestHost host, TextEditor editor, StringBuilder log)
    {
        var red = Color.FromArgb(255, 199, 40, 59);
        var green = Color.FromArgb(255, 40, 150, 90);
        var contrast = (ResourceDictionary)host.Page.Resources.ThemeDictionaries["HighContrast"];
        var keys = new[] { "ComboBoxItemPillFillBrush", "ListViewItemSelectionIndicatorBrush",
            "ListViewItemSelectionIndicatorPointerOverBrush", "ListViewItemSelectionIndicatorPressedBrush",
            "TextControlSelectionHighlightColor", "TextControlBorderBrushFocused" };
        var contrastColors = keys.Select(key => ((SolidColorBrush)contrast[key]).Color).ToArray();
        try
        {
            foreach (var theme in new[] { ElementTheme.Light, ElementTheme.Dark })
            {
                var settings = await SettingsAsync<PersonalizationSettingsPage>(host.Page, "Personalization");
                Named<RadioButton>(settings, theme == ElementTheme.Light ? "ThemeModeLightButton" : "ThemeModeDarkButton").IsChecked = true;
                await UntilAsync(() => host.Page.TestFeatureTheme == theme, "Accent controls use the requested theme.");
                Named<ToggleSwitch>(settings, "AccentColorToggle").IsOn = false;
                var picker = Named<ColorPicker>(settings, "AccentColorPicker");
                picker.Color = red;
                picker.UpdateLayout();
                var input = Descendants(picker).OfType<TextBox>().Single(control => control.Name == "RedTextBox");
                input.Focus(FocusState.Keyboard);
                await UntilAsync(() => input.FocusState != FocusState.Unfocused, "The real color channel input receives focus.");
                var border = Descendants(input).OfType<Border>().Single(control => control.Name == "BorderElement");
                try
                {
                    await UntilAsync(() => border.BorderBrush is LinearGradientBrush brush && brush.GradientStops[0].Color == red,
                        "The color picker's focused underline uses the custom accent in " + theme + ".");
                }
                catch
                {
                    log.AppendLine($"Accent input {theme}, focus={input.FocusState}, border={Describe(border.BorderBrush)}, " +
                        $"application={Describe((Brush)Application.Current.Resources["TextControlBorderBrushFocused"])}, " +
                        $"owned={Describe((Brush)((ResourceDictionary)host.Page.Resources.ThemeDictionaries[theme == ElementTheme.Light ? "Light" : "Default"])["TextControlBorderBrushFocused"])}");
                    throw;
                }
                Check((input.SelectionHighlightColor as SolidColorBrush)?.Color == red,
                    "Color channel text selection uses the custom accent.");
                var focusedBrush = border.BorderBrush;
                picker.Color = green;
                await UntilAsync(() => ((LinearGradientBrush)focusedBrush).GradientStops[0].Color == green,
                    "Changing the color updates the already-focused input brush.");
                Check(ReferenceEquals(focusedBrush, border.BorderBrush), "The focused input retains its live brush.");

                var representation = Descendants(picker).OfType<ComboBox>().Single(control => control.Name == "ColorRepresentationComboBox");
                await CheckComboAsync(representation, green, red, color => picker.Color = color);
                var advanced = await SettingsAsync<AdvancedSettingsPage>(host.Page, "Advanced");
                await CheckComboAsync(Named<ComboBox>(advanced, "LanguagePicker"), red, green,
                    color => ThemeSettingsService.AppAccentColor = color);
                await CloseSettingsAsync(host.Page);

                editor.ShowFindAndReplaceControl(false);
                var find = Named<FindAndReplaceControl>(editor, "FindAndReplaceControl");
                Named<TextBox>(find, "FindBar").Text = "selection";
                var command = Named<MenuFlyoutItem>(find, "FindAllButton");
                await UntilAsync(() => command.IsEnabled, "Find All is ready for the accent fixture.");
                Invoke(command);
                var list = Named<ListView>(editor, "FindAllList");
                await UntilAsync(() => list.ContainerFromIndex(0) is ListViewItem item && item.IsLoaded && item.IsSelected,
                    "The actual search result has a selected container.");
                var result = (ListViewItem)list.ContainerFromIndex(0);
                var presenter = Descendants(result).OfType<ListViewItemPresenter>().Single();
                await UntilAsync(() => (presenter.SelectionIndicatorBrush as SolidColorBrush)?.Color == green,
                    "Find All's selection marker uses the current accent.");
                ThemeSettingsService.AppAccentColor = red;
                await UntilAsync(() => (presenter.SelectionIndicatorBrush as SolidColorBrush)?.Color == red &&
                    (presenter.SelectionIndicatorPointerOverBrush as SolidColorBrush)?.Color == red &&
                    (presenter.SelectionIndicatorPressedBrush as SolidColorBrush)?.Color == red,
                    "An existing result updates selected, hovered and pressed marker colors.");
                ThemeSettingsService.UseWindowsAccentColor = true;
                await UntilAsync(() => (presenter.SelectionIndicatorBrush as SolidColorBrush)?.Color == ThemeSettingsService.AppAccentColor,
                    "Returning to the Windows accent updates the open result pane.");
                editor.HideFindAndReplaceControl();
                await UntilAsync(() => !editor.TestFindVisible, "The search panel finishes closing before changing themes.");
            }
            Check(keys.Select(key => ((SolidColorBrush)contrast[key]).Color).SequenceEqual(contrastColors),
                "Custom accent updates preserve the separate system high-contrast brushes.");
            log.AppendLine("PASS: Appearance.ControlAccents — Light/Dark color inputs, RGB/HSV and language popups, live result markers and Windows-accent return.");
        }
        finally { editor.HideFindAndReplaceControl(); }
    }

    private static async Task CheckComboAsync(ComboBox combo, Color initial, Color changed, Action<Color> changeColor)
    {
        combo.StartBringIntoView();
        combo.Focus(FocusState.Keyboard);
        await UntilAsync(() => combo.FocusState != FocusState.Unfocused,
            "The combo receives focus before opening its popup.");
        combo.IsDropDownOpen = true;
        try
        {
            await UntilAsync(() => combo.ContainerFromIndex(combo.SelectedIndex) is SelectorItem item && item.IsLoaded,
                "The real combo popup realizes its selected item.");
            var selected = (SelectorItem)combo.ContainerFromIndex(combo.SelectedIndex);
            var pill = Descendants(selected).OfType<Shape>().Single(shape => shape.Name == "Pill");
            await UntilAsync(() => (pill.Fill as SolidColorBrush)?.Color == initial,
                "The open combo's selection marker uses the current accent.");
            changeColor(changed);
            await UntilAsync(() => (pill.Fill as SolidColorBrush)?.Color == changed,
                "Changing the accent updates an already-open combo popup.");
        }
        catch (Exception error)
        {
            throw new InvalidOperationException($"Accent combo {combo.Name}: open={combo.IsDropDownOpen}, " +
                $"loaded={combo.IsLoaded}, selected={combo.SelectedIndex}, items={combo.Items.Count}, " +
                $"container={combo.ContainerFromIndex(combo.SelectedIndex)?.GetType().Name ?? "null"}, " +
                $"size={combo.ActualWidth}x{combo.ActualHeight}.", error);
        }
        finally { combo.IsDropDownOpen = false; }
    }

    private static string Describe(Brush brush) => brush switch
    {
        SolidColorBrush solid => solid.Color.ToString(),
        LinearGradientBrush gradient => string.Join(",", gradient.GradientStops.Select(stop => stop.Color.ToString())),
        _ => brush?.GetType().Name ?? "null"
    };
}
