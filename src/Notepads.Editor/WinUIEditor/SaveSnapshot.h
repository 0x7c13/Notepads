// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

#pragma once

#include <list>

// A forward-only view of the original UTF-8 bytes. All access is on the editor
// apartment. Unchanged ranges borrow offsets, never pointers, from the pinned
// document. Inserts only split a range. Only deleted, unread bytes need a copy.
namespace winrt::WinUIEditor::implementation
{
	class SaveSnapshot final : public Scintilla::Internal::SnapshotObserver
	{
		using Document = Scintilla::Internal::Document;
		using Position = Sci::Position;
		struct Range
		{
			Position start;
			Position length;
			std::unique_ptr<char[]> copy;
			Position consumed{};
		};

		Document &_document;
		std::list<Range> _ranges;
		Position _position{};
		Position _retained{};
		HRESULT _failure{S_OK};

		void SplitAt(Position point)
		{
			for (auto it = _ranges.begin(); it != _ranges.end(); ++it)
			{
				if (it->copy) continue;
				if (point <= it->start) break;
				const auto left = point - it->start;
				if (left < it->length)
				{
					if (_ranges.size() >= 4096) throw_hresult(HRESULT_FROM_WIN32(ERROR_NOT_ENOUGH_QUOTA));
					_ranges.insert(std::next(it), Range{point, it->length - left, {}});
					it->length = left;
					break;
				}
			}
		}

		void Preserve(Range &range)
		{
			// Bound transient memory even if a large select-all/delete races a slow disk.
			constexpr Position maximumRetained = 16 * 1024 * 1024;
			if (range.length > maximumRetained - _retained)
				throw_hresult(HRESULT_FROM_WIN32(ERROR_NOT_ENOUGH_QUOTA));
			range.copy = std::make_unique_for_overwrite<char[]>(static_cast<size_t>(range.length));
			_document.GetCharRange(range.copy.get(), range.start, range.length);
			_retained += range.length;
		}

	public:
		static constexpr Position EagerCopyBytes = 64 * 1024;

		explicit SaveSnapshot(Document &document) : _document(document)
		{
			const auto length = document.Length();
			if (length) _ranges.push_back({0, length, {}});
			if (length && length <= EagerCopyBytes) Preserve(_ranges.front());
			document.AddRef();
		}

		~SaveSnapshot() override
		{
			_document.Release();
		}

		bool ModificationInProgress() const noexcept { return _document.ModificationInProgress(); }

		Windows::Storage::Streams::IBuffer Read(int64_t offset, int32_t maximum, uint64_t length)
		{
			check_hresult(_failure);
			// Scintilla can publish a save-point event between changing its bytes and
			// delivering InsertText/DeleteText. Never advance the cursor in that gap.
			if (ModificationInProgress()) throw_hresult(E_ILLEGAL_METHOD_CALL);
			if (offset != _position || maximum <= 0) throw hresult_invalid_argument();
			const auto count = static_cast<uint32_t>(std::min<uint64_t>(maximum, length - offset));
			Windows::Storage::Streams::Buffer result{count};
			uint32_t written = 0;
			while (written < count)
			{
				if (_ranges.empty()) throw_hresult(E_UNEXPECTED);
				auto &range = _ranges.front();
				const auto take = std::min<Position>(count - written, range.length);
				if (range.copy)
				{
					memcpy(result.data() + written, range.copy.get() + range.consumed, static_cast<size_t>(take));
					range.consumed += take;
				}
				else
				{
					_document.GetCharRange(reinterpret_cast<char *>(result.data() + written), range.start, take);
					range.start += take;
				}
				range.length -= take;
				written += static_cast<uint32_t>(take);
				if (!range.length)
				{
					if (range.copy) _retained -= range.consumed;
					_ranges.pop_front();
				}
			}
			_position += count;
			result.Length(count);
			return result;
		}

		void Observe(const Scintilla::Internal::DocModification &change) noexcept override
		{
			using Scintilla::ModificationFlags;
			if (FAILED(_failure) || _ranges.empty()) return;
			try
			{
				const auto pos = change.position;
				const auto end = pos + change.length;
				if (FlagSet(change.modificationType, ModificationFlags::BeforeDelete))
				{
					SplitAt(pos);
					SplitAt(end);
					for (auto &range : _ranges)
						if (!range.copy && range.start >= pos && range.start < end) Preserve(range);
				}
				else if (FlagSet(change.modificationType, ModificationFlags::InsertText))
				{
					// Offsets still describe the pre-insert view. No text is read here.
					SplitAt(pos);
					for (auto &range : _ranges)
						if (!range.copy && range.start >= pos) range.start += change.length;
				}
				else if (FlagSet(change.modificationType, ModificationFlags::DeleteText))
				{
					for (auto &range : _ranges)
						if (!range.copy && range.start >= end) range.start -= change.length;
				}
			}
			catch (...)
			{
				_failure = to_hresult();
				_ranges.clear();
			}
		}

		void Invalidate() noexcept override
		{
			_failure = HRESULT_FROM_WIN32(ERROR_RETRY);
			_ranges.clear();
		}
	};
}
