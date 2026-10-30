// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Presentation.Controls.TextEditor;

public sealed partial class TextEditorCore
{
    internal void TestReadingOrderCommand(bool rightToLeft) => SetReadingOrderFromCommand(rightToLeft);
}
