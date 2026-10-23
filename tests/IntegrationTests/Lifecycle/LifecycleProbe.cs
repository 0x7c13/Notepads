// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Threading.Tasks;
using Notepads;
using Notepads.Features.Preferences;
using Notepads.Infrastructure.Settings;
using Notepads.Presentation.Controls.TextEditor;
using Notepads.Presentation.Views.MainPage;
using Notepads.Presentation.Workspace;
using NotepadsTestIsolation;
using Windows.ApplicationModel.Activation;
using Windows.UI.Core;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Media;

namespace NotepadsIntegrationTests;

internal static class LifecycleProbe
{
    private static int _activations;
    private static bool _bound;
    private static bool _prepared;
    private static long _activationTicks;
    private static long _paintTicks;
    private static long _readyTicks;
    private static int _observation;
    private static TaskCompletionSource<bool> _pendingActivation;
    private static Task _presentation;
    private static NotepadsMainPage _page;
    private static readonly TaskCompletionSource<bool> FirstPaint = new(TaskCreationOptions.RunContinuationsAsynchronously);
    private static string Root => ApplicationData.Current.LocalFolder.Path;
    private static string Stem => "lifecycle-" + Environment.ProcessId;

    internal static void Declined(string status)
    {
        _prepared = true;
        _activations++;
        Publish(status);
    }

    internal static void Activated(IActivatedEventArgs args)
    {
        _activationTicks = Stopwatch.GetTimestamp();
        _activations++;
        var prelaunch = args is LaunchActivatedEventArgs { PrelaunchActivated: true };
        _prepared |= prelaunch;
        if (!_bound)
        {
            _bound = true;
            Window.Current.Activated += (_, _) => BindPage();
            Application.Current.UnhandledException += (_, error) => Fail(error.Exception);
            var dispatcher = Window.Current.Dispatcher;
            _ = Task.Run(() => WatchCommandsAsync(dispatcher));
        }
        _ = Window.Current.Dispatcher.RunAsync(CoreDispatcherPriority.Low, async () =>
        {
            try
            {
                if (prelaunch) Publish("prelaunch");
                else
                {
                    BindPage();
                    await ReadyAsync();
                    if (LifecycleProgram.Phase == "--lifecycle-seed") await SeedAsync();
                    Publish(LifecycleProgram.Phase == "--lifecycle-seed" ? "seeded" : "ready");
                }
            }
            catch (Exception error) { Fail(error); }
        });
    }

    private static void BindPage()
    {
        if (_page != null || Window.Current.Content is not Frame { Content: NotepadsMainPage page }) return;
        _page = page;
        _page.TestCore.EditorAttached += (_, tab) => ObserveEditor(tab);
        var current = _page.TestCore.GetSelectedTab();
        if (current?.ReadyEditor != null) ObserveEditor(current);
    }

    private static void ObserveEditor(WorkspaceTab tab)
    {
        var core = ((TextEditor)tab.ReadyEditor).TestNativeCore;
        core.Width = 1512;
        core.Height = 843;
        var view = (WinUIEditor.EditorBaseControl)((Grid)core.Content).Children[0];
        view.Editor.Painted += (_, _) =>
        {
            if (_paintTicks != 0 || !ReferenceEquals(_page.TestCore.GetSelectedTab(), tab)) return;
            _paintTicks = Stopwatch.GetTimestamp();
            if (((App)Application.Current).TestOwnsPrimaryWorkspace && LifecycleProgram.Phase != "--lifecycle-seed" && File.Exists(Path.Combine(Root, "lifecycle-fixture.txt")) &&
                (!view.IsViewportRestorePending && view.Editor.DocLineFromVisible(view.Editor.FirstVisibleLine) == 80))
                FirstPaint.TrySetResult(true);
            else if (!((App)Application.Current).TestOwnsPrimaryWorkspace || LifecycleProgram.Phase == "--lifecycle-seed" || !File.Exists(Path.Combine(Root, "lifecycle-fixture.txt")))
                FirstPaint.TrySetResult(true);
            else FirstPaint.TrySetException(new InvalidOperationException("The first paint did not restore the saved viewport."));
        };
    }

