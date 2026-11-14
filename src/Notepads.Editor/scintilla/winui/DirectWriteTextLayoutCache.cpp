// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

#include "DirectWriteTextLayoutCache.h"

#include <algorithm>
#include <cmath>
#include <functional>
#include <limits>
#include <utility>

namespace Scintilla::Internal {
using Microsoft::WRL::ComPtr;

ComPtr<IDWriteTextLayout> CreateDrawingTextLayout(IDWriteFactory *factory, IDWriteTextFormat *format,
	std::wstring_view text, float width, float height, bool *cacheable) noexcept {
	if (cacheable) *cacheable = false;
	ComPtr<IDWriteTextLayout> layout;
	if (text.size() > std::numeric_limits<UINT32>::max() ||
		FAILED(factory->CreateTextLayout(text.data(), static_cast<UINT32>(text.size()),
			format, width, height, &layout))) return {};
	// Preserve the same WinUI typography used by the uncached drawing/measurement
	// path. Do not attach device-dependent effects to a retained layout.
	ComPtr<IDWriteTypography> typography;
	if (SUCCEEDED(factory->CreateTypography(&typography)) &&
		SUCCEEDED(typography->AddFontFeature({DWRITE_FONT_FEATURE_TAG_STANDARD_LIGATURES, 1})) &&
		SUCCEEDED(typography->AddFontFeature({DWRITE_FONT_FEATURE_TAG_CONTEXTUAL_LIGATURES, 1})) &&
		SUCCEEDED(typography->AddFontFeature({DWRITE_FONT_FEATURE_TAG_CONTEXTUAL_ALTERNATES, 1})) &&
		SUCCEEDED(typography->AddFontFeature({DWRITE_FONT_FEATURE_TAG_KERNING, 1})) &&
		SUCCEEDED(typography->AddFontFeature({DWRITE_FONT_FEATURE_TAG_TABULAR_FIGURES, 1}))) {
		const HRESULT applied = layout->SetTypography(typography.Get(), {0, static_cast<UINT32>(text.size())});
		if (cacheable) *cacheable = SUCCEEDED(applied);
	}
	return layout;
}

DirectWriteTextLayoutCache::DirectWriteTextLayoutCache(IDWriteFactory *factory_, IDWriteTextFormat *format_) noexcept :
	factory(factory_), format(format_) {}

void DirectWriteTextLayoutCache::Evict(Entry &entry) noexcept {
	if (!entry.layout) return;
	textUnits -= entry.text.size();
	--count;
	entry = {};
}

ComPtr<IDWriteTextLayout> DirectWriteTextLayoutCache::Get(std::wstring_view text, float width, float height) {
	// Oversized/invalid inputs neither consume the budget nor evict useful rows.
	if (text.empty() || text.size() > MaximumEntryUnits || !std::isfinite(width) || !std::isfinite(height))
		return CreateDrawingTextLayout(factory.Get(), format.Get(), text, width, height);
	const size_t hash = std::hash<std::wstring_view>{}(text) ^
		(std::hash<float>{}(width) << 1) ^ (std::hash<float>{}(height) << 2);
	const size_t first = (hash % (MaximumEntries / Ways)) * Ways;
	Entry *victim = &entries[first];
	for (size_t i = first; i < first + Ways; ++i) {
		auto &entry = entries[i];
		if (entry.layout && entry.width == width && entry.height == height && entry.text == text) {
			entry.used = ++clock;
			return entry.layout;
		}
		if (entry.used < victim->used) victim = &entry;
	}
	bool cacheable = false;
	auto layout = CreateDrawingTextLayout(factory.Get(), format.Get(), text, width, height, &cacheable);
	// Preserve the legacy draw after a typography failure, but retry next time
	// rather than retaining an incompletely configured layout.
	if (!cacheable) return layout;
	std::wstring ownedText(text); // Allocate before changing retention/accounting.
	Evict(*victim);
	while (textUnits + text.size() > MaximumTextUnits) {
		auto oldest = std::min_element(entries.begin(), entries.end(), [](const Entry &a, const Entry &b) {
			if (!a.layout) return false;
			return !b.layout || a.used < b.used;
		});
		Evict(*oldest);
	}
	victim->text = std::move(ownedText);
	victim->layout = layout;
	victim->width = width;
	victim->height = height;
	victim->used = ++clock;
	textUnits += text.size();
	++count;
	return layout;
}

float DirectWriteTextWidthCache::Get(std::wstring_view text) {
	Entry *victim = nullptr;
	if (!text.empty() && text.size() <= MaximumEntryUnits) {
		const size_t first = (std::hash<std::wstring_view>{}(text) % (MaximumEntries / Ways)) * Ways;
		victim = &entries[first];
		for (size_t i = first; i < first + Ways; ++i) {
			auto &entry = entries[i];
			if (entry.used && std::wstring_view(entry.text.data(), entry.length) == text) {
				entry.used = ++clock;
				return entry.width;
			}
			if (entry.used < victim->used) victim = &entry;
		}
	}
	bool cacheable = false;
	const auto layout = CreateDrawingTextLayout(factory.Get(), format.Get(), text, 10000.0F, 1000.0F, &cacheable);
	DWRITE_TEXT_METRICS metrics{};
	if (!layout || FAILED(layout->GetMetrics(&metrics))) return 1.0F;
	if (victim && cacheable) {
		std::copy(text.begin(), text.end(), victim->text.begin());
		victim->length = text.size();
		victim->width = metrics.widthIncludingTrailingWhitespace;
		victim->used = ++clock;
	}
	return metrics.widthIncludingTrailingWhitespace;
}

}
