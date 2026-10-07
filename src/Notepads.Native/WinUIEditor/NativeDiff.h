// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

#pragma once
#include <atomic>
#include <chrono>
#include <cstdint>
#include <string_view>
#include <vector>

namespace WinUIEditor
{
	struct DiffText
	{
		std::string_view first;
		std::string_view second;
		size_t Length() const noexcept { return first.size() + second.size(); }
		std::string_view SpanFrom(size_t position) const noexcept
		{
			return position < first.size() ? first.substr(position) : second.substr(position - first.size());
		}
		uint8_t At(size_t position) const noexcept
		{
			return static_cast<uint8_t>(position < first.size() ? first[position] : second[position - first.size()]);
		}
	};
	struct DiffHunk { uint32_t oldStart{}, oldCount{}, newStart{}, newCount{}; };
	enum class DiffReason { None, Memory, LineLimit, TimeBudget, WorkBudget, RefinementBudget };
	struct DiffInlineRange { uint32_t oldStart{}, oldLength{}, newStart{}, newLength{}; };
	constexpr uint32_t MaximumDiffLines = 1000000;
	struct DiffOptions
	{
		uint32_t maximumLines{MaximumDiffLines}; // Combined, including terminal logical lines.
		uint32_t maximumHunks{8192};
		uint32_t maximumEdits{256};
		uint64_t maximumWork{40000000};
		uint64_t hashMask{UINT64_MAX}; // Collision seam for deterministic native tests.
	};
	struct DiffJob
	{
		std::atomic<bool> canceled{};
		std::chrono::steady_clock::time_point deadline{std::chrono::steady_clock::now() + std::chrono::seconds(3)};
	};
	struct DiffCanceled {};
	struct DiffUnavailable { DiffReason reason{DiffReason::LineLimit}; };
	struct DiffResult
	{
		uint32_t oldLines{}, newLines{};
		uint32_t addedLines{}, deletedLines{}; // Changed source lines, excluding the terminal empty logical line.
		bool coarse{};
		DiffReason reason{};
		std::vector<DiffHunk> hunks;
		std::vector<DiffInlineRange> inlineRanges;
	};
	DiffResult CompareText(DiffText oldText, DiffText newText, DiffJob const &job, DiffOptions const &options = {});
}
