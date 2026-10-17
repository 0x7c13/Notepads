// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

#include "DirectWriteRunLayout.h"
#include "DirectWriteParagraph.h"

#include <dwrite_2.h>
#include <wrl/client.h>
#include <algorithm>
#include <array>
#include <atomic>
#include <cmath>
#include <limits>
#include <numeric>
#include <string>
#include <unordered_map>
#include <utility>

namespace Scintilla::Internal {
namespace {

using Microsoft::WRL::ComPtr;

// This backend addresses span-vector churn from many short contexts. Long or
// sparse contexts are inexpensive in native layout, which also owns its implicit
// long-run segmentation and floating-point origins. Never cut a shaping context.
constexpr UINT32 minimumContextCount = 256;
constexpr UINT32 maximumContextUnits = 128;

struct Format {
	ComPtr<IDWriteFontCollection> collection;
	ComPtr<IDWriteFontFace> face;
	std::wstring family;
	std::wstring locale;
	float size;
	DWRITE_FONT_WEIGHT weight;
	DWRITE_FONT_STYLE style;
	DWRITE_FONT_STRETCH stretch;

	bool Equivalent(const Format &other) const noexcept {
		return collection == other.collection && family == other.family && locale == other.locale &&
			size == other.size && weight == other.weight && style == other.style && stretch == other.stretch;
	}

	HRESULT Read(IDWriteTextFormat *source) {
		family.resize(static_cast<size_t>(source->GetFontFamilyNameLength()) + 1);
		locale.resize(static_cast<size_t>(source->GetLocaleNameLength()) + 1);
		HRESULT hr = source->GetFontFamilyName(family.data(), static_cast<UINT32>(family.size()));
		if (FAILED(hr)) return hr;
		hr = source->GetLocaleName(locale.data(), static_cast<UINT32>(locale.size()));
		if (FAILED(hr)) return hr;
		hr = source->GetFontCollection(&collection);
		if (FAILED(hr)) return hr;
		size = source->GetFontSize();
		weight = source->GetFontWeight();
		style = source->GetFontStyle();
		stretch = source->GetFontStretch();
		UINT32 index = 0;
		BOOL exists = FALSE;
		hr = collection->FindFamilyName(family.c_str(), &index, &exists);
		if (FAILED(hr) || !exists) return FAILED(hr) ? hr : S_FALSE;
		ComPtr<IDWriteFontFamily> fontFamily;
		hr = collection->GetFontFamily(index, &fontFamily);
		if (FAILED(hr)) return hr;
		ComPtr<IDWriteFont> font;
		hr = fontFamily->GetFirstMatchingFont(weight, stretch, style, &font);
		return FAILED(hr) ? hr : font->CreateFontFace(&face);
	}
};

// DirectWrite's analyzer sees the complete paragraph. Its callbacks update flat
// arrays, avoiding the high-level layout's repeated insertion into a span vector.
class Analysis final : public IDWriteTextAnalysisSource, public IDWriteTextAnalysisSink {
	std::atomic<ULONG> references{1};
public:
	std::wstring_view text;
	std::vector<Format> formats;
	UINT32 format = 0;
	std::vector<DWRITE_SCRIPT_ANALYSIS> scripts;
	std::vector<UINT8> levels;
	std::vector<DWRITE_LINE_BREAKPOINT> breaks;

	explicit Analysis(std::wstring_view text_) : text(text_),
		scripts(text.size()), levels(text.size()), breaks(text.size()) {}

	HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid, void **value) override {
		if (!value) return E_POINTER;
		*value = nullptr;
		if (iid == __uuidof(IUnknown) || iid == __uuidof(IDWriteTextAnalysisSource))
			*value = static_cast<IDWriteTextAnalysisSource *>(this);
		else if (iid == __uuidof(IDWriteTextAnalysisSink)) *value = static_cast<IDWriteTextAnalysisSink *>(this);
		else return E_NOINTERFACE;
		AddRef();
		return S_OK;
	}
	ULONG STDMETHODCALLTYPE AddRef() override { return ++references; }
	ULONG STDMETHODCALLTYPE Release() override {
		const ULONG value = --references;
		if (!value) delete this;
		return value;
	}
	HRESULT STDMETHODCALLTYPE GetTextAtPosition(UINT32 position, WCHAR const **value, UINT32 *length) override {
		*length = position < text.size() ? static_cast<UINT32>(text.size()) - position : 0;
		*value = *length ? text.data() + position : nullptr;
		return S_OK;
	}
	HRESULT STDMETHODCALLTYPE GetTextBeforePosition(UINT32 position, WCHAR const **value, UINT32 *length) override {
		*length = std::min(position, static_cast<UINT32>(text.size()));
		*value = *length ? text.data() : nullptr;
		return S_OK;
	}
	DWRITE_READING_DIRECTION STDMETHODCALLTYPE GetParagraphReadingDirection() override {
		return DWRITE_READING_DIRECTION_LEFT_TO_RIGHT;
	}
	bool SameContext(UINT32 a, UINT32 b) const noexcept {
		return scripts[a].script == scripts[b].script && scripts[a].shapes == scripts[b].shapes && levels[a] == levels[b];
	}
	bool CutsContext(UINT32 start, UINT32 end) const noexcept {
		return (start && SameContext(start - 1, start)) || (end < text.size() && SameContext(end - 1, end));
	}
	HRESULT STDMETHODCALLTYPE GetLocaleName(UINT32 position, UINT32 *length, WCHAR const **locale) override {
		if (position >= text.size()) { *length = 0; *locale = nullptr; return S_OK; }
		*length = static_cast<UINT32>(text.size()) - position;
		*locale = formats[format].locale.c_str();
		return S_OK;
	}
	HRESULT STDMETHODCALLTYPE GetNumberSubstitution(UINT32 position, UINT32 *length,
		IDWriteNumberSubstitution **number) override {
		*length = position < text.size() ? static_cast<UINT32>(text.size()) - position : 0;
		*number = nullptr;
		return S_OK;
	}
	HRESULT STDMETHODCALLTYPE SetScriptAnalysis(UINT32 start, UINT32 length, DWRITE_SCRIPT_ANALYSIS const *value) override {
		if (start > text.size() || length > text.size() - start) return E_INVALIDARG;
		std::fill_n(scripts.begin() + start, length, *value);
		return S_OK;
	}
	HRESULT STDMETHODCALLTYPE SetLineBreakpoints(UINT32 start, UINT32 length, DWRITE_LINE_BREAKPOINT const *value) override {
		if (start > text.size() || length > text.size() - start) return E_INVALIDARG;
		std::copy_n(value, length, breaks.begin() + start);
		return S_OK;
	}
	HRESULT STDMETHODCALLTYPE SetBidiLevel(UINT32 start, UINT32 length, UINT8, UINT8 level) override {
		if (start > text.size() || length > text.size() - start) return E_INVALIDARG;
		std::fill_n(levels.begin() + start, length, level);
		return S_OK;
	}
	HRESULT STDMETHODCALLTYPE SetNumberSubstitution(UINT32, UINT32, IDWriteNumberSubstitution *) override { return S_OK; }
};

HRESULT CheckShapingContext(IDWriteTextAnalyzer *analyzer, const Analysis &analysis,
	const DWRITE_GLYPH_RUN &run, const DWRITE_GLYPH_RUN_DESCRIPTION &description) {
	const auto text = analysis.text;
	const UINT32 length = static_cast<UINT32>(text.size());
	const UINT32 position = description.textPosition, count = description.stringLength, end = position + count;
	const UINT32 actual = run.glyphCount;
	const auto script = analysis.scripts[position];
	const auto level = analysis.levels[position];
	HRESULT hr = S_OK;
	// Font fallback may cut an analyzer context. Certify the retained
	// glyphs against that complete context in this same face. A bounded
	// proof avoids font/joining heuristics and unbounded repeated work.
	if (analysis.CutsContext(position, end)) {
		constexpr UINT32 contextLimit = maximumContextUnits;
		UINT32 contextStart = position, contextEnd = end;
		while (contextStart && analysis.SameContext(contextStart - 1, position)) {
			if (contextEnd - contextStart >= contextLimit) return S_FALSE;
			--contextStart;
		}
		while (contextEnd < length && analysis.SameContext(contextEnd, position)) {
			if (contextEnd - contextStart >= contextLimit) return S_FALSE;
			++contextEnd;
		}
		if (contextEnd - contextStart > contextLimit) return S_FALSE;
		std::array<UINT16, contextLimit> map{};
		std::array<DWRITE_SHAPING_TEXT_PROPERTIES, contextLimit> properties{};
		std::array<UINT16, contextLimit * 4> proofGlyphs{};
		std::array<DWRITE_SHAPING_GLYPH_PROPERTIES, contextLimit * 4> proofProperties{};
		std::array<float, contextLimit * 4> proofAdvances{};
		std::array<DWRITE_GLYPH_OFFSET, contextLimit * 4> proofOffsets{};
		UINT32 proofCount = 0;
		const UINT32 contextLength = contextEnd - contextStart;
		hr = analyzer->GetGlyphs(text.data() + contextStart, contextLength, run.fontFace, FALSE, level & 1,
			&script, description.localeName, nullptr, nullptr, nullptr, 0, static_cast<UINT32>(proofGlyphs.size()),
			map.data(), properties.data(), proofGlyphs.data(), proofProperties.data(), &proofCount);
		if (hr == HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER)) return S_FALSE;
		if (FAILED(hr)) return hr;
		hr = analyzer->GetGlyphPlacements(text.data() + contextStart, map.data(), properties.data(), contextLength,
			proofGlyphs.data(), proofProperties.data(), proofCount, run.fontFace, run.fontEmSize, FALSE, level & 1,
			&script, description.localeName, nullptr, nullptr, 0, proofAdvances.data(), proofOffsets.data());
		if (FAILED(hr)) return hr;
		const UINT32 localStart = position - contextStart, localEnd = end - contextStart;
		if ((localStart && map[localStart - 1] == map[localStart]) ||
			(localEnd < contextLength && map[localEnd - 1] == map[localEnd])) return S_FALSE;
		const auto owned = std::span(map).subspan(localStart, count);
		const UINT32 firstGlyph = *std::min_element(owned.begin(), owned.end());
		const UINT32 lastCluster = *std::max_element(owned.begin(), owned.end());
		UINT32 glyphEnd = proofCount;
		for (UINT32 i = 0; i < contextLength; ++i) {
			if (map[i] > lastCluster) glyphEnd = std::min<UINT32>(glyphEnd, map[i]);
			if ((i < localStart || i >= localEnd) && map[i] >= firstGlyph && map[i] <= lastCluster) return S_FALSE;
		}
		if (glyphEnd < firstGlyph || glyphEnd - firstGlyph != actual) return S_FALSE;
		for (UINT32 i = 0; i < count; ++i)
			if (description.clusterMap[i] + firstGlyph != map[localStart + i]) return S_FALSE;
		for (UINT32 i = 0; i < actual; ++i) {
			const UINT32 proof = firstGlyph + i;
			if (run.glyphIndices[i] != proofGlyphs[proof] || run.glyphAdvances[i] != proofAdvances[proof] ||
				run.glyphOffsets[i].advanceOffset != proofOffsets[proof].advanceOffset ||
				run.glyphOffsets[i].ascenderOffset != proofOffsets[proof].ascenderOffset) return S_FALSE;
		}
	}
	return S_OK;
}

