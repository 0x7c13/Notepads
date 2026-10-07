// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

#include "pch.h"
#include "EditorJournalCheckpoint.h"
#include "EditorJournalCheckpoint.g.cpp"

namespace winrt::WinUIEditor::implementation
{
	EditorJournalCheckpoint::EditorJournalCheckpoint(
		std::shared_ptr<::WinUIEditor::NativeJournalFile> file, ::WinUIEditor::JournalPrefix prefix)
		: _file(std::move(file)), _prefix(std::move(prefix))
	{
	}
	void EditorJournalCheckpoint::CheckOpen() const
	{
		if (_closed)
			throw_hresult(RO_E_CLOSED);
	}
	::WinUIEditor::JournalPrefix EditorJournalCheckpoint::Prefix()
	{
		std::lock_guard guard(_mutex);
		CheckOpen();
		return _prefix;
	}
	hstring EditorJournalCheckpoint::FilePath()
	{
		return hstring{Prefix().path};
	}
	uint64_t EditorJournalCheckpoint::BaseSequence()
	{
		return Prefix().baseSequence;
	}
	uint64_t EditorJournalCheckpoint::CommittedSequence()
	{
		return Prefix().sequence;
	}
	uint64_t EditorJournalCheckpoint::CommittedByteLength()
	{
		return Prefix().byteLength;
	}
	uint64_t EditorJournalCheckpoint::CommittedDocumentByteLength()
	{
		return Prefix().documentByteLength;
	}
	hstring EditorJournalCheckpoint::PrefixSha256()
	{
		return to_hstring(Prefix().sha256);
	}
	WinUIEditor::EditorJournalCheckpoint EditorJournalCheckpoint::Retain()
	{
		std::lock_guard guard(_mutex);
		CheckOpen();
		return make<implementation::EditorJournalCheckpoint>(_file, _prefix);
	}
	Windows::Foundation::IAsyncAction EditorJournalCheckpoint::FlushAsync()
	{
		auto lifetime = get_strong();
		std::shared_ptr<::WinUIEditor::NativeJournalFile> file;
		::WinUIEditor::JournalPrefix prefix;
		{
			std::lock_guard guard(_mutex);
			CheckOpen();
			file = _file;
			prefix = _prefix;
		}
		if (!file)
			co_return;
		co_await resume_background();
		prefix = file->Flush(std::move(prefix));
		{
			std::lock_guard guard(_mutex);
			if (!_closed)
				_prefix = std::move(prefix);
		}
	}
	void EditorJournalCheckpoint::Close()
	{
		std::lock_guard guard(_mutex);
		_closed = true;
		_file.reset();
		_prefix.digest.reset();
	}
	Windows::Foundation::IAsyncAction EditorJournalCheckpoint::WriteBaselineAsync(Windows::Storage::Streams::IInputStream baseline,
		uint64_t baselineLength, Windows::Storage::Streams::IOutputStream output, uint64_t maximumDocumentBytes)
	{
		auto lifetime = get_strong();
		if (!baseline || !output || maximumDocumentBytes > INT32_MAX || baselineLength > maximumDocumentBytes ||
			Prefix().documentByteLength > maximumDocumentBytes)
			throw hresult_invalid_argument();
		auto state = std::make_shared<BaselineWriteState>();
		auto cancellation = co_await get_cancellation_token();
		cancellation.callback([state]() noexcept { state->canceled.store(true); });
		cancellation.enable_propagation(false);
		co_await WriteBaselineOperationAsync(std::move(baseline), baselineLength, std::move(output), maximumDocumentBytes, state);
	}
	Windows::Foundation::IAsyncAction EditorJournalCheckpoint::WriteBaselineOperationAsync(Windows::Storage::Streams::IInputStream baseline,
		uint64_t baselineLength, Windows::Storage::Streams::IOutputStream output, uint64_t maximumDocumentBytes,
		std::shared_ptr<BaselineWriteState> state)
	{
		auto lifetime = get_strong();
		// Own the prefix independently of the caller's checkpoint lifetime.
		std::shared_ptr<::WinUIEditor::NativeJournalFile> file;
		::WinUIEditor::JournalPrefix prefix;
		{
			std::lock_guard guard(_mutex);
			CheckOpen();
			file = _file;
			prefix = _prefix;
		}
		co_await resume_background();
		if (file)
			prefix = file->Flush(std::move(prefix));
		prefix = ::WinUIEditor::NativeJournalFile::Validate(prefix.path, prefix.baseSequence, prefix.sequence, prefix.byteLength,
			prefix.sha256, [state]() { return state->canceled.load(); });
		// Include the largest intermediate edit, not just the final length.
		// Reject before creating or allocating the candidate native document.
		if (prefix.maximumDocumentByteLength > maximumDocumentBytes || baselineLength > maximumDocumentBytes)
			throw hresult_invalid_argument();
		if (state->canceled.load())
			throw hresult_canceled();
		auto document = std::make_unique<Scintilla::Internal::Document>(Scintilla::DocumentOption::StylesNone);
		document->eolMode = Scintilla::EndOfLine::Cr;
		document->Allocate(static_cast<Sci::Position>(baselineLength));
		document->SetUndoCollection(false);
		Windows::Storage::Streams::Buffer buffer{65536};
		uint64_t consumed = 0;
		while (consumed < baselineLength)
		{
			if (state->canceled.load())
				throw hresult_canceled();
			const auto request = static_cast<uint32_t>(std::min(uint64_t{buffer.Capacity()}, baselineLength - consumed));
			auto received = co_await baseline.ReadAsync(buffer, request, Windows::Storage::Streams::InputStreamOptions::Partial);
			const auto count = received.Length();
			if (!count || count > request)
				throw_hresult(HRESULT_FROM_WIN32(ERROR_HANDLE_EOF));
			const auto status = document->AddData(reinterpret_cast<const char *>(received.data()), count);
			if (status == static_cast<int>(Scintilla::Status::BadAlloc))
				throw_hresult(E_OUTOFMEMORY);
			if (status != static_cast<int>(Scintilla::Status::Ok))
				throw_hresult(E_FAIL);
			consumed += count;
		}
		auto remainder = co_await baseline.ReadAsync(buffer, 1, Windows::Storage::Streams::InputStreamOptions::Partial);
		if (remainder.Length())
			throw hresult_invalid_argument();
		::WinUIEditor::NativeJournalFile::ValidateCanonicalDocument(*document, [state]() { return state->canceled.load(); });
		::WinUIEditor::NativeJournalFile::Replay(*document, prefix, [state]() { return state->canceled.load(); });
		for (Sci::Position offset = 0; offset < document->Length();)
		{
			if (state->canceled.load())
				throw hresult_canceled();
			const auto count = static_cast<uint32_t>(std::min(static_cast<Sci::Position>(buffer.Capacity()), document->Length() - offset));
			document->GetCharRange(reinterpret_cast<char *>(buffer.data()), offset, count);
			buffer.Length(count);
			uint32_t written = 0;
			while (written < count)
			{
				Windows::Storage::Streams::Buffer remaining{count - written};
				memcpy(remaining.data(), buffer.data() + written, count - written);
				remaining.Length(count - written);
				const auto completed = co_await output.WriteAsync(remaining);
				if (!completed || completed > count - written)
					throw_hresult(HRESULT_FROM_WIN32(ERROR_WRITE_FAULT));
				written += completed;
			}
			offset += count;
		}
		if (state->canceled.load())
			throw hresult_canceled();
		if (!co_await output.FlushAsync())
			throw_hresult(HRESULT_FROM_WIN32(ERROR_WRITE_FAULT));
	}
	Windows::Foundation::IAsyncAction EditorJournalCheckpoint::CloseAsync()
	{
		Close();
		co_return;
	}
	void EditorJournalCheckpoint::PreserveForRecovery()
	{
		// Files belong to the host's registered document-asset lease. Native
		// checkpoint disposal never deletes a file; durable promotion is host-owned.
		std::lock_guard guard(_mutex);
		CheckOpen();
	}
	Windows::Foundation::IAsyncOperation<WinUIEditor::EditorJournalCheckpoint> EditorJournalCheckpoint::OpenAsync(
		hstring filePath, uint64_t baseSequence, uint64_t committedSequence, uint64_t committedByteLength, hstring prefixSha256)
	{
		auto cancellation = co_await get_cancellation_token();
		co_await resume_background();
		auto prefix = ::WinUIEditor::NativeJournalFile::Validate(std::wstring{filePath}, baseSequence, committedSequence,
			committedByteLength, to_string(prefixSha256), [cancellation]() { return cancellation(); });
		co_return make<implementation::EditorJournalCheckpoint>(nullptr, std::move(prefix));
	}
} // namespace winrt::WinUIEditor::implementation
