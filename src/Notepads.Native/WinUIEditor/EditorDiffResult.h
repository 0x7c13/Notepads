// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

#pragma once
#include "EditorDiffResult.g.h"
#include "NativeDiff.h"

namespace winrt::WinUIEditor::implementation
{
	struct EditorBaseControl;
	struct EditorDiffResult : EditorDiffResultT<EditorDiffResult>
	{
		EditorDiffResult(WinUIEditor::EditorDiffStatus status, ::WinUIEditor::DiffResult data,
			uint64_t oldRevision, uint64_t newRevision, uint64_t oldLength, uint64_t newLength,
			weak_ref<EditorBaseControl> oldView, weak_ref<EditorBaseControl> newView);
		WinUIEditor::EditorDiffStatus Status() const noexcept { return _status; }
		WinUIEditor::EditorDiffReason Reason() const noexcept { return static_cast<WinUIEditor::EditorDiffReason>(_data.reason); }
		uint64_t OldRevision() const noexcept { return _oldRevision; }
		uint64_t NewRevision() const noexcept { return _newRevision; }
		uint64_t OldByteLength() const noexcept { return _oldLength; }
		uint64_t NewByteLength() const noexcept { return _newLength; }
		uint32_t OldLineCount() const noexcept { return _data.oldLines; }
		uint32_t NewLineCount() const noexcept { return _data.newLines; }
		uint32_t AddedLineCount() const noexcept { return _data.addedLines; }
		uint32_t DeletedLineCount() const noexcept { return _data.deletedLines; }
		com_array<WinUIEditor::EditorDiffHunk> GetHunks() const;
		::WinUIEditor::DiffResult const &Data() const noexcept { return _data; }
		bool Matches(EditorBaseControl *view, bool oldSide) const;
	private:
		WinUIEditor::EditorDiffStatus _status;
		::WinUIEditor::DiffResult _data;
		uint64_t _oldRevision{}, _newRevision{}, _oldLength{}, _newLength{};
		weak_ref<EditorBaseControl> _oldView, _newView;
	};
}