bool UnsupportedCharacter(wchar_t ch) noexcept {
	// ZWNJ's separate zero-width cluster needs native hit-test ownership. ZWJ
	// is deliberately retained: emoji/joining clusters are certified by shaping.
	return ch == L'\t' || ch == 0x061c || ch == 0x200c || ch == 0x200e || ch == 0x200f ||
		(ch >= 0x202a && ch <= 0x202e) || (ch >= 0x2066 && ch <= 0x2069);
}

bool CandidateSource(std::wstring_view text) noexcept {
	UINT32 latinLength = 0;
	for (const auto ch : text) {
		if (UnsupportedCharacter(ch)) return false;
		// Consecutive ASCII letters have one Latin script and resolved level.
		// This proves the context bound would fail without allocating analysis
		// arrays for a long ordinary word. It never cuts or shapes that word.
		const bool latin = (ch >= L'A' && ch <= L'Z') || (ch >= L'a' && ch <= L'z');
		latinLength = latin ? latinLength + 1 : 0;
		if (latinLength > maximumContextUnits) return false;
	}
	return true;
}


}

class DirectWriteRunLayout::Impl {
public:
	struct Run {
		UINT32 start;
		UINT32 length;
		UINT32 glyphStart;
		UINT32 glyphCount;
		UINT32 face;
		UINT32 format;
		UINT8 level;
		float size;
		bool cutsContext;
		float width = 0;
		float left = 0;
	};
	std::wstring_view text;
	std::vector<Format> formats;
	std::vector<ComPtr<IDWriteFontFace>> faces;
	std::vector<UINT16> clusterMap;
	std::vector<UINT16> glyphs;
	std::vector<float> advances;
	std::vector<DWRITE_GLYPH_OFFSET> offsets;
	std::vector<ParagraphGlyphCluster> clusters;
	std::vector<UINT32> visualClusters;
	std::vector<ParagraphGlyphRun> drawRuns;
	float width = 0;
	float lineHeight = 0;
	UINT32 trailingWhitespace = 0;

