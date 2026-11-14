// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using Windows.ApplicationModel.DataTransfer;

namespace Notepads.Controls;

// Native touch drags and captured mouse drags share one transfer boundary.
public sealed class TabDragStartingEventArgs(Guid operationId, object item, DataPackage data) : EventArgs
{
    public Guid OperationId { get; } = operationId;
    public object Item { get; } = item;
    public DataPackage Data { get; } = data;
    public bool Cancel { get; set; }
}

public sealed class TabDragCompletedEventArgs(Guid operationId, object item, DataPackageOperation result, Exception error = null) : EventArgs
{
    public Guid OperationId { get; } = operationId;
    public object Item { get; } = item;
    public DataPackageOperation Result { get; } = result;
    public Exception Error { get; } = error;
}
