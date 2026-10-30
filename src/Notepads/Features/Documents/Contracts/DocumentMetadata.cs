// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Features.Documents.Contracts;

public sealed class DocumentMetadata
{
    public string FileNamePlaceholder { get; set; }

    // Null is the released V1 format: infer the default untitled name once.
    [System.Text.Json.Serialization.JsonIgnore(Condition = System.Text.Json.Serialization.JsonIgnoreCondition.WhenWritingNull)]
    public bool? UseContentBasedTitle { get; set; }

    // The roster can display an unloaded tab without opening its snapshot.
    [System.Text.Json.Serialization.JsonIgnore(Condition = System.Text.Json.Serialization.JsonIgnoreCondition.WhenWritingNull)]
    public string ContentTitle { get; set; }

    [System.Text.Json.Serialization.JsonIgnore(Condition = System.Text.Json.Serialization.JsonIgnoreCondition.WhenWritingNull)]
    public string LanguageOverride { get; set; }

    // 0 follows detection/language/preferences; -1 uses tabs; 2/4/8 use spaces.
    [System.Text.Json.Serialization.JsonIgnore(Condition = System.Text.Json.Serialization.JsonIgnoreCondition.WhenWritingDefault)]
    public int IndentationOverride { get; set; }

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

    public bool RightToLeftReadingOrder { get; set; }

    public double FontZoomFactor { get; set; }

    // DIPs from the document's reading-order leading edge, independent of its measured canvas width.
    public double ScrollViewerHorizontalOffset { get; set; }

    public double ScrollViewerVerticalOffset { get; set; }

    // A display row changes as wrapping progresses; the document anchor does not.
    [System.Text.Json.Serialization.JsonIgnore(Condition = System.Text.Json.Serialization.JsonIgnoreCondition.WhenWritingNull)]
    public int? FirstVisibleDocumentLine { get; set; }

    [System.Text.Json.Serialization.JsonIgnore(Condition = System.Text.Json.Serialization.JsonIgnoreCondition.WhenWritingDefault)]
    public int FirstVisibleSubLine { get; set; }

    // DIPs hidden above the anchored display row; zero for V1 sessions.
    [System.Text.Json.Serialization.JsonIgnore(Condition = System.Text.Json.Serialization.JsonIgnoreCondition.WhenWritingDefault)]
    public double FirstVisibleRowOffset { get; set; }

    public bool IsContentPreviewPanelOpened { get; set; }

    public bool IsInDiffPreviewMode { get; set; }

    // Metadata is a sealed value-only contract (scalars and immutable strings).
    // Copy the complete contract so new fields cannot be lost during capture.
    public DocumentMetadata Clone() => (DocumentMetadata)MemberwiseClone();
}