	HRESULT Create(IDWriteFactory *factory, std::wstring_view source, IDWriteTextFormat *defaultFormat,
		std::span<const ParagraphFormatRange> fontRanges, const ParagraphOptions &options) {
		ComPtr<IDWriteFactory2> factory2;
		HRESULT hr = factory->QueryInterface(IID_PPV_ARGS(&factory2));
		if (hr == E_NOINTERFACE) return S_FALSE;
		if (FAILED(hr)) return hr;
		ComPtr<IDWriteFontFallback> fallback;
		hr = factory2->GetSystemFontFallback(&fallback);
		if (FAILED(hr)) return hr;
		ComPtr<Analysis> analysis;
		analysis.Attach(new Analysis(source));
		std::unordered_map<IDWriteTextFormat *, UINT32> formatIndices;
		auto formatIndex = [&](IDWriteTextFormat *font, UINT32 &index) -> HRESULT {
			const auto found = formatIndices.find(font);
			if (found != formatIndices.end()) { index = found->second; return S_OK; }
			Format format;
			const HRESULT result = format.Read(font);
			if (result != S_OK) return result;
			for (UINT32 candidate = 0; candidate < analysis->formats.size(); ++candidate) {
				if (!format.Equivalent(analysis->formats[candidate])) continue;
				index = candidate;
				formatIndices.emplace(font, index);
				return S_OK;
			}
			index = static_cast<UINT32>(analysis->formats.size());
			analysis->formats.push_back(std::move(format));
			formatIndices.emplace(font, index);
			return S_OK;
		};
		UINT32 index = 0;
		hr = formatIndex(defaultFormat, index);
		if (hr != S_OK) return hr;
		{
			// Resolve final formatting once. The admitted paragraph has one actual
			// font; equivalent style objects never fragment analysis or shaping.
			std::vector<UINT32> formatAt(source.size());
			for (const auto &range : fontRanges) {
				if (range.start > source.size() || range.length > source.size() - range.start || !range.format) return E_INVALIDARG;
				hr = formatIndex(range.format, index);
				if (hr != S_OK) return hr;
				std::fill_n(formatAt.begin() + range.start, range.length, index);
			}
			analysis->format = formatAt.front();
			if (std::any_of(formatAt.begin(), formatAt.end(),
				[&](UINT32 value) { return value != analysis->format; })) return S_FALSE;
		}
		ComPtr<IDWriteTextAnalyzer> analyzer;
		hr = factory->CreateTextAnalyzer(&analyzer);
		if (FAILED(hr)) return hr;
		const UINT32 length = static_cast<UINT32>(source.size());
		hr = analyzer->AnalyzeScript(analysis.Get(), 0, length, analysis.Get());
		if (FAILED(hr)) return hr;
		hr = analyzer->AnalyzeBidi(analysis.Get(), 0, length, analysis.Get());
		if (FAILED(hr)) return hr;
		// Native BiDi draw segmentation can also cut across script boundaries.
		// Keep complete resolved-level spans bounded, not just intersections.
		for (UINT32 start = 0; start < length;) {
			UINT32 end = start + 1;
			while (end < length && analysis->levels[end] == analysis->levels[start]) ++end;
			if (end - start > maximumContextUnits) return S_FALSE;
			start = end;
		}
		UINT32 contextCount = 0;
		for (UINT32 start = 0; start < length;) {
			UINT32 end = start + 1;
			while (end < length && analysis->SameContext(start, end)) ++end;
			if (end - start > maximumContextUnits) return S_FALSE;
			++contextCount;
			start = end;
		}
		// Admission reflects the costly span count, not just document length.
		// The conservative density bound keeps sparse mixed text on native layout.
		constexpr UINT32 maximumAverageContextUnits = 32;
		if (contextCount < minimumContextCount ||
			static_cast<uint64_t>(contextCount) * maximumAverageContextUnits < length) return S_FALSE;
		hr = analyzer->AnalyzeLineBreakpoints(analysis.Get(), 0, length, analysis.Get());
		if (FAILED(hr)) return hr;
		for (const auto &point : analysis->breaks)
			if (point.breakConditionBefore == DWRITE_BREAK_CONDITION_MUST_BREAK ||
				point.breakConditionAfter == DWRITE_BREAK_CONDITION_MUST_BREAK) return S_FALSE;
		if (std::any_of(analysis->scripts.begin(), analysis->scripts.end(),
			[](DWRITE_SCRIPT_ANALYSIS script) { return script.shapes == DWRITE_SCRIPT_SHAPES_NO_VISUAL; })) return S_FALSE;
		while (trailingWhitespace < length && analysis->breaks[length - trailingWhitespace - 1].isWhitespace) ++trailingWhitespace;
		text = source;
		lineHeight = options.lineHeight;
		std::vector<Run> runs;
		runs.reserve(contextCount);
		hr = MapRuns(*analysis.Get(), fallback.Get(), runs);
		if (hr != S_OK) return hr;
		hr = ShapeRuns(*analysis.Get(), analyzer.Get(), runs);
		if (hr != S_OK) return hr;
		formats = std::move(analysis->formats);
		return BuildGeometry(runs);
	}

