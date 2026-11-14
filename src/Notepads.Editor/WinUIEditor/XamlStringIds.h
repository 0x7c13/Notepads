#pragma once

namespace WinUIEditor
{
	enum XamlTextCommand : DWORD
	{
		LabelCut = 5528,
		LabelCopy = 5529,
		LabelPaste = 5530,
		LabelSelectAll = 5531,
		LabelDelete = 5532,
		// ...
		LabelUndo = 5542,
		LabelRedo = 5543,
	};
}
