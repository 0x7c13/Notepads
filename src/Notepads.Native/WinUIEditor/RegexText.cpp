// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

#include "RegexText.h"
#include <algorithm>
#include <array>
#include <climits>
#include <new>
namespace WinUIEditor
{
	namespace
	{
		constexpr int32_t WindowLength = 256;
		struct State
		{
			SplitText source;
			std::array<UChar, WindowLength> units;
			std::array<int64_t, WindowLength + 1> indices;
		};
		State &Data(UText *text) noexcept { return *static_cast<State *>(text->pExtra); }
		State const &Data(UText const *text) noexcept { return *static_cast<State const *>(text->pExtra); }
		bool Trail(uint8_t byte) noexcept { return (byte & 0xc0) == 0x80; }
		int64_t Boundary(SplitText source, int64_t index) noexcept
		{
			index = std::clamp(index, int64_t{0}, source.Length());
			if (index == source.Length()) return index;
			auto start = index;
			const auto limit = std::max(int64_t{0}, index - 3);
			while (start > limit && Trail(source.At(start))) --start;
			if (start == index) return index;
			auto first = source.At(start);
			int width = first >= 0xc2 && first <= 0xdf ? 2 : first >= 0xe0 && first <= 0xef ? 3 : first >= 0xf0 && first <= 0xf4 ? 4 : 0;
			if (!width || start + width <= index || start + width > source.Length()) return index;
			for (int i = 1; i < width; ++i)
			{
				auto byte = source.At(start + i);
				if (!Trail(byte) || (i == 1 && ((first == 0xe0 && byte < 0xa0) || (first == 0xed && byte > 0x9f) || (first == 0xf0 && byte < 0x90) || (first == 0xf4 && byte > 0x8f)))) return index;
			}
			return start;
		}
		UChar32 Decode(SplitText source, int64_t &index) noexcept
		{
			auto first = source.At(index++);
			if (first < 0x80) return first == '\r' ? '\n' : first;
			int count = first >= 0xc2 && first <= 0xdf ? 1 : first >= 0xe0 && first <= 0xef ? 2 : first >= 0xf0 && first <= 0xf4 ? 3 : 0;
			if (!count || source.Length() - index < count) return 0xfffd;
			UChar32 scalar = first & (0x7f >> (count + 1));
			for (int i = 0; i < count; ++i)
			{
				auto byte = source.At(index + i);
				if (!Trail(byte) || (i == 0 && ((first == 0xe0 && byte < 0xa0) || (first == 0xed && byte > 0x9f) || (first == 0xf0 && byte < 0x90) || (first == 0xf4 && byte > 0x8f)))) return 0xfffd;
				scalar = (scalar << 6) | (byte & 0x3f);
			}
			index += count;
			return scalar;
		}
		int32_t U_CALLCONV MapNative(UText const *text, int64_t index) noexcept
		{
			auto const &indices = Data(text).indices;
			auto found = std::lower_bound(indices.begin(), indices.begin() + text->chunkLength + 1, index);
			if (found == indices.begin() + text->chunkLength + 1) return text->chunkLength;
			if (*found == index) return static_cast<int32_t>(found - indices.begin());
			--found;
			while (found != indices.begin() && found[-1] == *found) --found;
			return static_cast<int32_t>(found - indices.begin());
		}
		int64_t U_CALLCONV MapOffset(UText const *text) noexcept { return Data(text).indices[static_cast<size_t>(text->chunkOffset)]; }
		int64_t U_CALLCONV Length(UText *text) noexcept { return Data(text).source.Length(); }
		UBool U_CALLCONV Access(UText *text, int64_t requested, UBool forward) noexcept
		{
			auto &state = Data(text);
			auto index = Boundary(state.source, requested);
			if ((forward ? text->chunkNativeStart <= index && index < text->chunkNativeLimit : text->chunkNativeStart < index && index <= text->chunkNativeLimit))
			{
				text->chunkOffset = MapNative(text, index);
				return true;
			}
			auto start = Boundary(state.source, std::max(int64_t{0}, index - (forward ? WindowLength / 2 : WindowLength - 4)));
			text->chunkNativeStart = start;
			int32_t count = 0;
			while (start < state.source.Length() && count < WindowLength - 1)
			{
				auto native = start;
				auto scalar = Decode(state.source, start);
				state.indices[static_cast<size_t>(count)] = native;
				if (scalar > 0xffff)
				{
					state.units[static_cast<size_t>(count++)] = static_cast<UChar>(0xd800 + ((scalar - 0x10000) >> 10));
					state.indices[static_cast<size_t>(count)] = native;
					state.units[static_cast<size_t>(count++)] = static_cast<UChar>(0xdc00 + ((scalar - 0x10000) & 0x3ff));
				}
				else state.units[static_cast<size_t>(count++)] = static_cast<UChar>(scalar);
			}
			state.indices[static_cast<size_t>(count)] = start;
			text->chunkContents = state.units.data();
			text->chunkLength = count;
			text->chunkNativeLimit = start;
			text->nativeIndexingLimit = 0;
			text->chunkOffset = MapNative(text, index);
			return forward ? index < state.source.Length() : index > 0;
		}
		UText *U_CALLCONV Clone(UText *dest, UText const *source, UBool deep, UErrorCode *status) noexcept
		{
			if (U_FAILURE(*status)) return nullptr;
			if (deep) { *status = U_UNSUPPORTED_ERROR; return nullptr; }
			dest = OpenSplitText(dest, Data(source).source, status);
			if (dest) Access(dest, MapOffset(source), true);
			return dest;
		}
		int32_t U_CALLCONV Extract(UText *text, int64_t start, int64_t limit, UChar *output, int32_t capacity, UErrorCode *status) noexcept
		{
			if (U_FAILURE(*status)) return 0;
			auto source = Data(text).source;
			if (start < 0 || limit < start || limit > source.Length() || capacity < 0 || (!output && capacity)) { *status = U_ILLEGAL_ARGUMENT_ERROR; return 0; }
			start = Boundary(source, start); limit = Boundary(source, limit);
			int32_t count = 0;
			while (start < limit)
			{
				auto scalar = Decode(source, start);
				if (scalar > 0xffff)
				{
					if (count < capacity) output[count] = static_cast<UChar>(0xd800 + ((scalar - 0x10000) >> 10));
					++count;
					scalar = 0xdc00 + ((scalar - 0x10000) & 0x3ff);
				}
				if (count < capacity) output[count] = static_cast<UChar>(scalar);
				++count;
			}
			if (count < capacity) output[count] = 0;
			else if (count > capacity) *status = U_BUFFER_OVERFLOW_ERROR;
			else *status = U_STRING_NOT_TERMINATED_WARNING;
			Access(text, limit, true);
			return count;
		}
		UTextFuncs const Functions = {sizeof(UTextFuncs), 0, 0, 0, Clone, Length, Access, Extract, nullptr, nullptr, MapOffset, MapNative, nullptr, nullptr, nullptr, nullptr};
	}
	UText *OpenSplitText(UText *text, SplitText source, UErrorCode *status) noexcept
	{
		if (U_FAILURE(*status)) return nullptr;
		if (source.first.size() > INT32_MAX || source.second.size() > INT32_MAX - source.first.size())
		{
			*status = U_INDEX_OUTOFBOUNDS_ERROR;
			return nullptr;
		}
		text = utext_setup(text, sizeof(State), status);
		if (!text) return nullptr;
		auto state = new (text->pExtra) State{};
		state->source = source;
		text->context = source.first.data();
		text->pFuncs = &Functions;
		text->providerProperties = 0;
		text->chunkNativeStart = text->chunkNativeLimit = 0;
		Access(text, 0, true);
		return text;
	}
}