	HRESULT MapRuns(Analysis &analysis, IDWriteFontFallback *fallback, std::vector<Run> &runs) {
		const UINT32 length = static_cast<UINT32>(text.size());
		HRESULT hr = S_OK;
		// Prepare the complete font/script/BiDi plan before retaining any glyphs.
		std::unordered_map<IDWriteFontFace *, UINT32> faceIndices;
		for (UINT32 position = 0; position < length;) {
			const UINT32 format = analysis.format;
			const auto &style = analysis.formats[format];
			const UINT32 formatEnd = length;
			UINT32 mapped = 0;
			ComPtr<IDWriteFont> mappedFont;
			float scale = 1;
			hr = fallback->MapCharacters(&analysis, position, formatEnd - position, style.collection.Get(),
				style.family.c_str(), style.weight, style.style, style.stretch, &mapped, &mappedFont, &scale);
			if (FAILED(hr)) return hr;
			if (!mapped || mapped > formatEnd - position || !std::isfinite(scale) || scale <= 0) return E_UNEXPECTED;
			ComPtr<IDWriteFontFace> face;
			if (mappedFont) { hr = mappedFont->CreateFontFace(&face); if (FAILED(hr)) return hr; }
			else face = style.face;
			const auto [entry, inserted] = faceIndices.emplace(face.Get(), static_cast<UINT32>(faces.size()));
			if (inserted) faces.push_back(face);
			const UINT32 faceIndex = entry->second;
			const UINT32 mappedEnd = position + mapped;
			while (position < mappedEnd) {
				const auto script = analysis.scripts[position];
				const UINT8 level = analysis.levels[position];
				UINT32 end = position + 1;
				while (end < mappedEnd && analysis.scripts[end].script == script.script &&
					analysis.scripts[end].shapes == script.shapes && analysis.levels[end] == level) ++end;
				runs.push_back({position, end - position, 0, 0, faceIndex, format, level, style.size * scale,
					analysis.CutsContext(position, end)});
				position = end;
			}
		}
		return S_OK;
	}

