// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

#include "NativeDiff.h"
#include <algorithm>
#include <cstdlib>
#include <iostream>
#include <memory>
#include <random>
#include <stdexcept>
#include <string>
#include <vector>
#include "Position.h"
#include "ContractionState.h"
#include "DisplayLineMap.h"

namespace Scintilla::Internal::Platform
{
	void Assert(const char *, const char *, int) noexcept { std::abort(); }
}

namespace
{
	using namespace WinUIEditor;
	using namespace Scintilla::Internal;
	void Require(bool value, char const *message) { if (!value) throw std::runtime_error(message); }
	std::vector<std::string_view> Lines(std::string const &text)
	{
		std::vector<std::string_view> lines;
		size_t start = 0;
		for (size_t i = 0; i < text.size(); ++i)
			if (text[i] == '\r') { lines.push_back(std::string_view(text).substr(start, i + 1 - start)); start = i + 1; }
		lines.push_back(std::string_view(text).substr(start));
		return lines;
	}
	DiffText Split(std::string const &text, size_t at) { return {std::string_view(text).substr(0, at), std::string_view(text).substr(at)}; }
	std::unique_ptr<IContractionState> Mapping(uint32_t lines, std::vector<DisplayGap> gaps)
	{
		auto base = ContractionStateCreate(false);
		base->InsertLines(0, lines - 1);
		return DisplayLineMapCreate(std::move(base), std::move(gaps), {});
	}
	void Verify(std::string const &oldText, std::string const &newText, DiffOptions options = {}, size_t oldSplit = 0, size_t newSplit = 0)
	{
		DiffJob job;
		auto result = CompareText(Split(oldText, oldSplit), Split(newText, newSplit), job, options);
		auto oldLines = Lines(oldText), newLines = Lines(newText);
		Require(result.oldLines == oldLines.size() && result.newLines == newLines.size(), "line counts");
		Require(result.hunks.size() <= options.maximumHunks, "hunk budget");
		std::vector<DisplayGap> oldGaps, newGaps;
		for (auto h : result.hunks)
		{
			if (h.newCount > h.oldCount) oldGaps.push_back({h.oldStart + h.oldCount, h.newCount - h.oldCount});
			if (h.oldCount > h.newCount) newGaps.push_back({h.newStart + h.newCount, h.oldCount - h.newCount});
		}
		auto oldMap = Mapping(result.oldLines, oldGaps), newMap = Mapping(result.newLines, newGaps);
		Require(oldMap->LinesDisplayed() == newMap->LinesDisplayed(), "aligned height");
		uint32_t oldCursor = 0, newCursor = 0;
		auto equalThrough = [&](uint32_t oldEnd, uint32_t newEnd)
		{
			Require(oldEnd >= oldCursor && newEnd >= newCursor && oldEnd - oldCursor == newEnd - newCursor, "ordered equal coverage");
			while (oldCursor < oldEnd)
			{
				Require(oldLines[oldCursor] == newLines[newCursor], "byte-exact equal coverage");
				Require(oldMap->DisplayFromDoc(oldCursor) == newMap->DisplayFromDoc(newCursor), "unchanged row alignment");
				++oldCursor; ++newCursor;
			}
		};
		for (auto h : result.hunks)
		{
			Require(h.oldStart + h.oldCount <= oldLines.size() && h.newStart + h.newCount <= newLines.size(), "hunk bounds");
			equalThrough(h.oldStart, h.newStart);
			oldCursor += h.oldCount; newCursor += h.newCount;
		}
		equalThrough(result.oldLines, result.newLines);
		for (uint32_t i = 0; i < result.oldLines; ++i)
			Require(oldMap->DocFromDisplay(oldMap->DisplayFromDoc(i)) == i && !oldMap->IsDisplayGap(oldMap->DisplayFromDoc(i)), "old mapping round trip");
		for (uint32_t i = 0; i < result.newLines; ++i)
			Require(newMap->DocFromDisplay(newMap->DisplayFromDoc(i)) == i, "new mapping round trip");
		for (auto r : result.inlineRanges)
		{
			Require(r.oldStart + r.oldLength <= oldText.size() && r.newStart + r.newLength <= newText.size(), "inline bounds");
			for (auto p : {r.oldStart, r.oldStart + r.oldLength})
				Require(p == oldText.size() || (static_cast<uint8_t>(oldText[p]) & 0xc0) != 0x80, "old UTF-8 boundary");
			for (auto p : {r.newStart, r.newStart + r.newLength})
				Require(p == newText.size() || (static_cast<uint8_t>(newText[p]) & 0xc0) != 0x80, "new UTF-8 boundary");
		}
	}
}

