// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;

namespace Notepads.Features.Sessions.Contracts;

// Transient launch intent and live editing state. Durable content authority remains
// the transfer offer/receipt/acknowledgement; deleting this request fences imports.
internal sealed class WindowMoveRequest
{
    internal const long MaximumStateBytes = 256L * 1024 * 1024;
    public int Version { get; set; } = 1;
    public TransferToken Source { get; set; }
    public long StateBytes { get; set; }
    public string StateSha256 { get; set; }
    public bool HasAddedLogEntry { get; set; }
    public int DetectedIndentation { get; set; }
    public bool IndentationChecked { get; set; }

    public void Validate()
    {
        if (Source == null || Version != 1 || StateBytes is < 264 or > MaximumStateBytes ||
            !Sha256Hex.IsValid(StateSha256) || DetectedIndentation is < -1 or > 256)
            throw new InvalidDataException("Invalid new-window move request.");
        Source.Validate();
    }

    public bool Matches(WindowMoveRequest other) => other != null && Version == other.Version && Source.Matches(other.Source) &&
        StateBytes == other.StateBytes && string.Equals(StateSha256, other.StateSha256, StringComparison.OrdinalIgnoreCase) &&
        HasAddedLogEntry == other.HasAddedLogEntry && DetectedIndentation == other.DetectedIndentation &&
        IndentationChecked == other.IndentationChecked;
}
