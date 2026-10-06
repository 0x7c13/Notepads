// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using Notepads.Features.Sessions.Contracts.Legacy;

namespace Notepads.Features.Sessions.Contracts;

internal sealed class NotepadsSessionDataV2
{
    public int Version { get; set; } = 2;

    public SessionScopeData Scope { get; set; }

    public Guid SelectedTextEditor { get; set; }

    public double TabScrollViewerHorizontalOffset { get; set; }

    public List<TextEditorSessionDataV2> TextEditors { get; set; } = new List<TextEditorSessionDataV2>();

    // Failed V1 recovery records remain intact until they can be restored.
    // Publishing V2 must never imply that those tabs were intentionally closed.
    public List<TextEditorSessionDataV1> UnrecoveredLegacyEditors { get; set; } = new List<TextEditorSessionDataV1>();

    // Global transfer roots retain the ordinary V2 asset graph while binding
    // source deletion to a receiver's independently durable generation.
    public TransferToken TransferSource { get; set; }

    public TransferReceiptData TransferReceipt { get; set; }

    public TransferToken TransferAcknowledgement { get; set; }

    public TextEditorSessionDataV2 AcknowledgedSource { get; set; }

    public long SourceRemovalRevision { get; set; }
}
