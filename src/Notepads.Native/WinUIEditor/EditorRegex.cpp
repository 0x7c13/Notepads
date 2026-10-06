// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

#include "pch.h"
#include "NativeRegex.h"
#include "EditorWrapper.h"
#include "EditorBaseControl.h"
#include "NativeJournal.h"
#include "Helpers.h"

namespace winrt::WinUIEditor::implementation
{
	using Status = WinUIEditor::EditorSearchStatus;
	namespace
	{
		using SearchResult = WinUIEditor::EditorSearchResult;
		using SearchOperation = Windows::Foundation::IAsyncOperation<SearchResult>;
		using CompletionHandler = Windows::Foundation::AsyncOperationCompletedHandler<SearchResult>;
		using AsyncStatus = Windows::Foundation::AsyncStatus;

		// Cancellation requests stop the worker. Completion is published only after
		// its read lease and staged journal transaction have both been drained.
		struct RegexOperation : implements<RegexOperation, SearchOperation, Windows::Foundation::IAsyncInfo>
		{
			explicit RegexOperation(std::shared_ptr<::WinUIEditor::RegexJob> job) : _job(std::move(job)) {}

			uint32_t Id() const noexcept { return 1; }
			AsyncStatus Status() const noexcept
			{
				std::lock_guard guard(_mutex);
				return _status;
			}
			hresult ErrorCode() const noexcept
			{
				std::lock_guard guard(_mutex);
				return _error;
			}
			void Cancel() noexcept
			{
				std::lock_guard guard(_mutex);
				if (_status == AsyncStatus::Started) _job->canceled.store(true);
			}
			void Close() const noexcept {}
			SearchResult GetResults() const
			{
				std::lock_guard guard(_mutex);
				if (_status == AsyncStatus::Started) throw_hresult(E_ILLEGAL_METHOD_CALL);
				check_hresult(_error);
				return _result;
			}
			CompletionHandler Completed() const
			{
				std::lock_guard guard(_mutex);
				return _completed ? _completed.get() : nullptr;
			}
			void Completed(CompletionHandler const &handler)
			{
				AsyncStatus status;
				{
					std::lock_guard guard(_mutex);
					if (_handlerAssigned) throw_hresult(E_ILLEGAL_DELEGATE_ASSIGNMENT);
					_handlerAssigned = true;
					status = _status;
					if (status == AsyncStatus::Started)
					{
						_completed = handler ? make_agile(handler) : nullptr;
						return;
					}
				}
				if (handler) handler(*this, status);
			}
			void Finish(SearchResult result, hresult error = S_OK) noexcept
			{
				CompletionHandler handler{nullptr};
				AsyncStatus status;
				{
					std::lock_guard guard(_mutex);
					_result = result;
					_error = result.Status == Status::Canceled ? hresult{HRESULT_FROM_WIN32(ERROR_CANCELLED)} : error;
					_status = _error == HRESULT_FROM_WIN32(ERROR_CANCELLED) ? AsyncStatus::Canceled
						: FAILED(_error) ? AsyncStatus::Error : AsyncStatus::Completed;
					status = _status;
					try { if (_completed) handler = _completed.get(); }
					catch (...) {}
					_completed = nullptr;
				}
				try { if (handler) handler(*this, status); }
				catch (...) {}
			}

		private:
			std::shared_ptr<::WinUIEditor::RegexJob> _job;
			mutable std::mutex _mutex;
			AsyncStatus _status{AsyncStatus::Started};
			hresult _error{S_OK};
			SearchResult _result{};
			agile_ref<CompletionHandler> _completed;
			bool _handlerAssigned{};
		};

		fire_and_forget CompleteRegexAsync(com_ptr<RegexOperation> operation, SearchOperation worker)
		{
			try { operation->Finish(co_await worker); }
			catch (...) { operation->Finish({Status::Failed, -1, -1, 0, 0, -1}, to_hresult()); }
		}

