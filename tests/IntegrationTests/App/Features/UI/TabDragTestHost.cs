// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using CommunityToolkit.WinUI;
using Notepads.Controls;
using Notepads.Presentation.Controls.TextEditor;
using Notepads.Presentation.Workspace;
using Windows.ApplicationModel.DataTransfer;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Input;
using Windows.UI.Xaml.Media;
using Windows.UI.ViewManagement;
using Windows.UI;

namespace NotepadsIntegrationTests;

// Real production workspace; the desktop client supplies all pointer input and
// obtains header geometry independently through UIA. No synthetic drag events.
internal static class TabDragTestHost
{
    private static FeatureTestHost _host;
    private static SetsView _sets;
    private static WorkspaceTab[] _tabs;
    private static readonly List<TextEditorTransfer> Transfers = [];
    private static readonly Dictionary<Guid, ulong> Documents = [];
    private static int _starts, _completions, _nativeStarts, _selections, _unloads;
    private static int _presses, _releases, _pressModifiers, _pressedTab, _interactions;
    private static string _lastResult;
    private static TaskCompletionSource _dropGate;
    private static bool _dropHeld;
    private static bool _animations;

    internal static async Task<string> StartAsync(int count, string mode)
    {
        if (count is < 2 or > 40 || mode is not ("equal" or "unequal" or "unequal-tail" or "rtl" or "light" or "adaptive"))
            throw new ArgumentException("Expected 2–40 tabs and equal/unequal/unequal-tail/rtl/light/adaptive mode.");
        await DisposeAsync();
        _host = await FeatureTestHost.CreateAsync();
        FeatureUI.Named<Grid>(_host.Page, "RootGrid").RequestedTheme = mode == "light" ? ElementTheme.Light : ElementTheme.Dark;
        _sets = FeatureUI.Named<SetsView>(_host.Page, "Sets");
        _starts = _completions = _nativeStarts = _selections = _unloads = 0;
        _presses = _releases = _pressModifiers = _interactions = 0;
        _pressedTab = -1;
        _lastResult = "none";
        _animations = new UISettings().AnimationsEnabled;
        Transfers.Clear();
        Documents.Clear();
        while (_host.Core.GetTabs().Length < count) await FeatureUI.NewTabAsync(_host.Page);
        _tabs = _host.Core.GetTabs();
        if (mode != "adaptive") _sets.SetsWidthBehavior = SetsWidthMode.Actual;
        _sets.FlowDirection = mode == "rtl" ? FlowDirection.RightToLeft : FlowDirection.LeftToRight;
        for (int index = 0; index < _tabs.Length; index++)
        {
            var tab = _tabs[index];
            _host.Core.SwitchTo(tab);
            ((TextEditor)tab.ReadyEditor).TypeText("Tab " + index.ToString("D2") + " Ω 😀");
            var item = _host.Page.TestContainer(tab);
            if (mode != "adaptive")
            {
                double width = mode is "unequal" or "unequal-tail" ? (index == (mode == "unequal" ? 0 : 2) ? 200 : 90) : 140;
                item.Width = item.MinWidth = item.MaxWidth = width;
            }
            AutomationProperties.SetAutomationId(item, "DragTab" + index);
            Documents[tab.Id] = Native(tab).DocPointer;
            ((FrameworkElement)tab.ReadyEditor).Unloaded += (_, _) => { if (ReferenceEquals(tab, _tabs[0])) _unloads++; };
        }
        _host.Core.SwitchTo(_tabs[0]);
        _sets.TabDragStarting += (_, _) =>
        {
            _starts++;
            Transfers.AddRange(((NotepadsCore)_host.Core).TestTabTransfers);
        };
        _sets.TabDragCompleted += (_, args) => { _completions++; _lastResult = args.Error?.ToString() ?? args.Result.ToString(); };
        _sets.DragItemsStarting += (_, _) => _nativeStarts++;
        _sets.SetInteractionCompleted += (_, _) => _interactions++;
        _sets.AddHandler(UIElement.PointerPressedEvent, new PointerEventHandler(ObservePress), true);
        _sets.AddHandler(UIElement.PointerReleasedEvent, new PointerEventHandler((_, _) => _releases++), true);
        _sets.Drop += HoldDropAsync;
        _host.Core.TabSelected += (_, _) => _selections++;
        _sets.ScrollTo(0);
        await FrameAsync();
        // Small overlays move through the real production template/animations.
        // The independent desktop client finds their pixels in actual DWM output;
        // TransformToVisual can report an unpresented intermediate layout.
        uint[] colors = [0x16E74D, 0xF521D3, 0x1DADF9];
        for (int index = 0; index < Math.Min(3, _tabs.Length); index++)
            ((Grid)_host.Page.TestContainer(_tabs[index]).FindDescendant("LayoutRoot")).Children.Add(new Border
            {
                Width = 6, Height = 6, HorizontalAlignment = HorizontalAlignment.Left,
                VerticalAlignment = VerticalAlignment.Center, Margin = new Thickness(4, 0, 0, 0),
                Background = new SolidColorBrush(Color.FromArgb(255, (byte)(colors[index] >> 16), (byte)(colors[index] >> 8), (byte)colors[index])),
                IsHitTestVisible = false
            });
        _unloads = 0; // Only the selected document is expected to remain attached during its reorder.
        return State();
    }

    private static WinUIEditor.Editor Native(WorkspaceTab tab) =>
        ((WinUIEditor.EditorBaseControl)((Grid)((TextEditor)tab.ReadyEditor).TestNativeCore.Content).Children[0]).Editor;

