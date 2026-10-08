// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

// Test-only access to the production command adapters, without synthesizing
// keyboard state or changing the public editor API.
namespace Notepads.Presentation.Controls.TextEditor;

public sealed partial class TextEditorCore
{
    internal void TestIndentation(bool remove) => ChangeIndentation(remove);
    internal void TestDeleteBack() => DeleteBackWithUnindent();
    internal void TestJoinLines() => JoinText();
    internal void TestEnter() => EnterWithAutoIndentation();
    internal void TestCharAdded() => OnNativeCharAdded(Native, null);
    internal void TestApplyTypedIndentation(long version, long pos) => ApplyTypedIndentation(version, pos);
    internal long TestIndentationCheckedVersion => _indentationCheckedVersion;
}
