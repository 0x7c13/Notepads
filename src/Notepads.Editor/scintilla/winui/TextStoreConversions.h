// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

#pragma once

#include <windows.h>
#include <textstor.h>
#include <cmath>
#include <cstdint>
#include <limits>

namespace Scintilla::Internal::TextStoreConversions {

static_assert(sizeof(LONG) == sizeof(std::int32_t));
inline constexpr std::int64_t MaximumAcp = (std::numeric_limits<LONG>::max)();

// ACPs count UTF-16 code units. Invalid positions must not become valid ones
// through truncation or saturation. Failed conversions leave outputs unchanged.
inline constexpr bool TryAcp(std::int64_t value, LONG &result) noexcept {
	if (value < 0 || value > MaximumAcp) return false;
	result = static_cast<LONG>(value);
	return true;
}

inline constexpr bool TryAcpAdd(LONG start, std::int64_t delta, LONG &result) noexcept {
	if (start < 0 || delta < -static_cast<std::int64_t>(start) || delta > MaximumAcp - start) return false;
	result = static_cast<LONG>(start + delta);
	return true;
}

inline constexpr bool ValidRange(LONG start, LONG end, std::int64_t length) noexcept {
	return length >= 0 && length <= MaximumAcp && start >= 0 && end >= start && end <= length;
}

struct Replacement {
	LONG end;
	LONG length;
	// Additional deletion used by the overtype boundary fallback.
	LONG overwritten{};
};

inline constexpr bool TryReplacement(std::int64_t length, LONG start, LONG end,
	std::int64_t inserted, Replacement &result) noexcept {
	Replacement replacement{};
	if (!ValidRange(start, end, length) || inserted < 0 ||
		!TryAcpAdd(start, inserted, replacement.end) ||
		!TryAcpAdd(static_cast<LONG>(length - (end - start)), inserted, replacement.length)) return false;
	result = replacement;
	return true;
}

template <typename Document, typename Selection, typename MapAcp>
bool TryInputReplacement(Document &document, const Selection &selection, bool overtype,
	std::int64_t length, LONG start, LONG end, std::int64_t inserted, MapAcp mapAcp, Replacement &result) {
	if (TryReplacement(length, start, end, inserted, result)) return true;
	// Only resolve a character when ordinary replacement would overflow. The
	// common input path must not add another scan from the start of the line.
	// A single caret's deletion cannot justify admitting a multi-caret edit or
	// a virtual-space replacement, whose total growth may be larger.
	LONG newEnd = 0;
	if (!overtype || selection.Count() != 1 || selection.IsRectangular() ||
		!selection.RangeMain().Empty() || selection.RangeMain().caret.VirtualSpace() != 0 ||
		!ValidRange(start, end, length) || start != end || inserted <= 0 ||
		!TryAcpAdd(start, inserted, newEnd)) return false;
	bool withinSurrogatePair = false;
	const auto position = mapAcp(start, &withinSurrogatePair);
	if (withinSurrogatePair || position < 0 || position >= document.Length() ||
		document.IsPositionInLineEnd(position)) return false;
	// Match InsertCharacter/DelChar: overwrite one complete character, but
	// never a line ending. A non-BMP character removes two ACP code units.
	LONG overwritten = 0, overwrittenEnd = 0;
	const auto removed = document.CountUTF16(position, position + document.LenChar(position));
	Replacement replacement{};
	if (!TryAcp(removed, overwritten) || removed == 0 ||
		!TryAcpAdd(start, removed, overwrittenEnd) ||
		!TryReplacement(length, start, overwrittenEnd, inserted, replacement)) return false;
	replacement.overwritten = overwritten;
	result = replacement;
	return true;
}

struct TextChangeResult {
	TS_TEXTCHANGE change;
	LONG insertionStart;
};

inline bool TryTextChange(std::int64_t oldLength, LONG oldEnd, std::int64_t actualEnd,
	std::int64_t removed, std::int64_t inserted, TextChangeResult &result) noexcept {
	LONG newEnd = 0;
	if (!ValidRange(0, oldEnd, oldLength) || removed < 0 || inserted < 0 || !TryAcp(actualEnd, newEnd)) return false;
	LONG start = 0, correctedOldEnd = 0;
	if (TryAcpAdd(newEnd, -inserted, start) && TryAcpAdd(start, removed, correctedOldEnd) && correctedOldEnd <= oldLength) {
		result = {{start, correctedOldEnd, newEnd}, start};
	} else {
		// Other rectangle rows can move the main caret so far that the old
		// interval shifted by the same amount would no longer be a valid ACP.
		// Report the encompassing prefix, but keep the inserted range separate:
		// callers must not treat unchanged prefix text as part of the insertion.
		result = {{0, oldEnd, newEnd}, start};
	}
	return true;
}

inline bool EnclosingRect(double left, double top, double right, double bottom, RECT &result) noexcept {
	if (!std::isfinite(left) || !std::isfinite(top) || !std::isfinite(right) || !std::isfinite(bottom) ||
		left > right || top > bottom) return false;
	left = std::floor(left);
	top = std::floor(top);
	right = std::ceil(right);
	bottom = std::ceil(bottom);
	constexpr double minimum = (std::numeric_limits<LONG>::min)();
	constexpr double maximum = (std::numeric_limits<LONG>::max)();
	if (left < minimum || top < minimum || right > maximum || bottom > maximum) return false;
	result = {static_cast<LONG>(left), static_cast<LONG>(top), static_cast<LONG>(right), static_cast<LONG>(bottom)};
	return true;
}

}
