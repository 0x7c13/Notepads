// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Runtime.CompilerServices;
using System.Text;
using System.Threading.Tasks;
using Notepads.Controls;
using Notepads.Presentation.Workspace;
using Windows.ApplicationModel.DataTransfer;
using Windows.Foundation.Collections;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Data;
using Windows.UI.Xaml.Media.Animation;
using static NotepadsIntegrationTests.FeatureUI;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal static class TabDragContractsTests
{
    // Exercise native admission, including throwing subscribers, without
    // inventing framework event args or adding a test API to the control.
    [UnsafeAccessor(UnsafeAccessorKind.Method, Name = "BeginNativeDrag")]
    private static extern bool BeginNativeDrag(SetsView sets, object item, DataPackage data);

    [UnsafeAccessor(UnsafeAccessorKind.Method, Name = "CompleteNativeDrag")]
    private static extern void CompleteNativeDrag(SetsView sets, DataPackageOperation result, Exception error);

    internal static async Task RunAsync(FeatureTestHost host, StringBuilder log)
    {
        var sets = Named<SetsView>(host.Page, "Sets");
        var core = (NotepadsCore)host.Core;
        var tab = core.GetSelectedTab();
        var item = host.Page.TestContainer(tab);
        int completions = 0;
        int focusRequests = 0;
        TabDragCompletedEventArgs completed = null;
        void Completed(object sender, TabDragCompletedEventArgs args) { completions++; completed = args; }
        void InteractionCompleted(object sender, SetSelectedEventArgs args) => focusRequests++;
        sets.TabDragCompleted += Completed;
        sets.SetInteractionCompleted += InteractionCompleted;
        try
        {
            foreach (var throws in new[] { false, true })
            {
                var failure = new InvalidOperationException("Admission subscriber fixture");
                Guid operation = default;
                void Refuse(object sender, TabDragStartingEventArgs args)
                {
                    operation = args.OperationId;
                    Check(core.TestTabTransferOperation(tab) == operation,
                        "The earlier production subscriber owns the admitted transfer before a later refusal.");
                    if (throws) throw failure;
                    args.Cancel = true;
                }
                sets.TabDragStarting += Refuse;
                int before = completions;
                try
                {
                    if (throws)
                    {
                        Exception observed = null;
                        try { BeginNativeDrag(sets, item, new DataPackage()); }
                        catch (InvalidOperationException exception) { observed = exception; }
                        Check(ReferenceEquals(observed, failure), "Admission propagates the original subscriber exception.");
                    }
                    else Check(!BeginNativeDrag(sets, item, new DataPackage()), "A cancelled native admission is refused.");
                }
                finally { sets.TabDragStarting -= Refuse; }
                Check(completions == before + 1 && completed.OperationId == operation &&
                    ReferenceEquals(completed.Item, item) && completed.Result == DataPackageOperation.None &&
                    ReferenceEquals(completed.Error, throws ? failure : null),
                    "Refusal completes exactly the admitted operation, preserving its error and item.");
                CompleteNativeDrag(sets, DataPackageOperation.None, null);
                Check(completions == before + 1, "Late framework completion cannot complete a refused operation twice.");
                await UntilAsync(() => core.TestTabTransferOperation(tab) == null,
                    "Refused admission releases the earlier subscriber's transfer reservation.");
                Check(BeginNativeDrag(sets, item, new DataPackage()), "The next native drag is admitted after refusal or exception.");
                Check(!BeginNativeDrag(sets, item, new DataPackage()) &&
                    !sets.MoveSet(item, sets.Items.Count - 1),
                    "An active native drag refuses overlapping admission and programmatic roster mutation.");
                CompleteNativeDrag(sets, DataPackageOperation.None, null);
                await UntilAsync(() => core.TestTabTransferOperation(tab) == null, "The subsequent operation also retires normally.");
            }
        }
        finally
        {
            sets.TabDragCompleted -= Completed;
            sets.SetInteractionCompleted -= InteractionCompleted;
            CompleteNativeDrag(sets, DataPackageOperation.None, null);
        }
        Check(focusRequests == 0, "Native completion and admission refusal do not request focus in the source window.");

        var originalValue = sets.ReadLocalValue(ItemsControl.ItemContainerTransitionsProperty);
        var originalBinding = sets.GetBindingExpression(ItemsControl.ItemContainerTransitionsProperty)?.ParentBinding;
        int originalIndex = sets.Items.IndexOf(item);
        int target = originalIndex == 0 ? sets.Items.Count - 1 : 0;
        try
        {
            foreach (var bound in new[] { false, true })
            {
                var transitions = new TransitionCollection { new ReorderThemeTransition() };
                var binding = new Binding
                {
                    Source = new ListView { ItemContainerTransitions = transitions },
                    Path = new PropertyPath("ItemContainerTransitions"), Mode = BindingMode.OneWay
                };
                if (bound) sets.SetBinding(ItemsControl.ItemContainerTransitionsProperty, binding);
                else sets.ItemContainerTransitions = transitions;
                Check(ReferenceEquals(sets.ItemContainerTransitions, transitions), "The caller's transitions are installed.");
                int rosterChanges = 0;
                int reentrantRefusals = 0;
                bool suppressed = true;
                void RosterChanged(IObservableVector<object> sender, IVectorChangedEventArgs args)
                {
                    rosterChanges++;
                    if (!sets.MoveSet((SetsViewItem)sets.Items[0], sets.Items.Count - 1)) reentrantRefusals++;
                    // XAML may lazily create an empty collection after null.
                    // Observe the edits themselves, not getter null identity.
                    suppressed &= (sets.ItemContainerTransitions?.Count ?? 0) == 0;
                }
                sets.Items.VectorChanged += RosterChanged;
                try { Check(sets.MoveSet(item, target), "Public reorder commits its requested destination."); }
                finally { sets.Items.VectorChanged -= RosterChanged; }
                Check(rosterChanges == 2 && reentrantRefusals == 2 && suppressed,
                    "Both roster mutations refuse reentrant moves without retiring the outer transition scope.");
                await TabDragTestHost.FrameAsync();
                Check(ReferenceEquals(sets.ItemContainerTransitions, transitions) &&
                    (!bound || ReferenceEquals(sets.GetBindingExpression(ItemsControl.ItemContainerTransitionsProperty)?.ParentBinding, binding)),
                    "After layout public reorder restores the caller's exact transition collection and binding.");
                Check(!sets.MoveSet(item, -1) && ReferenceEquals(sets.ItemContainerTransitions, transitions),
                    "A refused reorder does not leave transitions suppressed.");
                Check(sets.MoveSet(item, originalIndex), "Restore the fixture's original tab order.");
                await TabDragTestHost.FrameAsync();
            }
        }
        finally
        {
            if (originalBinding != null) sets.SetBinding(ItemsControl.ItemContainerTransitionsProperty, originalBinding);
            else if (originalValue == DependencyProperty.UnsetValue) sets.ClearValue(ItemsControl.ItemContainerTransitionsProperty);
            else sets.SetValue(ItemsControl.ItemContainerTransitionsProperty, originalValue);
        }
        log.AppendLine("PASS: WorkspaceCommands.TabDragContracts — native refusal/exception releases admission; public reorder restores caller transitions after layout.");
    }
}
