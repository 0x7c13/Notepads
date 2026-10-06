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
    internal void TestJoinLines() => JoinText();
}
