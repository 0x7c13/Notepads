// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

#include "pch.h"
#include "EditorDiffResult.h"
#include "EditorBaseControl.h"
#include "EditorDiffResult.g.cpp"

namespace winrt::WinUIEditor::implementation
{
	EditorDiffResult::EditorDiffResult(WinUIEditor::EditorDiffStatus status, ::WinUIEditor::DiffResult data,
		uint64_t oldRevision, uint64_t newRevision, uint64_t oldLength, uint64_t newLength,
		weak_ref<EditorBaseControl> oldView, weak_ref<EditorBaseControl> newView)
		: _status(status), _data(std::move(data)), _oldRevision(oldRevision), _newRevision(newRevision),
		_oldLength(oldLength), _newLength(newLength), _oldView(std::move(oldView)), _newView(std::move(newView)) {}
	bool EditorDiffResult::Matches(EditorBaseControl *view, bool oldSide) const
	{
		return (oldSide ? _oldView : _newView).get().get() == view;
	}

	com_array<WinUIEditor::EditorDiffHunk> EditorDiffResult::GetHunks() const
	{
		com_array<WinUIEditor::EditorDiffHunk> result(static_cast<uint32_t>(_data.hunks.size()));
		uint64_t padding = 0;
		for (uint32_t i = 0; i < _data.hunks.size(); ++i)
		{
			const auto &hunk = _data.hunks[i];
			result[i] = {hunk.oldStart, hunk.oldCount, hunk.newStart, hunk.newCount, hunk.oldStart + padding};
			if (hunk.newCount > hunk.oldCount) padding += hunk.newCount - hunk.oldCount;
		}
		return result;
	}
}
