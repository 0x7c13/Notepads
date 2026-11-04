// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

#pragma once

#include <algorithm>
#include <cstddef>
#include <iterator>
#include <string_view>
#include <vector>

#include "UniConversion.h"

namespace Scintilla::Internal {

// Borrows immutable text owned by the containing layout. Checkpoints retain
// character boundaries, including the old UTF16Length behavior inside a character.
class Utf8PositionIndex {
	struct Checkpoint { size_t byte; size_t utf16; };
	static constexpr size_t stride = 256;
	std::string_view text;
	std::vector<Checkpoint> checkpoints;
public:
	explicit Utf8PositionIndex(std::string_view source = {}) : text(source) {
		if (std::all_of(text.begin(), text.end(), [](unsigned char ch) { return ch < 0x80; }))
			return;
		checkpoints.reserve(text.size() / stride + 1);
		checkpoints.push_back({0, 0});
		size_t utf16 = 0;
		for (size_t byte = 0; byte < text.size();) {
			const auto width = UTF8BytesOfLead[static_cast<unsigned char>(text[byte])];
			const size_t next = byte + width;
			if (next / stride > byte / stride && next / stride <= text.size() / stride)
				checkpoints.push_back({byte, utf16});
			utf16 += next > text.size() ? 1 : UTF16LengthFromUTF8ByteCount(width);
			byte = next;
		}
	}

	// The optional work count lets tests reject full-prefix rescans independently
	// of wall-clock timing. Production callers need only the UTF-16 position.
	size_t Utf16Position(size_t position, size_t *bytesScanned = nullptr) const noexcept {
		position = std::min(position, text.size());
		if (checkpoints.empty()) {
			if (bytesScanned) *bytesScanned = 0;
			return position;
		}
		const auto checkpoint = checkpoints[position / stride];
		const auto suffix = text.substr(checkpoint.byte, position - checkpoint.byte);
		if (bytesScanned) *bytesScanned = suffix.size();
		return checkpoint.utf16 + UTF16Length(suffix);
	}

	size_t BytePosition(size_t position, size_t *bytesScanned = nullptr) const noexcept {
		if (checkpoints.empty()) {
			if (bytesScanned) *bytesScanned = 0;
			return std::min(position, text.size());
		}
		const auto after = std::upper_bound(checkpoints.begin(), checkpoints.end(), position,
			[](size_t value, const Checkpoint &checkpoint) { return value < checkpoint.utf16; });
		const auto checkpoint = *std::prev(after);
		const auto suffix = text.substr(checkpoint.byte);
		const auto offset = UTF8PositionFromUTF16Position(suffix, position - checkpoint.utf16);
		if (bytesScanned) *bytesScanned = std::min(offset, suffix.size());
		return checkpoint.byte + offset;
	}
};

}
