// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

#include "pch.h"
#include "EditorStateReader.h"
#include "EditorStateReader.g.cpp"

namespace winrt::WinUIEditor::implementation
{
	EditorStateReader::EditorStateReader(WinUIEditor::EditorUtf8Reader lease, com_ptr<EditorBaseControl> const &view)
		: _lease(std::move(lease)), _writer(view->CaptureEditorState()) {}
	uint64_t EditorStateReader::Length() const
	{
		if (_closed.load()) throw_hresult(RO_E_CLOSED);
		return _writer->Length();
	}
	uint64_t EditorStateReader::Sequence() const
	{
		if (_closed.load()) throw_hresult(RO_E_CLOSED);
		return _lease.Sequence();
	}
	Windows::Foundation::IAsyncOperation<Windows::Storage::Streams::IBuffer> EditorStateReader::ReadAsync()
	{
		auto lifetime = get_strong();
		co_await _owner;
		if (_closed.load()) throw_hresult(RO_E_CLOSED);
		Windows::Storage::Streams::Buffer buffer{65536};
		buffer.Length(static_cast<uint32_t>(_writer->Read({buffer.data(), buffer.Capacity()})));
		co_return buffer;
	}
	void EditorStateReader::Close()
	{
		if (!_closed.exchange(true)) _lease.Close();
	}
}