	HRESULT ShapeRuns(Analysis &analysis, IDWriteTextAnalyzer *analyzer, std::vector<Run> &runs) {
		const UINT32 length = static_cast<UINT32>(text.size());
		HRESULT hr = S_OK;
		clusterMap.resize(length);
		glyphs.reserve(length);
		advances.reserve(length);
		offsets.reserve(length);
		std::vector<DWRITE_SHAPING_TEXT_PROPERTIES> textProperties;
		std::vector<UINT16> runGlyphs;
		std::vector<DWRITE_SHAPING_GLYPH_PROPERTIES> glyphProperties;
		std::vector<float> runAdvances;
		std::vector<DWRITE_GLYPH_OFFSET> runOffsets;
		std::vector<UINT16> clusterStarts;
		std::vector<float> prefix;
		// Cross-font proofs can reject otherwise valid short contexts. Prepare
		// those first into the final buffers, not into a second cache or layout.
		// Physical buffer order never controls logical geometry or paint order.
		for (const bool cutsContext : {true, false}) {
			for (auto &run : runs) {
				if (run.cutsContext != cutsContext) continue;
				const auto position = run.start, count = run.length;
				const auto script = analysis.scripts[position];
				const auto &style = analysis.formats[run.format];
				const auto face = faces[run.face].Get();
				UINT32 capacity = count * 3 / 2 + 16;
				UINT32 actual = 0;
				textProperties.resize(count);
				for (;;) {
					runGlyphs.resize(capacity);
					glyphProperties.resize(capacity);
					hr = analyzer->GetGlyphs(text.data() + position, count, face, FALSE, run.level & 1, &script,
						style.locale.c_str(), nullptr, nullptr, nullptr, 0, capacity, clusterMap.data() + position,
						textProperties.data(), runGlyphs.data(), glyphProperties.data(), &actual);
					if (hr != HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER)) break;
					if (capacity == UINT16_MAX) return S_FALSE;
					capacity = std::min<UINT32>(UINT16_MAX, capacity * 2);
				}
				if (FAILED(hr)) return hr;
				if (actual > UINT16_MAX) return S_FALSE;
				runAdvances.resize(actual);
				runOffsets.resize(actual);
				hr = analyzer->GetGlyphPlacements(text.data() + position, clusterMap.data() + position,
					textProperties.data(), count, runGlyphs.data(), glyphProperties.data(), actual, face,
					run.size, FALSE, run.level & 1, &script, style.locale.c_str(), nullptr, nullptr, 0,
					runAdvances.data(), runOffsets.data());
				if (FAILED(hr)) return hr;
				const DWRITE_GLYPH_RUN shaped{face, run.size, actual, runGlyphs.data(), runAdvances.data(), runOffsets.data(), FALSE, run.level};
				const DWRITE_GLYPH_RUN_DESCRIPTION description{style.locale.c_str(), text.data() + position, count,
					clusterMap.data() + position, position};
				hr = CheckShapingContext(analyzer, analysis, shaped, description);
				if (hr != S_OK) return hr;
				// Validate native cluster ownership while preparing this run, so a
				// failed proof/zero-width run does not survive until final geometry.
				clusterStarts.assign(clusterMap.begin() + position, clusterMap.begin() + position + count);
				size_t groups = 1;
				for (size_t i = 1; i < clusterStarts.size(); ++i) groups += clusterStarts[i] != clusterStarts[i - 1];
				std::sort(clusterStarts.begin(), clusterStarts.end());
				clusterStarts.erase(std::unique(clusterStarts.begin(), clusterStarts.end()), clusterStarts.end());
				if (groups != clusterStarts.size() || clusterStarts.front() != 0 || clusterStarts.back() >= actual) return S_FALSE;
				prefix.assign(actual + 1, 0);
				for (UINT32 i = 0; i < actual; ++i) prefix[i + 1] = prefix[i] + runAdvances[i];
				for (size_t i = 0; i < clusterStarts.size(); ++i) {
					const UINT32 next = i + 1 < clusterStarts.size() ? clusterStarts[i + 1] : actual;
					const float clusterWidth = prefix[next] - prefix[clusterStarts[i]];
					if (!std::isfinite(clusterWidth) || clusterWidth <= 0) return S_FALSE;
				}
				run.width = prefix.back();
				if (glyphs.size() + actual > UINT32_MAX) return E_INVALIDARG;
				run.glyphStart = static_cast<UINT32>(glyphs.size());
				run.glyphCount = actual;
				glyphs.insert(glyphs.end(), runGlyphs.begin(), runGlyphs.begin() + actual);
				advances.insert(advances.end(), runAdvances.begin(), runAdvances.end());
				offsets.insert(offsets.end(), runOffsets.begin(), runOffsets.end());
			}
		}
		return S_OK;
	}

	HRESULT BuildGeometry(std::vector<Run> &runs) {
		for (UINT32 runIndex = 0; runIndex < runs.size(); ++runIndex) {
			const auto &run = runs[runIndex];
			for (UINT32 start = run.start; start < run.start + run.length;) {
				const UINT32 first = start;
				const UINT32 glyph = clusterMap[start];
				while (start < run.start + run.length && clusterMap[start] == glyph) ++start;
				clusters.push_back({first, start - first, runIndex, 0});
			}
			// Match native logical glyph accumulation even though proofs were
			// prepared first and the physical buffer order can differ.
			for (UINT32 i = 0; i < run.glyphCount; ++i) width += advances[run.glyphStart + i];
		}
		std::vector<UINT32> order(runs.size());
		std::iota(order.begin(), order.end(), 0);
		UINT8 maximum = 0, minimumOdd = UINT8_MAX;
		for (const auto &run : runs) {
			maximum = std::max(maximum, run.level);
			if (run.level & 1) minimumOdd = std::min(minimumOdd, run.level);
		}
		if (!std::isfinite(width)) return E_INVALIDARG;
		for (int level = maximum; level >= minimumOdd; --level) {
			for (size_t start = 0; start < order.size();) {
				if (runs[order[start]].level < level) { ++start; continue; }
				size_t end = start + 1;
				while (end < order.size() && runs[order[end]].level >= level) ++end;
				std::reverse(order.begin() + start, order.begin() + end);
				start = end;
			}
		}
		float x = 0;
		for (const auto runIndex : order) {
			runs[runIndex].left = x;
			x += runs[runIndex].width;
		}
		std::vector<float> prefix;
		std::vector<UINT32> glyphOrder;
		for (size_t first = 0; first < clusters.size();) {
			const auto &run = runs[clusters[first].run];
			size_t end = first + 1;
			while (end < clusters.size() && clusters[end].run == clusters[first].run) ++end;
			glyphOrder.resize(end - first);
			std::iota(glyphOrder.begin(), glyphOrder.end(), static_cast<UINT32>(first));
			std::sort(glyphOrder.begin(), glyphOrder.end(), [&](UINT32 a, UINT32 b) { return clusterMap[clusters[a].start] < clusterMap[clusters[b].start]; });
			if (clusterMap[clusters[glyphOrder.front()].start] != 0) return S_FALSE;
			prefix.assign(run.glyphCount + 1, 0);
			for (UINT32 i = 0; i < run.glyphCount; ++i) prefix[i + 1] = prefix[i] + advances[run.glyphStart + i];
			for (size_t i = 0; i < glyphOrder.size(); ++i) {
				auto &cluster = clusters[glyphOrder[i]];
				const UINT32 glyph = clusterMap[cluster.start];
				const UINT32 next = i + 1 < glyphOrder.size() ? clusterMap[clusters[glyphOrder[i + 1]].start] : run.glyphCount;
				if (next <= glyph) return S_FALSE;
				cluster.width = prefix[next] - prefix[glyph];
				// Invisible standalone clusters have native hit-test ownership
				// rules beyond this single-row path. Keep those paragraphs native.
				if (!std::isfinite(cluster.width) || cluster.width <= 0) return S_FALSE;
				const float origin = run.left + ((run.level & 1) ? run.width : 0);
				cluster.left = (run.level & 1) ? origin - prefix[next] : origin + prefix[glyph];
				cluster.right = (run.level & 1) ? origin - prefix[glyph] : origin + prefix[next];
			}
			for (size_t i = first; i < end; ++i) {
				const auto &cluster = clusters[i];
				if (cluster.length > UINT16_MAX) return S_FALSE;
			}
			first = end;
		}
		visualClusters.resize(clusters.size());
		std::iota(visualClusters.begin(), visualClusters.end(), 0);
		std::stable_sort(visualClusters.begin(), visualClusters.end(), [&](UINT32 a, UINT32 b) { return clusters[a].left < clusters[b].left; });
		drawRuns.reserve(runs.size());
		// DirectWrite draws callbacks in logical order. Keep that compositing
		// order even though the retained origins were placed in visual order.
		for (const auto &run : runs) {
			drawRuns.push_back({{faces[run.face].Get(), run.size, run.glyphCount, glyphs.data() + run.glyphStart,
				advances.data() + run.glyphStart, offsets.data() + run.glyphStart, FALSE, run.level},
				{formats[run.format].locale.c_str(), text.data() + run.start, run.length, clusterMap.data() + run.start, run.start},
				run.left + ((run.level & 1) ? run.width : 0), run.left, run.width});
		}
		return S_OK;
	}

	const ParagraphGlyphCluster &At(UINT32 position) const noexcept {
		const auto after = std::upper_bound(clusters.begin(), clusters.end(), position,
			[](UINT32 value, const ParagraphGlyphCluster &cluster) { return value < cluster.start; });
		return after == clusters.begin() ? clusters.front() : *std::prev(after);
	}
	DWRITE_HIT_TEST_METRICS Metrics(const ParagraphGlyphCluster &cluster) const noexcept {
		const auto level = drawRuns[cluster.run].glyphs.bidiLevel;
		return {cluster.start, cluster.length, cluster.left, 0, cluster.width, lineHeight, level, TRUE, FALSE};
	}
};

