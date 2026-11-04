// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Presentation.Workspace
{
    public sealed partial class NotepadsCore
    {
        internal int TestOwnedEditorCount => _ownedEditors.Count;
        internal global::Notepads.Controls.TabDragStartingEventArgs TestBeginTabTransfer(System.Guid operation, WorkspaceTab tab)
        {
            var args = new global::Notepads.Controls.TabDragStartingEventArgs(operation, tab, new Windows.ApplicationModel.DataTransfer.DataPackage());
            Sets_TabDragStarting(this, args);
            return args;
        }
        internal System.Guid? TestTabTransferOperation(WorkspaceTab tab) =>
            _transfers.TryGetValue(tab.Id, out var active) ? active.OperationId : null;
        // Pause actual transfer cleanup at its existing semaphore, without timers
        // or a product hook. UnsafeAccessor is resolved statically by Native AOT.
        [System.Runtime.CompilerServices.UnsafeAccessor(System.Runtime.CompilerServices.UnsafeAccessorKind.Field, Name = "_gate")]
        private static extern ref System.Threading.SemaphoreSlim TransferGate(TextEditorTransfer transfer);
        internal System.Threading.SemaphoreSlim TestTabTransferGate(WorkspaceTab tab) => TransferGate(_transfers[tab.Id].Transfer);
        internal TextEditorTransfer[] TestTabTransfers =>
            System.Linq.Enumerable.ToArray(System.Linq.Enumerable.Select(_transfers.Values, active => active.Transfer));
        internal bool TestTabTransferCompleting => System.Linq.Enumerable.Any(_transfers.Values, active => active.Completing);
        internal void TestEndTabTransfer(System.Guid operation, WorkspaceTab tab,
            Windows.ApplicationModel.DataTransfer.DataPackageOperation result = Windows.ApplicationModel.DataTransfer.DataPackageOperation.None) =>
            Sets_TabDragCompleted(this, new global::Notepads.Controls.TabDragCompletedEventArgs(operation, tab,
                result));
    }
}

namespace Notepads.Presentation.Controls.TextEditor
{
    public sealed partial class TextEditor
    {
        internal TextEditorCore TestNativeCore => TextEditorCore;
    }
}