		struct ComScope
		{
			HRESULT result{CoInitializeEx(nullptr, COINIT_MULTITHREADED)};
			ComScope() { check_hresult(result); }
			~ComScope() { if (SUCCEEDED(result)) CoUninitialize(); }
		};
		::WinUIEditor::SplitText Split(Scintilla::Internal::SplitView view) noexcept
		{
			return {{view.segment1, view.length1}, {view.segment2 + view.length1, view.length - view.length1}};
		}
		void CheckMemory(uint64_t required)
		{
			const auto limit = Windows::System::MemoryManager::AppMemoryUsageLimit();
			const auto used = Windows::System::MemoryManager::AppMemoryUsage();
			const auto headroom = std::max(uint64_t{32 * 1024 * 1024}, limit / 8);
			if (used >= limit || headroom >= limit - used || required > limit - used - headroom)
				throw ::WinUIEditor::RegexFailure{Status::ResourceLimit};
		}
		struct Candidate
		{
			std::unique_ptr<Scintilla::Internal::Document> document;
			std::unique_ptr<Scintilla::Internal::IContractionState> contraction;
			std::string previousText;
			std::string pending;
			uint64_t baselineBytes{};
			uint64_t undoBytes{};

			Candidate(Scintilla::Internal::Document const &source)
				: baselineBytes(source.LengthNoExcept()), undoBytes(source.UndoMemoryUsage())
			{
				CheckMemory(undoBytes + baselineBytes * 3 + uint64_t{8 * 1024 * 1024});
				document = std::make_unique<Scintilla::Internal::Document>(source.Options());
				document->SetUndoCollection(false);
				document->eolMode = Scintilla::EndOfLine::Cr;
				document->AllocateLineCharacterIndex(Scintilla::LineCharacterIndexType::Utf16);
				pending.reserve(65540);
			}
			void Flush(bool final, ::WinUIEditor::RegexJob const &job)
			{
				job.Check();
				size_t count = pending.size();
				if (!final)
				{
					count = std::min(count, size_t{65536});
					while (count < pending.size() && count && (static_cast<uint8_t>(pending[count]) & 0xc0) == 0x80) --count;
					// A block may end inside a scalar. Keep that scalar for the next span.
					if (count == pending.size() && count)
					{
						auto start = count - 1;
						while (start && (static_cast<uint8_t>(pending[start]) & 0xc0) == 0x80) --start;
						auto lead = static_cast<uint8_t>(pending[start]);
						auto width = lead < 0x80 ? 1u : lead < 0xe0 ? 2u : lead < 0xf0 ? 3u : 4u;
						if (count - start < width) count = start;
					}
				}
				if (!count) return;
				if (static_cast<uint64_t>(document->LengthNoExcept()) + count > INT32_MAX)
					throw ::WinUIEditor::RegexFailure{Status::ResourceLimit};
				// Candidate capacity/styles/index growth plus the remaining undo
				// clone, old notification bytes and new undo payload are admitted.
				CheckMemory(undoBytes + baselineBytes * 2 + (static_cast<uint64_t>(document->LengthNoExcept()) + count) * 6 + 8 * 1024 * 1024);
				auto status = document->AddData(pending.data(), static_cast<Sci_Position>(count));
				if (status != static_cast<int>(Scintilla::Status::Ok))
					throw ::WinUIEditor::RegexFailure{Status::ResourceLimit};
				pending.erase(0, count);
			}
			void Emit(std::string_view bytes, ::WinUIEditor::RegexJob const &job)
			{
				while (!bytes.empty())
				{
					job.Check();
					auto count = std::min(bytes.size(), size_t{65536} - pending.size());
					pending.append(bytes.data(), count);
					bytes.remove_prefix(count);
					if (pending.size() == 65536) Flush(false, job);
				}
			}
			void Prepare(Scintilla::Internal::Document const &source, ::WinUIEditor::RegexJob const &job)
			{
				Flush(true, job);
				auto check = [&]() { job.Check(); CheckMemory(1024 * 1024); };
				source.PrepareLeasedReplacement(*document, check);
				contraction = Scintilla::Internal::ContractionStateCreate(source.IsLarge());
				contraction->InsertLines(0, source.LinesTotal() - 1);
				auto view = Split(source.AllView());
				previousText.reserve(static_cast<size_t>(baselineBytes));
				for (auto segment : {view.first, view.second})
				{
					while (!segment.empty())
					{
						check();
						auto count = std::min(size_t{65536}, segment.size());
						previousText.append(segment.data(), count);
						segment.remove_prefix(count);
					}
				}
			}
		};
	}