DirectWriteRunLayout::DirectWriteRunLayout() : impl(std::make_unique<Impl>()) {}
DirectWriteRunLayout::~DirectWriteRunLayout() = default;

HRESULT DirectWriteRunLayout::TryCreate(IDWriteFactory *factory, std::wstring_view text,
	IDWriteTextFormat *format, std::span<const ParagraphFormatRange> formats,
	const ParagraphOptions &options, std::unique_ptr<DirectWriteRunLayout> &result) {
	result.reset();
	if (!factory || !format || text.size() > UINT32_MAX) return E_INVALIDARG;
	// Native layout owns RTL-base alignment and its float accumulation rules.
	// RTL runs inside an LTR-base paragraph still share the optimized geometry.
	if (text.size() < MinimumLength || options.rtl || options.wrap != ParagraphWrap::None || !CandidateSource(text)) return S_FALSE;
	auto candidate = std::unique_ptr<DirectWriteRunLayout>(new DirectWriteRunLayout());
	const HRESULT hr = candidate->impl->Create(factory, text, format, formats, options);
	if (hr == S_OK) result = std::move(candidate);
	return hr;
}

float DirectWriteRunLayout::Width() const noexcept { return impl->width; }
UINT32 DirectWriteRunLayout::TrailingWhitespace() const noexcept { return impl->trailingWhitespace; }
std::span<const ParagraphGlyphRun> DirectWriteRunLayout::GlyphRuns() const noexcept { return impl->drawRuns; }
std::span<const ParagraphGlyphCluster> DirectWriteRunLayout::GlyphClusters() const noexcept {
	return impl->clusters;
}

