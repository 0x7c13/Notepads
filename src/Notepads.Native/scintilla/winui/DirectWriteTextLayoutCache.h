// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

#pragma once

#include <array>
#include <cstddef>
#include <cstdint>
#include <string>
#include <string_view>
#include <dwrite.h>
#include <wrl/client.h>

namespace Scintilla::Internal {

Microsoft::WRL::ComPtr<IDWriteTextLayout> CreateDrawingTextLayout(IDWriteFactory *factory,
	IDWriteTextFormat *format, std::wstring_view text, float width, float height, bool *cacheable = nullptr) noexcept;

// UI-thread drawing, owned by one realised, immutable font. Layouts retain
// no brush, render target, selection or document positions. Callers must not
// mutate them. Per-byte measurement/paragraph layouts have separate mutation contracts.
class DirectWriteTextLayoutCache {
public:
	// Both object count and retained source complexity are bounded. DirectWrite's
	// internal allocations are opaque; these are not claims about exact heap bytes.
	static constexpr size_t MaximumEntries = 512;
	static constexpr size_t MaximumTextUnits = 64 * 1024;
	static constexpr size_t MaximumEntryUnits = 4096;
private:
	// Eight candidates avoid repeated collision eviction in styled viewports
	// without retaining the much larger working set of a 1024-entry cache.
	static constexpr size_t Ways = 8;
	static_assert(Ways > 0 && MaximumEntries >= Ways && MaximumEntries % Ways == 0);
	static_assert(MaximumEntryUnits <= MaximumTextUnits);
	struct Entry {
		std::wstring text;
		Microsoft::WRL::ComPtr<IDWriteTextLayout> layout;
		float width = 0;
		float height = 0;
		uint64_t used = 0;
	};
	Microsoft::WRL::ComPtr<IDWriteFactory> factory;
	Microsoft::WRL::ComPtr<IDWriteTextFormat> format;
	std::array<Entry, MaximumEntries> entries;
	size_t textUnits = 0;
	size_t count = 0;
	uint64_t clock = 0;
	void Evict(Entry &entry) noexcept;
public:
	DirectWriteTextLayoutCache(IDWriteFactory *factory_, IDWriteTextFormat *format_) noexcept;
	DirectWriteTextLayoutCache(const DirectWriteTextLayoutCache &) = delete;
	DirectWriteTextLayoutCache &operator=(const DirectWriteTextLayoutCache &) = delete;
	Microsoft::WRL::ComPtr<IDWriteTextLayout> Get(std::wstring_view text, float width, float height);
	size_t Size() const noexcept { return count; }
	size_t TextUnits() const noexcept { return textUnits; }
};

// Short labels (especially line numbers) need only a width, not a retained
// DirectWrite layout. Keep them out of the drawing cache's working set. Owned
// by the same immutable font, used on its UI thread, never by layout workers.
class DirectWriteTextWidthCache {
public:
	static constexpr size_t MaximumEntries = 128;
	static constexpr size_t MaximumEntryUnits = 32;
private:
	static constexpr size_t Ways = 4;
	struct Entry {
		std::array<wchar_t, MaximumEntryUnits> text{};
		size_t length = 0;
		float width = 0;
		uint64_t used = 0;
	};
	Microsoft::WRL::ComPtr<IDWriteFactory> factory;
	Microsoft::WRL::ComPtr<IDWriteTextFormat> format;
	std::array<Entry, MaximumEntries> entries{};
	uint64_t clock = 0;
public:
	DirectWriteTextWidthCache(IDWriteFactory *factory_, IDWriteTextFormat *format_) noexcept : factory(factory_), format(format_) {}
	DirectWriteTextWidthCache(const DirectWriteTextWidthCache &) = delete;
	DirectWriteTextWidthCache &operator=(const DirectWriteTextWidthCache &) = delete;
	float Get(std::wstring_view text);
};

}
