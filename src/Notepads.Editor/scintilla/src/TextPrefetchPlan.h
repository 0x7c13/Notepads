// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

#pragma once

#include <array>
#include <cstdint>
#include <string_view>

namespace Scintilla::Internal {

// A small startup plan for expensive pictographic source runs. It does not
// depend on viewport proximity, so a later semantic jump can reuse real pixels.
class TextPrefetchPlan {
public:
	static constexpr size_t MaximumBytes = 64 * 1024;
	static constexpr int MaximumLines = 4096;
	struct Candidate {
		int64_t line = -1;
		int bytes = 0;
		unsigned weight = 0;
	};
	std::array<Candidate, 2> candidates{};
	static unsigned Weight(std::string_view text) noexcept {
		unsigned count = 0;
		for (size_t i = 0; i + 3 < text.size(); ++i) {
			// Valid four-byte U+1F000..U+1FFFF sequences cover the common
			// supplementary pictographs, without priming musical/math/CJK runs.
			if (static_cast<unsigned char>(text[i]) == 0xF0 &&
				static_cast<unsigned char>(text[i + 1]) == 0x9F &&
				(static_cast<unsigned char>(text[i + 2]) & 0xC0) == 0x80 &&
				(static_cast<unsigned char>(text[i + 3]) & 0xC0) == 0x80) {
				++count;
				i += 3;
			}
		}
		return count;
	}
	bool Consider(int64_t line, int bytes, unsigned weight) noexcept {
		if (weight < 8 || bytes <= 0 || bytes > 4096 || weight <= candidates.back().weight) return false;
		Candidate value{line, bytes, weight};
		if (weight > candidates.front().weight) {
			candidates.back() = candidates.front();
			candidates.front() = value;
		} else {
			candidates.back() = value;
		}
		return true;
	}
};

}