int main()
{
	try
	{
		struct CountCase { char const *oldText, *newText; uint32_t added, deleted; };
		for (auto const &test : {CountCase{"", "", 0, 0}, CountCase{"", "a", 1, 0}, CountCase{"a", "", 0, 1},
			CountCase{"", "\r", 1, 0}, CountCase{"\r", "", 0, 1}, CountCase{"a", "a\r", 1, 1},
			CountCase{"a\r", "a", 1, 1}, CountCase{"a\r", "a\r\r", 1, 0}, CountCase{"a\r\r", "a\r", 0, 1},
			CountCase{"a\rb", "c\rd", 2, 2}, CountCase{"a\rb\r", "a\rb\rc\r", 1, 0}}) {
			DiffJob job;
			const auto result = CompareText({test.oldText, {}}, {test.newText, {}}, job);
			Require(result.addedLines == test.added && result.deletedLines == test.deleted, "changed physical line totals");
		}
		DiffOptions noWork;
		noWork.maximumWork = 0;
		DiffJob noWorkJob;
		const auto emptyExact = CompareText({{}, {}}, {{}, {}}, noWorkJob, noWork);
		Require(!emptyExact.coarse && emptyExact.hunks.empty() && emptyExact.addedLines == 0 && emptyExact.deletedLines == 0,
			"empty documents remain exact with no refinement budget");
		std::string same(196613, 'a');
		same[65535] = '\r';
		same[65536] = '\0';
		auto restored = same;
		restored.back() = 'b';
		restored.back() = 'a'; // A manual edit/revert does not necessarily return to a save point.
		DiffJob equalityJob;
		const auto equalResult = CompareText(Split(same, 65537), Split(restored, 131071), equalityJob, noWork);
		Require(!equalResult.coarse && equalResult.hunks.empty() && equalResult.oldLines == 2 && equalResult.newLines == 2,
			"split-buffer whole-text equality is independent of refinement budget");
		restored.back() = 'b';
		Verify(same, restored, noWork, 65537, 131071); // The proof must check beyond every split/chunk boundary.
		for (auto const &a : {std::string{}, std::string{"a"}, std::string{"a\r"}, std::string{"\r"}, std::string{"a\rb\r"}})
			for (auto const &b : {std::string{}, std::string{"a"}, std::string{"a\r"}, std::string{"\r"}, std::string{"x\ra\rb\r"}})
				for (size_t split = 0; split <= a.size(); ++split) Verify(a, b, {}, split, b.size() / 2);
		Verify("\xe4\xbd\xa0\xe5\xa5\xbd\rArabic: \xd8\xa7\r", "\xe4\xbd\xa0\xe4\xbb\xac\rArabic: \xd8\xa8\r");
		Verify(std::string("a\0b\r", 4), std::string("a\0c\r", 4));
		std::mt19937 random(72951);
		for (int iteration = 0; iteration < 1500; ++iteration)
		{
			std::string a, b;
			for (int i = 0, n = random() % 35; i < n; ++i) a += std::to_string(random() % 9) + '\r';
			for (int i = 0, n = random() % 35; i < n; ++i) b += std::to_string(random() % 9) + '\r';
			if (random() % 2 && !a.empty()) a.pop_back();
			if (random() % 2 && !b.empty()) b.pop_back();
			DiffOptions options;
			if (iteration % 3 == 0) options.hashMask = 0;
			if (iteration % 5 == 0) options.maximumWork = 20;
			if (iteration % 7 == 0) options.maximumHunks = 1;
			if (iteration % 11 == 0) options.maximumEdits = 0;
			Verify(a, b, options, a.size() / 2, b.size() / 3);
		}
		auto map = Mapping(3, {{0, 40000}, {2, 60000}, {3, 70000}});
		Require(map->LinesDisplayed() == 170003 && map->DisplayFromDoc(0) == 40000 && map->DisplayFromDoc(3) == 170003, "wide BOF/EOF counts");
		Require(map->GetHeight(0) == 1 && map->IsDisplayGap(0) && map->IsDisplayGap(170002), "gaps separate from text height");
		Require(map->NextTextDisplayLine(40002, -1) == 40001 && map->NextTextDisplayLine(40002, 1) == 100002, "caret skips whole gap");
		Require(map->NextTextDisplayLine(0, -1) == -1 && map->NextTextDisplayLine(170002, 1) == 170003, "navigation outside source uses boundary sentinels");
		Require(map->DocFromDisplay(170003) == 3, "EOF sentinel");
		Require(map->GapBoundaryFromDisplay(0) == 0 && map->GapBoundaryFromDisplay(40002) == 2 &&
			map->GapBoundaryFromDisplay(170002) == 3, "BOF/internal/EOF source boundaries");
		DiffJob canceled;
		canceled.canceled = true;
		try { CompareText({"a", {}}, {"b", {}}, canceled); Require(false, "cancellation missing"); } catch (DiffCanceled const &) {}
		DiffJob expired;
		expired.deadline = std::chrono::steady_clock::now();
		try { CompareText({"a", {}}, {"b", {}}, expired); Require(false, "index deadline missing"); } catch (DiffUnavailable const &) {}
		try { CompareText({{}, {}}, {{}, {}}, expired, noWork); Require(false, "empty equality deadline missing"); }
		catch (DiffUnavailable const &failure) { Require(failure.reason == DiffReason::TimeBudget, "equality deadline reason"); }
		try { CompareText({{}, {}}, {{}, {}}, canceled, noWork); Require(false, "empty equality cancellation missing"); }
		catch (DiffCanceled const &) {}
		std::cout << "Native diff: exact/coarse coverage, collision, Unicode, alignment and cancellation checks passed.\n";
		return 0;
	}
	catch (std::exception const &error) { std::cerr << error.what() << '\n'; return 1; }
}