	uint64_t Editor::DocumentRevision()
	{
		auto view = _editor.get();
		if (!view || view->IsFinalized()) throw_hresult(RO_E_CLOSED);
		return view->DocumentRevision();
	}
	uint64_t Editor::SelectionRevision()
	{
		auto view = _editor.get();
		if (!view || view->IsFinalized()) throw_hresult(RO_E_CLOSED);
		return view->SelectionRevision();
	}
	Windows::Foundation::IAsyncOperation<WinUIEditor::EditorSearchResult> Editor::FindRegexAsync(
		hstring pattern, bool matchCase, int64_t origin, bool previous, bool wrap, int64_t excludedEmpty)
	{
		auto job = std::make_shared<::WinUIEditor::RegexJob>();
		auto operation = make_self<RegexOperation>(job);
		if (pattern.size() > 32768)
		{
			operation->Finish({Status::ResourceLimit, -1, -1, 0, 0, -1});
			return operation.as<SearchOperation>();
		}
		::WinUIEditor::RegexRequest request{std::wstring(pattern), {}, matchCase, previous, wrap, false, false, origin, excludedEmpty};
		CompleteRegexAsync(operation, RunRegexOperationAsync(std::move(request), std::move(job)));
		return operation.as<SearchOperation>();
	}
	Windows::Foundation::IAsyncOperation<WinUIEditor::EditorSearchResult> Editor::ReplaceRegexAsync(
		hstring pattern, bool matchCase, hstring replacement, int64_t origin, bool previous, bool replaceAll)
	{
		auto job = std::make_shared<::WinUIEditor::RegexJob>();
		auto operation = make_self<RegexOperation>(job);
		if (pattern.size() > 32768 || replacement.size() > 32768)
		{
			operation->Finish({Status::ResourceLimit, -1, -1, 0, 0, -1});
			return operation.as<SearchOperation>();
		}
		::WinUIEditor::RegexRequest request{std::wstring(pattern), std::wstring(replacement), matchCase, previous, false, true, replaceAll, origin};
		if (replaceAll) { request.previous = false; request.origin = 0; }
		CompleteRegexAsync(operation, RunRegexOperationAsync(std::move(request), std::move(job)));
		return operation.as<SearchOperation>();
	}
	Windows::Foundation::IAsyncOperation<WinUIEditor::EditorSearchResult> Editor::RunRegexOperationAsync(
		::WinUIEditor::RegexRequest request, std::shared_ptr<::WinUIEditor::RegexJob> job)
	{
		auto lifetime = get_strong();
		auto view = _editor.get();
		WinUIEditor::EditorSearchResult result{Status::Stale, -1, -1, 0, 0, -1};
		if (!view || view->IsFinalized()) co_return result;
		if (!view->Dispatcher().HasThreadAccess()) throw_hresult(RPC_E_WRONG_THREAD);
		if (_regexJob || _textLoadState) co_return result;
		if (CodePage() != 65001 || request.origin < 0 || request.origin > Length()) throw hresult_invalid_argument();
		if (request.replace && (ReadOnly() || !view->IsEnabled())) { result.Status = Status::ReadOnly; co_return result; }
		_regexJob = job;
		struct ActiveJob
		{
			std::shared_ptr<::WinUIEditor::RegexJob> &active;
			DWORD threadId{GetCurrentThreadId()};
			~ActiveJob() { if (GetCurrentThreadId() == threadId) active.reset(); }
		} active{_regexJob};
		auto owner = apartment_context();
		const auto owningThreadId = GetCurrentThreadId();
		bool disconnected = false;
		view->AcquireReadLease();
		struct ReadLease
		{
			com_ptr<EditorBaseControl> view;
			DWORD ownerThread;
			bool &disconnected;
			~ReadLease()
			{
				if (GetCurrentThreadId() != ownerThread && !disconnected) std::terminate();
				view->ReleaseReadLease();
			}
		} lease{view, owningThreadId, disconnected};
		auto revision = view->DocumentRevision();
		auto selectionRevision = view->SelectionRevision();
		auto const &document = view->LeasedDocument();
		if (document.LengthNoExcept() > INT32_MAX) { result.Status = Status::ResourceLimit; co_return result; }
		auto source = Split(document.AllView());
		std::shared_ptr<::WinUIEditor::NativeDocumentJournal> journal;
		if (request.replace) journal = view->PrepareJournalReplacement();
		struct JournalPreparation
		{
			std::shared_ptr<::WinUIEditor::NativeDocumentJournal> journal;
			~JournalPreparation() { if (journal && journal->PreparingReplacement()) journal->AbandonReplacement(); }
		} preparation{journal};
		std::unique_ptr<Candidate> candidate;
		co_await resume_background();
		try
		{
			ComScope com;
			result = ::WinUIEditor::RunRegex(source, request, *job, [&](auto bytes)
			{
				if (!candidate) candidate = std::make_unique<Candidate>(document);
				candidate->Emit(bytes, *job);
			});
			if (request.replace && result.Status == Status::Found)
			{
				if (!candidate) candidate = std::make_unique<Candidate>(document);
				candidate->Prepare(document, *job);
				if (journal) journal->StageReplacement(candidate->document->AllView(), [&]() { job->Check(); });
			}
		}
		catch (::WinUIEditor::RegexFailure const &failure) { result.Status = failure.status; result.ErrorOffset = failure.offset; }
		catch (std::bad_alloc const &) { result.Status = Status::ResourceLimit; }
		catch (...) { result.Status = Status::Failed; }
		result.Revision = revision;
		bool committed = false;
		try
		{
			co_await owner;
			if (request.replace && result.Status == Status::Found && !view->IsFinalized())
				co_await view->WaitForTextStoreUnlockAsync();
			if (job->canceled.load()) result.Status = Status::Canceled;
			else if (view->IsFinalized() || revision != view->DocumentRevision() || selectionRevision != view->SelectionRevision()) result.Status = Status::Stale;
			else if (request.replace && !view->IsEnabled()) result.Status = Status::ReadOnly;
			else if (result.Status == Status::Found && job->Interruption() != Status::Found) result.Status = job->Interruption();
			if (request.replace && result.Status == Status::Found)
			{
				view->PublishPreparedReplacement(*candidate->document, candidate->previousText.data(), std::move(candidate->contraction), true);
				committed = true;
				if (journal) journal->CommitReplacement();
				result.Revision = view->DocumentRevision();
				result.SelectionRevision = view->SelectionRevision();
			}
		}
		catch (hresult_error const &error)
		{
			disconnected = ::WinUIEditor::IsApartmentDisconnected(error.code());
			if (!disconnected && GetCurrentThreadId() != owningThreadId) std::terminate();
			result.Status = disconnected ? Status::Canceled : Status::Failed;
		}
		catch (...)
		{
			if (GetCurrentThreadId() != owningThreadId) std::terminate();
			result.Status = Status::Failed;
		}
		if (!committed && journal)
		{
			co_await resume_background();
			journal->AbortReplacementOnWorker();
			if (!disconnected)
			{
				try { co_await owner; }
				catch (hresult_error const &error)
				{
					if (!::WinUIEditor::IsApartmentDisconnected(error.code())) std::terminate();
					disconnected = true;
				}
			}
			journal->FinishReplacement();
		}
		if (disconnected) view->FinalizeAfterApartmentClosed();
		co_return result;
	}
}
