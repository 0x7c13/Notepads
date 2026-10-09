// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

#include <algorithm>
#include <cstdint>
#include <limits>
#include <memory>
#include <stdexcept>
#include <vector>
#include "Position.h"
#include "ContractionState.h"
#include "DisplayLineMap.h"

namespace Scintilla::Internal
{
	namespace
	{
		class DisplayLineMap final : public IContractionState
		{
			struct Gap { Sci::Line line, start, end, cumulative; };
			std::unique_ptr<IContractionState> _base;
			std::vector<Gap> _gaps;
			std::vector<TintedLineRange> _tints;
			unsigned int _lineColour{}, _gapColour{}, _gapHatchColour{};
			Sci::Line _padding{};
			DisplayHighlight _highlight{};
			void Retire() noexcept { _gaps.clear(); _tints.clear(); _padding = 0; _highlight = {}; }
			auto GapAt(Sci::Line row) const noexcept
			{
				auto found = std::upper_bound(_gaps.begin(), _gaps.end(), row,
					[](Sci::Line value, Gap const &gap) { return value < gap.start; });
				return found == _gaps.begin() ? _gaps.end() : found - 1;
			}
		public:
			DisplayLineMap(std::unique_ptr<IContractionState> base, std::vector<DisplayGap> gaps, std::vector<TintedLineRange> tints)
				: _base(std::move(base)), _tints(std::move(tints))
			{
				if (!_base || _base->HiddenLines() || _base->LinesDisplayed() != _base->LinesInDoc())
					throw std::invalid_argument("Display padding requires an unwrapped, unfolded view.");
				Sci::Line previousLine = -1;
				_gaps.reserve(gaps.size());
				for (auto const gap : gaps)
				{
					if (gap.beforeLine <= previousLine || gap.beforeLine > _base->LinesInDoc() || gap.rows <= 0 ||
						gap.rows > INT32_MAX - _base->LinesDisplayed() - _padding)
						throw std::invalid_argument("Invalid display padding.");
					const auto start = _base->DisplayFromDoc(gap.beforeLine) + _padding;
					_padding += gap.rows;
					_gaps.push_back({gap.beforeLine, start, start + gap.rows, _padding});
					previousLine = gap.beforeLine;
				}
				Sci::Line previousEnd = 0;
				for (auto const range : _tints)
				{
					if (range.start < previousEnd || range.count <= 0 || range.start > _base->LinesInDoc() - range.count)
						throw std::invalid_argument("Invalid display tint range.");
					previousEnd = range.start + range.count;
				}
			}
			void Clear() override { Retire(); _base->Clear(); }
			Sci::Line LinesInDoc() const noexcept override { return _base->LinesInDoc(); }
			Sci::Line LinesDisplayed() const noexcept override { return _base->LinesDisplayed() + _padding; }
			Sci::Line DisplayFromDoc(Sci::Line line) const noexcept override
			{
				auto found = std::upper_bound(_gaps.begin(), _gaps.end(), line,
					[](Sci::Line value, Gap const &gap) { return value < gap.line; });
				return _base->DisplayFromDoc(line) + (found == _gaps.begin() ? 0 : (found - 1)->cumulative);
			}
			Sci::Line DisplayFromDocSub(Sci::Line line, Sci::Line sub) const noexcept override
			{
				return DisplayFromDoc(line) + std::min(sub, static_cast<Sci::Line>(GetHeight(line) - 1));
			}
			Sci::Line DisplayLastFromDoc(Sci::Line line) const noexcept override { return DisplayFromDoc(line) + GetHeight(line) - 1; }
			Sci::Line DocFromDisplay(Sci::Line row) const noexcept override
			{
				const auto found = GapAt(row);
				if (found == _gaps.end()) return _base->DocFromDisplay(row);
				if (row < found->end) return std::min(found->line, LinesInDoc() - 1);
				return _base->DocFromDisplay(row - found->cumulative);
			}
			bool IsDisplayGap(Sci::Line row) const noexcept override
			{
				const auto found = GapAt(row);
				return found != _gaps.end() && row < found->end;
			}
			Sci::Line GapBoundaryFromDisplay(Sci::Line row) const noexcept override
			{
				const auto found = GapAt(row);
				return found != _gaps.end() && row < found->end ? found->line : DocFromDisplay(row);
			}
			Sci::Line NextTextDisplayLine(Sci::Line row, int direction) const noexcept override
			{
				const auto found = GapAt(row);
				if (found == _gaps.end() || row >= found->end) return row;
				if (direction < 0) return found->start > 0 ? found->start - 1 : -1;
				return found->line < LinesInDoc() ? found->end : LinesDisplayed();
			}
			unsigned int DisplayGapColour() const noexcept override { return _gapColour; }
			unsigned int DisplayGapHatchColour() const noexcept override { return _gapHatchColour; }
			unsigned int LineTint(Sci::Line line) const noexcept override
			{
				auto found = std::upper_bound(_tints.begin(), _tints.end(), line,
					[](Sci::Line value, TintedLineRange const &range) { return value < range.start; });
				if (found == _tints.begin()) return 0;
				--found;
				return line < found->start + found->count ? _lineColour : 0;
			}
			void SetDisplayColours(unsigned int line, unsigned int gap, unsigned int hatch) noexcept override {
				_lineColour = line;
				_gapColour = gap;
				_gapHatchColour = hatch;
			}
			void InsertLines(Sci::Line line, Sci::Line count) override { Retire(); _base->InsertLines(line, count); }
			DisplayHighlight GetDisplayHighlight() const noexcept override { return _highlight; }
			void SetDisplayHighlight(DisplayHighlight highlight) noexcept override { _highlight = highlight; }
			void DeleteLines(Sci::Line line, Sci::Line count) override { Retire(); _base->DeleteLines(line, count); }
			bool GetVisible(Sci::Line line) const noexcept override { return _base->GetVisible(line); }
			bool SetVisible(Sci::Line start, Sci::Line end, bool visible) override { if (!visible) Retire(); return _base->SetVisible(start, end, visible); }
			bool HiddenLines() const noexcept override { return _base->HiddenLines(); }
			const char *GetFoldDisplayText(Sci::Line line) const noexcept override { return _base->GetFoldDisplayText(line); }
			bool SetFoldDisplayText(Sci::Line line, const char *text) override { return _base->SetFoldDisplayText(line, text); }
			bool GetExpanded(Sci::Line line) const noexcept override { return _base->GetExpanded(line); }
			bool SetExpanded(Sci::Line line, bool expanded) override { if (!expanded) Retire(); return _base->SetExpanded(line, expanded); }
			bool ExpandAll() override { return _base->ExpandAll(); }
			Sci::Line ContractedNext(Sci::Line line) const noexcept override { return _base->ContractedNext(line); }
			int GetHeight(Sci::Line line) const noexcept override { return _base->GetHeight(line); }
			bool SetHeight(Sci::Line line, int height) override { if (height != 1) Retire(); return _base->SetHeight(line, height); }
			void ShowAll() noexcept override { _base->ShowAll(); }
		};
	}
	std::unique_ptr<IContractionState> DisplayLineMapCreate(std::unique_ptr<IContractionState> base,
		std::vector<DisplayGap> gaps, std::vector<TintedLineRange> tints)
	{
		return std::make_unique<DisplayLineMap>(std::move(base), std::move(gaps), std::move(tints));
	}
}