float DirectWriteRunLayout::TextPosition(UINT32 position, bool trailing) const noexcept {
	const auto &cluster = impl->At(std::min(position, static_cast<UINT32>(impl->text.size()) - 1));
	const bool end = trailing || position >= impl->text.size();
	return ((impl->drawRuns[cluster.run].glyphs.bidiLevel & 1) != 0) == end ? cluster.left : cluster.right;
}

DWRITE_HIT_TEST_METRICS DirectWriteRunLayout::HitPoint(float x, bool &trailing) const noexcept {
	const auto after = std::upper_bound(impl->visualClusters.begin(), impl->visualClusters.end(), x,
		[&](float value, UINT32 index) { return value < impl->clusters[index].left; });
	const auto index = after == impl->visualClusters.begin() ? impl->visualClusters.front() : *std::prev(after);
	const auto &cluster = impl->clusters[index];
	trailing = (x >= cluster.left + cluster.width / 2) != ((impl->drawRuns[cluster.run].glyphs.bidiLevel & 1) != 0);
	return impl->Metrics(cluster);
}

std::vector<DWRITE_HIT_TEST_METRICS> DirectWriteRunLayout::Range(UINT32 start, UINT32 length) const {
	std::vector<DWRITE_HIT_TEST_METRICS> result;
	if (!length || start >= impl->text.size()) return result;
	UINT32 end = start + std::min(length, static_cast<UINT32>(impl->text.size()) - start);
	start = impl->At(start).start;
	if (end < impl->text.size()) end = impl->At(end).start;
	if (start == end) {
		auto metric = impl->Metrics(impl->At(start));
		metric.left = TextPosition(start, false);
		metric.width = 0;
		metric.length = 0;
		result.push_back(metric);
		return result;
	}
	for (auto it = std::lower_bound(impl->clusters.begin(), impl->clusters.end(), start,
		[](const ParagraphGlyphCluster &cluster, UINT32 value) { return cluster.start + cluster.length <= value; });
		it != impl->clusters.end() && it->start < end; ++it) {
		const auto metric = impl->Metrics(*it);
		if (!result.empty() && result.back().bidiLevel == metric.bidiLevel &&
			result.back().textPosition + result.back().length == metric.textPosition) {
			auto &previous = result.back();
			previous.left = std::min(previous.left, metric.left);
			previous.width += metric.width;
			previous.length += metric.length;
		} else result.push_back(metric);
	}
	return result;
}

}
