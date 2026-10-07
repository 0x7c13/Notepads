// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

#include "pch.h"
#include "EditorWrapper.h"
#include "EditorDiffResult.h"
#include "DrainingAsyncOperation.h"
#include "Helpers.h"
#include "DisplayLineMap.h"

namespace winrt::WinUIEditor::implementation
{
	using DiffStatus = WinUIEditor::EditorDiffStatus;
	namespace
	{
		using DiffOperation = DrainingAsyncOperation<WinUIEditor::EditorDiffResult, ::WinUIEditor::DiffJob>;
		constexpr uint64_t DiffScratchBytes = 64 * 1024 * 1024;
		// A container indicator slot (8..31); lexers own 0..7.
		constexpr int DiffInlineIndicator = 25;
		::WinUIEditor::DiffText Split(Scintilla::Internal::SplitView view) noexcept
		{
			return {{view.segment1, view.length1}, {view.segment2 + view.length1, view.length - view.length1}};
		}
		fire_and_forget CompleteDiff(com_ptr<DiffOperation> operation,
			Windows::Foundation::IAsyncOperation<WinUIEditor::EditorDiffResult> worker)
		{
			try
			{
				auto result = co_await worker;
				const auto error = result.Status() == DiffStatus::Canceled ? hresult{HRESULT_FROM_WIN32(ERROR_CANCELLED)} : hresult{S_OK};
				operation->Finish(std::move(result), error);
			}
			catch (...) { operation->Finish(nullptr, to_hresult()); }
		}
	}
	bool Editor::CanPrepareDiff(uint64_t savedByteLength, uint64_t savedLines)
	{
		auto view = _editor.get();
		if (!view || view->IsFinalized()) throw_hresult(RO_E_CLOSED);
		if (!view->Dispatcher().HasThreadAccess()) throw_hresult(RPC_E_WRONG_THREAD);
		if (savedByteLength > INT32_MAX || savedLines == 0 || savedLines > ::WinUIEditor::MaximumDiffLines ||
			static_cast<uint64_t>(LineCount()) > ::WinUIEditor::MaximumDiffLines - savedLines || Length() > INT32_MAX) return false;
		// Candidate text/gap capacity, optional styles, UTF-16/contraction indices,
		// bounded comparison scratch, projected results and rendering maps.
		const auto bytes = savedByteLength + static_cast<uint64_t>(Length());
		const auto lines = savedLines + static_cast<uint64_t>(LineCount());
		return ::WinUIEditor::HasMemoryHeadroom(bytes * 4 + lines * 160 + DiffScratchBytes);
	}
	Windows::Foundation::IAsyncOperation<WinUIEditor::EditorDiffResult> Editor::CompareAsync(WinUIEditor::Editor other)
	{
		if (!other || get_self<Editor>(other) == this) throw hresult_invalid_argument();
		auto job = std::make_shared<::WinUIEditor::DiffJob>();
		auto operation = make_self<DiffOperation>(job);
		CompleteDiff(operation, RunDiffAsync(std::move(other), std::move(job)));
		return operation.as<Windows::Foundation::IAsyncOperation<WinUIEditor::EditorDiffResult>>();
	}
	Windows::Foundation::IAsyncOperation<WinUIEditor::EditorDiffResult> Editor::RunDiffAsync(
		WinUIEditor::Editor other, std::shared_ptr<::WinUIEditor::DiffJob> job)
	{
		auto lifetime = get_strong();
		auto opposite = get_self<Editor>(other);
		auto left = _editor.get(), right = opposite->_editor.get();
		if (!left || !right || left->IsFinalized() || right->IsFinalized()) throw_hresult(RO_E_CLOSED);
		if (!left->Dispatcher().HasThreadAccess() || !right->Dispatcher().HasThreadAccess()) throw_hresult(RPC_E_WRONG_THREAD);
		if (CodePage() != Scintilla::CpUtf8 || opposite->CodePage() != Scintilla::CpUtf8 || _textLoadState || opposite->_textLoadState)
			throw hresult_invalid_argument();
		auto owner = apartment_context();
		const auto ownerThread = GetCurrentThreadId();
		struct Lease
		{
			com_ptr<EditorBaseControl> view;
			bool held{};
			~Lease() { if (held) view->ReleaseReadLease(); }
			void Acquire() { view->AcquireReadLease(); held = true; }
		} leftLease{left}, rightLease{right};
		leftLease.Acquire();
		rightLease.Acquire();
		const auto oldRevision = left->DocumentRevision(), newRevision = right->DocumentRevision();
		const auto oldLength = static_cast<uint64_t>(Length()), newLength = static_cast<uint64_t>(opposite->Length());
		auto oldText = Split(left->LeasedDocument().AllView()), newText = Split(right->LeasedDocument().AllView());
		DiffStatus status = DiffStatus::Unavailable;
		::WinUIEditor::DiffResult data;
		data.reason = ::WinUIEditor::DiffReason::Memory;
		if (::WinUIEditor::HasMemoryHeadroom(DiffScratchBytes))
		{
			co_await resume_background();
			try
			{
				data = ::WinUIEditor::CompareText(oldText, newText, *job);
				status = data.coarse ? DiffStatus::Coarse : DiffStatus::Exact;
			}
			catch (::WinUIEditor::DiffCanceled const &) { status = DiffStatus::Canceled; }
			catch (::WinUIEditor::DiffUnavailable const &failure) { status = DiffStatus::Unavailable; data.reason = failure.reason; }
			catch (std::bad_alloc const &) { status = DiffStatus::Unavailable; data.reason = ::WinUIEditor::DiffReason::Memory; }
			catch (...) { status = DiffStatus::Failed; }
			try { co_await owner; }
			catch (hresult_error const &error)
			{
				if (!::WinUIEditor::IsApartmentDisconnected(error.code()) || GetCurrentThreadId() == ownerThread) throw;
				left->FinalizeAfterApartmentClosed();
				right->FinalizeAfterApartmentClosed();
				throw hresult_canceled();
			}
		}
		if (job->canceled.load()) status = DiffStatus::Canceled;
		else if (left->IsFinalized() || right->IsFinalized() || oldRevision != left->DocumentRevision() || newRevision != right->DocumentRevision()) status = DiffStatus::Stale;
		co_return make<implementation::EditorDiffResult>(status, std::move(data), oldRevision, newRevision, oldLength, newLength, left->get_weak(), right->get_weak());
	}
	void Editor::ApplyDiffPresentation(WinUIEditor::EditorDiffResult const &result, bool oldSide)
	{
		auto view = _editor.get();
		if (!view || view->IsFinalized()) throw_hresult(RO_E_CLOSED);
		if (!view->Dispatcher().HasThreadAccess()) throw_hresult(RPC_E_WRONG_THREAD);
		if (!result || (result.Status() != DiffStatus::Exact && result.Status() != DiffStatus::Coarse) ||
			!ReadOnly() || WrapMode() != WinUIEditor::Wrap::None ||
			!get_self<implementation::EditorDiffResult>(result)->Matches(view.get(), oldSide) ||
			view->DocumentRevision() != (oldSide ? result.OldRevision() : result.NewRevision()) ||
			static_cast<uint64_t>(Length()) != (oldSide ? result.OldByteLength() : result.NewByteLength())) throw hresult_invalid_argument();
		if (!::WinUIEditor::HasMemoryHeadroom(4 * 1024 * 1024)) throw_hresult(E_OUTOFMEMORY);
		auto const &data = get_self<implementation::EditorDiffResult>(result)->Data();
		std::vector<Scintilla::Internal::DisplayGap> gaps;
		std::vector<Scintilla::Internal::TintedLineRange> tints;
		gaps.reserve(data.hunks.size());
		tints.reserve(data.hunks.size());
		for (auto const &hunk : data.hunks)
		{
			const auto start = oldSide ? hunk.oldStart : hunk.newStart;
			const auto count = oldSide ? hunk.oldCount : hunk.newCount;
			const auto oppositeCount = oldSide ? hunk.newCount : hunk.oldCount;
			// Index admission bounds every source/display count below INT32_MAX,
			// including the 32-bit Scintilla build's signed line representation.
			if (count) tints.push_back({static_cast<Sci::Line>(start), static_cast<Sci::Line>(count)});
			if (oppositeCount > count) gaps.push_back({static_cast<Sci::Line>(start + count), static_cast<Sci::Line>(oppositeCount - count)});
		}
		view->SetDisplayLineMap(std::move(gaps), std::move(tints));
		IndicatorCurrent(DiffInlineIndicator);
		IndicatorClearRange(0, Length());
		IndicSetStyle(DiffInlineIndicator, WinUIEditor::IndicatorStyle::StraightBox);
		IndicSetUnder(DiffInlineIndicator, true);
		for (auto const &range : data.inlineRanges)
		{
			const auto start = oldSide ? range.oldStart : range.newStart;
			const auto length = oldSide ? range.oldLength : range.newLength;
			if (length) IndicatorFillRange(start, length);
		}
	}
	void Editor::SetDiffColours(int32_t line, int32_t inlineColour, int32_t gap, int32_t gapHatch)
	{
		auto view = _editor.get();
		if (!view || view->IsFinalized()) throw_hresult(RO_E_CLOSED);
		if (!view->Dispatcher().HasThreadAccess()) throw_hresult(RPC_E_WRONG_THREAD);
		view->SetDisplayColours(static_cast<unsigned int>(line), static_cast<unsigned int>(gap), static_cast<unsigned int>(gapHatch));
		IndicSetFore(DiffInlineIndicator, inlineColour & 0xffffff);
		const auto alpha = static_cast<WinUIEditor::Alpha>(static_cast<uint32_t>(inlineColour) >> 24);
		IndicSetAlpha(DiffInlineIndicator, alpha);
		IndicSetOutlineAlpha(DiffInlineIndicator, alpha);
	}
	void Editor::DetachDocument()
	{
		auto view = _editor.get();
		if (!view || view->IsFinalized()) return;
		if (!view->Dispatcher().HasThreadAccess()) throw_hresult(RPC_E_WRONG_THREAD);
		if (_textLoadState) throw_hresult(E_ILLEGAL_METHOD_CALL);
		// SetDocPointer performs the native read-lease and journal checks and retires
		// the owned document, lexer and display map. Large buffers need not wait for GC.
		view->PublicWndProc(Scintilla::Message::SetDocPointer, 0, 0);
	}
}