    private static async Task ReadyAsync()
    {
        if (_page == null) throw new InvalidOperationException("A real activation did not create the workspace.");
        await FirstPaint.Task.WaitAsync(TimeSpan.FromSeconds(20));
        var frame = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
        void Rendering(object sender, object args) => frame.TrySetResult(true);
        CompositionTarget.Rendering += Rendering;
        try { await frame.Task.WaitAsync(TimeSpan.FromSeconds(10)); }
        finally { CompositionTarget.Rendering -= Rendering; }
        _readyTicks = Stopwatch.GetTimestamp();
        if (_page.TestCore.GetSelectedReadyEditor() is not TextEditor editor || !editor.TestNativeCore.IsEnabled)
            throw new InvalidOperationException("The selected editor is not editable.");
        var view = (WinUIEditor.EditorBaseControl)((Grid)editor.TestNativeCore.Content).Children[0];
        if (view.Editor.ReadOnly || view.IsViewportRestorePending) throw new InvalidOperationException("The selected native editor is not ready for editing.");
        _page.TestController.StopSessionBackup();
    }

    private static async Task SeedAsync()
    {
        var editor = _page.TestCore.GetSelectedReadyEditor();
        editor.TypeText(string.Concat(Enumerable.Repeat("prelaunch 中文 Ελληνικά 😀 " + new string('x', 256) + "\r", 170)));
        var metadata = editor.GetTextEditorStateMetaData();
        metadata.WrapWord = false;
        metadata.FontZoomFactor = 1;
        metadata.SelectionStartPosition = metadata.SelectionEndPosition = 40;
        metadata.FirstVisibleDocumentLine = 80;
        metadata.FirstVisibleSubLine = 0;
        editor.ResetEditorState(metadata);
        await Task.Yield();
        if (!await _page.TestController.SaveSessionAsync()) throw new InvalidOperationException("Could not save the fixture.");
        File.WriteAllText(Path.Combine(Root, "lifecycle-fixture.txt"), Hash(editor.GetText()));
    }

    private static async Task WatchCommandsAsync(CoreDispatcher dispatcher)
    {
        try
        {
            var path = Path.Combine(Root, Stem + "-command.txt");
            string previous = null;
            while (true)
            {
                string command = null;
                try { if (File.Exists(path)) command = File.ReadAllText(path); }
                catch (IOException) { }
                if (!string.IsNullOrEmpty(command) && command != previous)
                {
                    previous = command;
                    var separator = command.IndexOf(':');
                    var commandId = int.Parse(command.AsSpan(0, separator), System.Globalization.CultureInfo.InvariantCulture);
                    var action = command[(separator + 1)..];
                    var completed = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
                    await dispatcher.RunAsync(CoreDispatcherPriority.Normal, async () =>
                    {
                        try
                        {
                            if (action == "exit")
                            {
                                if (_page != null) await _page.TestDisposeWindowAsync();
                                TestApplicationData.ValidatePreservedFileGrants();
                                Publish("exiting", commandId: commandId);
                                await LifecycleProgram.WaitForRetentionAsync();
                                _ = Task.Run(() => Environment.Exit(0));
                            }
                            else
                            {
                                if (action is "begin-activation" or "begin-late-activation")
                                {
                                    if (_pendingActivation != null) throw new InvalidOperationException("An activation is already pending.");
                                    _pendingActivation = new(TaskCreationOptions.RunContinuationsAsynchronously);
                                    _presentation = action == "begin-activation" ? App.TestPresentActivation(_pendingActivation.Task) :
                                        App.TestLatePresentation(_pendingActivation.Task);
                                }
                                if (action is "complete-activation" or "dispose-activation")
                                {
                                    if (_pendingActivation == null) throw new InvalidOperationException("There is no pending activation.");
                                    // Page shutdown settles its canceled activation queue successfully.
                                    if (action == "dispose-activation") await _page.TestDisposeWindowAsync();
                                    _pendingActivation.SetResult(true);
                                    await _presentation;
                                    _pendingActivation = null;
                                }
                                if (action == "new-window") ApplicationPreferences.AlwaysOpenNewWindow = true;
                                if (action == "reuse-window") ApplicationPreferences.AlwaysOpenNewWindow = false;
                                if (action == "clear-active") ApplicationSettingsStore.Remove(SettingsKey.ActiveInstanceIdStr);
                                if (action.StartsWith("stale-active-", StringComparison.Ordinal))
                                    ApplicationSettingsStore.Write(SettingsKey.ActiveInstanceIdStr, Guid.Parse(action.AsSpan("stale-active-".Length)).ToString());
                                if (action.StartsWith("focus", StringComparison.Ordinal)) Window.Current.Activate();
                                Publish(action, commandId: commandId);
                            }
                        }
                        catch (Exception error) { Fail(error); }
                        finally { completed.TrySetResult(true); }
                    });
                    await completed.Task;
                }
                await Task.Delay(25);
            }
        }
        catch (Exception error)
        {
            await dispatcher.RunAsync(CoreDispatcherPriority.Normal, () => Fail(error));
        }
    }

