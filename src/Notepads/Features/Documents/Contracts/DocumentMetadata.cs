// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Features.Documents.Contracts;

public sealed class DocumentMetadata
{
    public string FileNamePlaceholder { get; set; }

    [System.Text.Json.Serialization.JsonIgnore(Condition = System.Text.Json.Serialization.JsonIgnoreCondition.WhenWritingNull)]
    public string LanguageOverride { get; set; }

    public string LastSavedEncoding { get; set; }

    public string LastSavedLineEnding { get; set; }

    public long DateModifiedFileTime { get; set; }

    public string RequestedLineEnding { get; set; }

    public string RequestedEncoding { get; set; }

    public bool HasEditingFile { get; set; }

    public bool RequiresSaveAs { get; set; }

    public bool IsModified { get; set; }

    public int SelectionStartPosition { get; set; }

    public int SelectionEndPosition { get; set; }

    public bool WrapWord { get; set; }

    public double FontZoomFactor { get; set; }

    public double ScrollViewerHorizontalOffset { get; set; }

    public double ScrollViewerVerticalOffset { get; set; }

    public bool IsContentPreviewPanelOpened { get; set; }

    public bool IsInDiffPreviewMode { get; set; }

    // Metadata is a sealed value-only contract (scalars and immutable strings).
    // Copy the complete contract so new fields cannot be lost during capture.
    public DocumentMetadata Clone() => (DocumentMetadata)MemberwiseClone();
}