    private static void ObservePress(object sender, PointerRoutedEventArgs args)
    {
        for (var node = args.OriginalSource as DependencyObject; node != null; node = VisualTreeHelper.GetParent(node))
        {
            if (node is not SetsViewItem item) continue;
            _pressedTab = Array.IndexOf(_tabs, item.Tag as WorkspaceTab);
            _pressModifiers = (int)args.KeyModifiers;
            _presses++;
            return;
        }
    }

    internal static async Task FrameAsync()
    {
        var complete = new TaskCompletionSource();
        int frames = 0;
        void Rendering(object sender, object args) { if (++frames == 3) complete.TrySetResult(); }
        CompositionTarget.Rendering += Rendering;
        try { await complete.Task.WaitAsync(TimeSpan.FromSeconds(5)); }
        finally { CompositionTarget.Rendering -= Rendering; }
    }

    internal static string State()
    {
        if (_host == null) throw new InvalidOperationException("No tab workspace.");
        var tabs = _host.Core.GetTabs();
        // Transfer commit holds a native reader while publishing removal intent.
        // Observe that phase without violating the reader's protected API surface.
        bool committing = ((NotepadsCore)_host.Core).TestTabTransferCompleting;
        string documents = committing ? "Pending" : tabs.Where(tab => Documents.ContainsKey(tab.Id)).All(tab =>
            tab.ReadyEditor != null && Native(tab).DocPointer == Documents[tab.Id] &&
            ((TextEditor)tab.ReadyEditor).TestNativeCore.CanUndo).ToString();
        var source = _tabs.Length > 0 && tabs.Contains(_tabs[0]) ? _host.Page.TestContainer(_tabs[0]) : null;
        var background = source?.FindDescendant("HeaderDragBackground") as Border;
        var layoutRoot = source?.FindDescendant("LayoutRoot") as Grid;
        bool opaque = background?.Visibility == Visibility.Visible &&
            background.Background is SolidColorBrush brush && brush.Color.A == 255 && layoutRoot?.Opacity == 1;
        var presenter = _sets.FindDescendant("SetsItemsPresenter") as FrameworkElement;
        var layout = string.Join(";", tabs.Take(2).Select(tab =>
        {
            var item = _host.Page.TestContainer(tab);
            var rect = item.TransformToVisual(presenter).TransformBounds(new Windows.Foundation.Rect(0, 0, item.ActualWidth, item.ActualHeight));
            return FormattableString.Invariant($"{rect.Left},{rect.Width}");
        }));
        return "tabOrder=" + string.Join(",", tabs.Select(tab => Array.IndexOf(_tabs, tab))) +
            "\ntabSelected=" + Array.IndexOf(_tabs, _host.Core.GetSelectedTab()) +
            "\ntabStarts=" + _starts + "\ntabCompletions=" + _completions + "\ntabResult=" + _lastResult +
            "\ntabNativeStarts=" + _nativeStarts + "\ntabSelections=" + _selections + "\ntabUnloads=" + _unloads +
            "\ntabPresses=" + _presses + "\ntabReleases=" + _releases + "\ntabPressModifiers=" + _pressModifiers +
            "\ntabPressed=" + _pressedTab + "\ntabInteractions=" + _interactions +
            "\ntabAnimations=" + _animations +
            "\ntabDragOpaque=" + opaque + "\ntabTheme=" + _sets.ActualTheme + "\ntabDropHeld=" + _dropHeld +
            "\ntabPublished=" + Transfers.Count(transfer => transfer.HasPublishedData) +
            "\ntabTransferCompleting=" + committing + "\ntabDocuments=" + documents +
            "\ntabText=" + (committing ? "" : Convert.ToBase64String(Encoding.UTF8.GetBytes(_host.Core.GetSelectedReadyEditor()?.GetText() ?? ""))) +
            "\ntabFocus=" + (!committing && _host.Core.GetSelectedTab()?.ReadyEditor != null && Native(_host.Core.GetSelectedTab()).Focus) +
            "\ntabLayout=" + layout + "\ntabScroll=" + _sets.ScrollViewerHorizontalOffset.ToString(CultureInfo.InvariantCulture) + "\n";
    }

    internal static string Close(int index)
    {
        _host.Core.DeleteTab(_tabs[index]);
        return State();
    }

    internal static string FocusStatusAction()
    {
        if (!FeatureUI.Named<Button>(_host.Page, "EncodingIndicatorButton").Focus(FocusState.Programmatic))
            throw new InvalidOperationException("The status action did not receive focus.");
        return State();
    }

    internal static string SetDropHold(bool hold)
    {
        if (hold)
        {
            if (_dropGate != null) throw new InvalidOperationException("A drop hold is already armed.");
            _dropGate = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        }
        else
        {
            if (!_dropHeld) throw new InvalidOperationException("No drop deferral is held.");
            _dropGate.TrySetResult();
            _dropGate = null;
        }
        return State();
    }

    private static async void HoldDropAsync(object sender, DragEventArgs args)
    {
        if (_dropGate is not { } gate || !args.DataView.Contains(TextEditorTransfer.DataFormat)) return;
        var deferral = args.GetDeferral();
        _dropHeld = true;
        try { await gate.Task; }
        finally { _dropHeld = false; deferral.Complete(); }
    }

    internal static async Task DisposeAsync()
    {
        if (_host == null) return;
        _dropGate?.TrySetResult();
        _dropGate = null;
        var host = _host;
        _host = null;
        await host.DisposeAsync();
        Transfers.Clear();
        Documents.Clear();
    }

}