    private static string Hash(string text) => Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(text)));

    internal static string SettingsFingerprint() => Hash(string.Join("\n", ApplicationData.Current.LocalSettings.Values
        .OrderBy(pair => pair.Key, StringComparer.Ordinal).Select(pair => pair.Key + "=" + pair.Value?.GetType().FullName + ":" +
            Convert.ToString(pair.Value, System.Globalization.CultureInfo.InvariantCulture))));

    private static void Publish(string status, string error = null, int commandId = 0)
    {
        var app = (App)Application.Current;
        using var process = Process.GetCurrentProcess();
        var editor = _page?.TestCore.GetSelectedReadyEditor() as TextEditor;
        var core = editor?.TestNativeCore;
        var result = new LifecycleObservation
        {
            Status = status, Error = error, ProcessId = process.Id, Activations = _activations, Prepared = _prepared,
            PresentationRequests = App.TestPresentationRequests,
            StartTicks = process.StartTime.ToUniversalTime().Ticks,
            CommandId = commandId, EntryTicks = LifecycleProgram.EntryTicks, IsolationReadyTicks = LifecycleProgram.IsolationReadyTicks,
            SettingsBefore = LifecycleProgram.SettingsBefore, SettingsAfter = SettingsFingerprint(),
            HasWorkspace = app.TestHasWorkspace, Primary = app.TestOwnsPrimaryWorkspace, Visible = Window.Current.Visible,
            ActiveInstance = ApplicationSettingsStore.Read(SettingsKey.ActiveInstanceIdStr) as string,
            Instance = app.TestInstanceKey, ClipboardFlushes = TestClipboard.FlushCount,
            FirstPaintTicks = _paintTicks, ReadyTicks = _readyTicks, ActivationTicks = _activationTicks,
            PrivateBytes = process.PrivateMemorySize64, CpuMilliseconds = process.TotalProcessorTime.TotalMilliseconds,
            TextHash = editor == null ? null : Hash(editor.GetText()), Tabs = _page?.TestCore.GetTabCount() ?? 0,
            FirstVisibleLine = core?.FirstVisibleRow ?? -1, Width = core?.ActualWidth ?? 0, Height = core?.ActualHeight ?? 0,
            FontSize = core?.FontSize ?? 0
        };
        var path = Path.Combine(Root, Stem + ".json");
        var json = JsonSerializer.Serialize(result, LifecycleJsonContext.Default.LifecycleObservation);
        var receipt = Path.Combine(Root, $"{Stem}-{status}-{++_observation:D4}.json");
        File.WriteAllText(receipt + ".tmp", json);
        File.Move(receipt + ".tmp", receipt);
        File.WriteAllText(path + ".tmp", json);
        File.Move(path + ".tmp", path, true);
    }

    private static void Fail(Exception error) => Publish("failed", error.ToString());
}

internal sealed class LifecycleObservation
{
    public string Status { get; set; }
    public string Error { get; set; }
    public int ProcessId { get; set; }
    public long StartTicks { get; set; }
    public int CommandId { get; set; }
    public long EntryTicks { get; set; }
    public long IsolationReadyTicks { get; set; }
    public string SettingsBefore { get; set; }
    public string SettingsAfter { get; set; }
    public double Width { get; set; }
    public double Height { get; set; }
    public double FontSize { get; set; }
    public int Activations { get; set; }
    public int PresentationRequests { get; set; }
    public bool Prepared { get; set; }
    public bool HasWorkspace { get; set; }
    public bool Primary { get; set; }
    public bool Visible { get; set; }
    public string ActiveInstance { get; set; }
    public string Instance { get; set; }
    public int ClipboardFlushes { get; set; }
    public long ActivationTicks { get; set; }
    public long FirstPaintTicks { get; set; }
    public long ReadyTicks { get; set; }
    public long PrivateBytes { get; set; }
    public double CpuMilliseconds { get; set; }
    public string TextHash { get; set; }
    public int Tabs { get; set; }
    public long FirstVisibleLine { get; set; }
}

[JsonSerializable(typeof(LifecycleObservation))]
internal partial class LifecycleJsonContext : JsonSerializerContext;
