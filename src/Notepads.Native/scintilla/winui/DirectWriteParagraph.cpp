// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

#include "DirectWriteParagraph.h"
#include "DirectWriteRunLayout.h"

#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#include <dwrite.h>
#include <wrl/client.h>

#include <algorithm>
#include <array>
#include <atomic>
#include <cmath>
#include <limits>
#include <mutex>
#include <stdexcept>
#include <string>
#include <utility>

#include "UniConversion.h"

namespace Scintilla::Internal {
namespace {

using Microsoft::WRL::ComPtr;

struct DecodedCharacter {
	uint32_t value;
	size_t length;
};

// Share Scintilla's drawing boundaries, including its byte representations for
// Unicode noncharacters. Font and representation spans use the same policy.
DecodedCharacter Decode(std::string_view text, size_t offset) noexcept {
	const auto first = static_cast<unsigned char>(text[offset]);
	const size_t length = UTF8DrawBytes(text.data() + offset, text.size() - offset);
	if (length == 1) return {first < 0x80 ? first : uint32_t{unicodeReplacementChar}, 1};
	return {static_cast<uint32_t>(UnicodeFromUTF8(text.substr(offset, length))), length};
}

bool DirectionControl(uint32_t value) noexcept {
	return value == 0x061c || value == 0x200e || value == 0x200f ||
		(value >= 0x202a && value <= 0x202e) || (value >= 0x2066 && value <= 0x2069);
}

bool SupplementaryBidiBlock(uint32_t value) noexcept {
	// GetStringTypeW classifies individual UTF-16 units and does not reliably
	// report the direction of supplementary scripts. These script blocks contain
	// RTL letters/numbers (historic Semitic scripts, Mende, Adlam and Arabic).
	// Conservatively shaping their other characters is safe; skipping a strong
	// RTL letter would route the entire paragraph through the LTR-only renderer.
	return (value >= 0x10800 && value <= 0x10fff) ||
		(value >= 0x1e800 && value <= 0x1efff);
}

class Representation final : public IDWriteInlineObject {
	std::atomic<ULONG> references{1};
	DWRITE_INLINE_OBJECT_METRICS metrics;
public:
	Representation(float width, float height, float baseline) noexcept :
		metrics{width, height, baseline, FALSE} {}
	HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid, void **result) override {
		if (!result) return E_POINTER;
		*result = nullptr;
		if (iid == __uuidof(IUnknown) || iid == __uuidof(IDWriteInlineObject)) {
			*result = static_cast<IDWriteInlineObject *>(this);
			AddRef();
			return S_OK;
		}
		return E_NOINTERFACE;
	}
	ULONG STDMETHODCALLTYPE AddRef() override { return ++references; }
	ULONG STDMETHODCALLTYPE Release() override {
		const ULONG count = --references;
		if (!count) delete this;
		return count;
	}
	HRESULT STDMETHODCALLTYPE Draw(void *, IDWriteTextRenderer *, FLOAT, FLOAT,
		BOOL, BOOL, IUnknown *) override {
		// Scintilla draws the visible representation at this range's geometry.
		return S_OK;
	}
	HRESULT STDMETHODCALLTYPE GetMetrics(DWRITE_INLINE_OBJECT_METRICS *result) override {
		if (!result) return E_POINTER;
		*result = metrics;
		return S_OK;
	}
	HRESULT STDMETHODCALLTYPE GetOverhangMetrics(DWRITE_OVERHANG_METRICS *result) override {
		if (!result) return E_POINTER;
		*result = {};
		return S_OK;
	}
	HRESULT STDMETHODCALLTYPE GetBreakConditions(DWRITE_BREAK_CONDITION *before,
		DWRITE_BREAK_CONDITION *after) override {
		if (!before || !after) return E_POINTER;
		*before = *after = DWRITE_BREAK_CONDITION_NEUTRAL;
		return S_OK;
	}
};

HRESULT ApplyFormat(IDWriteTextLayout *layout, IDWriteTextFormat *format,
	DWRITE_TEXT_RANGE range) {
	if (!format || !range.length) return S_OK;
	std::wstring family(static_cast<size_t>(format->GetFontFamilyNameLength()) + 1, L'\0');
	HRESULT result = format->GetFontFamilyName(family.data(), static_cast<UINT32>(family.size()));
	if (FAILED(result)) return result;
	result = layout->SetFontFamilyName(family.c_str(), range);
	if (FAILED(result)) return result;
	std::wstring locale(static_cast<size_t>(format->GetLocaleNameLength()) + 1, L'\0');
	result = format->GetLocaleName(locale.data(), static_cast<UINT32>(locale.size()));
	if (FAILED(result)) return result;
	result = layout->SetLocaleName(locale.c_str(), range);
	if (FAILED(result)) return result;
	ComPtr<IDWriteFontCollection> collection;
	result = format->GetFontCollection(&collection);
	if (FAILED(result)) return result;
	result = layout->SetFontCollection(collection.Get(), range);
	if (FAILED(result)) return result;
	result = layout->SetFontSize(format->GetFontSize(), range);
	if (FAILED(result)) return result;
	result = layout->SetFontWeight(format->GetFontWeight(), range);
	if (FAILED(result)) return result;
	result = layout->SetFontStyle(format->GetFontStyle(), range);
	if (FAILED(result)) return result;
	return layout->SetFontStretch(format->GetFontStretch(), range);
}

}

class DirectWriteParagraph::Impl {
public:
	struct Boundary {
		size_t byte;
		UINT32 wide;
	};
	// Create bounds the complete input to UINT32_MAX bytes. Native line
	// metrics also use UINT32 counts; these internal indices fit that bound.
	// Public geometry continues to expose size_t offsets.
	struct Stop {
		uint32_t byte;
		uint32_t line;
		float x;
		int8_t approach;
		bool trailing;
		Stop(ParagraphCaret caret, float position, int approach_, size_t line_) noexcept :
			byte(static_cast<uint32_t>(caret.position)), line(static_cast<uint32_t>(line_)),
			x(position), approach(static_cast<int8_t>(approach_)), trailing(caret.trailing) {}
		ParagraphCaret Caret() const noexcept { return {byte, trailing}; }
	};
	struct VisualCluster {
		uint32_t start;
		uint32_t length;
		float centre;
		VisualCluster(ParagraphRange range, float position) noexcept :
			start(static_cast<uint32_t>(range.start)), length(static_cast<uint32_t>(range.length)), centre(position) {}
		ParagraphRange Range() const noexcept { return {start, length}; }
	};
	static_assert(sizeof(Stop) == 16);
	static_assert(sizeof(VisualCluster) == 12);
	HRESULT status = E_FAIL;
	std::string text;
	std::wstring wide;
	std::vector<Boundary> boundaries;
	ComPtr<IDWriteTextLayout> layout;
	std::unique_ptr<DirectWriteRunLayout> analyzed;
	ParagraphOptions options;
	std::vector<ParagraphLine> lines;
	std::vector<std::vector<Stop>> stops;
	std::vector<const Stop *> logicalStops;
	std::vector<std::vector<VisualCluster>> visualClusters;
	std::once_flag stopsReady;
	float textWidth = 0;

	float DrawOriginX() const noexcept {
		return options.rtl ? options.trailingPadding : options.leadingPadding;
	}

	UINT32 WidePosition(size_t byte) const noexcept {
		const auto after = std::upper_bound(boundaries.begin(), boundaries.end(), byte,
			[](size_t position, const Boundary &boundary) { return position < boundary.byte; });
		return after == boundaries.begin() ? 0 : std::prev(after)->wide;
	}
	size_t BytePosition(UINT32 position) const noexcept {
		const auto after = std::upper_bound(boundaries.begin(), boundaries.end(), position,
			[](UINT32 value, const Boundary &boundary) { return value < boundary.wide; });
		return after == boundaries.begin() ? 0 : std::prev(after)->byte;
	}
	bool ValidSpan(size_t start, size_t length) const noexcept {
		return start <= text.size() && length <= text.size() - start &&
			BytePosition(WidePosition(start)) == start &&
			BytePosition(WidePosition(start + length)) == start + length;
	}

	HRESULT Create(IDWriteFactory *factory, std::string_view input, IDWriteTextFormat *defaultFormat,
		std::span<const ParagraphFontRun> fonts, std::span<const ParagraphRepresentation> representations,
		ParagraphOptions requestedOptions) {
		if (!factory || !defaultFormat || input.size() > std::numeric_limits<UINT32>::max() ||
			!std::isfinite(requestedOptions.width) || requestedOptions.width <= 0 ||
			!std::isfinite(requestedOptions.lineHeight) || requestedOptions.lineHeight <= 0 ||
			!std::isfinite(requestedOptions.baseline) || requestedOptions.baseline < 0 ||
			requestedOptions.baseline > requestedOptions.lineHeight ||
			!std::isfinite(requestedOptions.tabWidth) || requestedOptions.tabWidth <= 0 ||
			!std::isfinite(requestedOptions.leadingPadding) || requestedOptions.leadingPadding < 0 ||
			!std::isfinite(requestedOptions.trailingPadding) || requestedOptions.trailingPadding < 0 ||
			!std::isfinite(requestedOptions.leadingPadding + requestedOptions.trailingPadding)) return E_INVALIDARG;
		options = requestedOptions;
		text = input;
		wide.reserve(text.size());
		for (size_t i = 0; i < text.size();) {
			boundaries.push_back({i, static_cast<UINT32>(wide.size())});
			const auto character = Decode(text, i);
			if (character.value > 0xffff) {
				const uint32_t supplementary = character.value - 0x10000;
				wide.push_back(static_cast<wchar_t>(0xd800 + (supplementary >> 10)));
				wide.push_back(static_cast<wchar_t>(0xdc00 + (supplementary & 0x3ff)));
			} else {
				wide.push_back(static_cast<wchar_t>(character.value));
			}
			i += character.length;
		}
		boundaries.push_back({text.size(), static_cast<UINT32>(wide.size())});
		// The high-level layout repeatedly moves its span vector on dense long
		// paragraphs. Keep its exact general path for wrapping/representations;
		// the specialized path analyzes the complete paragraph into flat arrays.
		if (!options.rtl && wide.size() >= DirectWriteRunLayout::MinimumLength && representations.empty() && options.wrap == ParagraphWrap::None) {
			std::vector<ParagraphFormatRange> ranges;
			ranges.reserve(fonts.size());
			for (const auto &font : fonts) {
				if (!font.format || !ValidSpan(font.start, font.length)) return E_INVALIDARG;
				const UINT32 start = WidePosition(font.start);
				ranges.push_back({start, WidePosition(font.start + font.length) - start, font.format});
			}
			const HRESULT result = DirectWriteRunLayout::TryCreate(factory, wide, defaultFormat, ranges, options, analyzed);
			if (FAILED(result)) return result;
			if (result == S_OK) {
				textWidth = analyzed->Width();
				if (!std::isfinite(textWidth + options.leadingPadding + options.trailingPadding)) return E_INVALIDARG;
				lines.push_back({0, text.size(), 0, options.lineHeight, options.baseline,
					text.size() - BytePosition(static_cast<UINT32>(wide.size()) - analyzed->TrailingWhitespace())});
				return S_OK;
			}
		}
		const float textWidthAvailable = std::max(1.0f, options.width - options.leadingPadding - options.trailingPadding);
		HRESULT result = factory->CreateTextLayout(wide.data(), static_cast<UINT32>(wide.size()),
			defaultFormat, textWidthAvailable, std::numeric_limits<float>::max(), &layout);
		if (FAILED(result)) return result;
		result = layout->SetReadingDirection(options.rtl ? DWRITE_READING_DIRECTION_RIGHT_TO_LEFT :
			DWRITE_READING_DIRECTION_LEFT_TO_RIGHT);
		if (FAILED(result)) return result;
		result = layout->SetFlowDirection(DWRITE_FLOW_DIRECTION_TOP_TO_BOTTOM);
		if (FAILED(result)) return result;
		result = layout->SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING);
		if (FAILED(result)) return result;
		result = layout->SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR);
		if (FAILED(result)) return result;
		const auto wrapping = options.wrap == ParagraphWrap::None ? DWRITE_WORD_WRAPPING_NO_WRAP :
			options.wrap == ParagraphWrap::Character ? DWRITE_WORD_WRAPPING_CHARACTER : DWRITE_WORD_WRAPPING_WRAP;
		result = layout->SetWordWrapping(wrapping);
		if (FAILED(result)) return result;
		result = layout->SetLineSpacing(DWRITE_LINE_SPACING_METHOD_UNIFORM, options.lineHeight, options.baseline);
		if (FAILED(result)) return result;
		result = layout->SetIncrementalTabStop(options.tabWidth);
		if (FAILED(result)) return result;
		for (size_t index = 0; index < fonts.size();) {
			ParagraphFontRun run = fonts[index++];
			if (!run.format || !ValidSpan(run.start, run.length)) return E_INVALIDARG;
			// Font runs often originate in byte-wise style arrays. Merge adjacent
			// identical fonts before asking DirectWrite to partition its layout.
			while (index < fonts.size() && fonts[index].format == run.format &&
				fonts[index].start == run.start + run.length) {
				if (!ValidSpan(fonts[index].start, fonts[index].length)) return E_INVALIDARG;
				run.length += fonts[index++].length;
			}
			const UINT32 start = WidePosition(run.start);
			result = ApplyFormat(layout.Get(), run.format, {start, WidePosition(run.start + run.length) - start});
			if (FAILED(result)) return result;
		}
		for (const auto &representation : representations) {
			if (!ValidSpan(representation.start, representation.length) || !representation.length ||
				!std::isfinite(representation.width) || representation.width < 0) return E_INVALIDARG;
			const UINT32 start = WidePosition(representation.start);
			ComPtr<IDWriteInlineObject> object;
			object.Attach(new Representation(representation.width, options.lineHeight, options.baseline));
			result = layout->SetInlineObject(object.Get(),
				{start, WidePosition(representation.start + representation.length) - start});
			if (FAILED(result)) return result;
		}
		DWRITE_TEXT_METRICS metrics{};
		result = layout->GetMetrics(&metrics);
		if (FAILED(result)) return result;
		textWidth = metrics.widthIncludingTrailingWhitespace;
		if (!std::isfinite(textWidth + options.leadingPadding + options.trailingPadding)) return E_INVALIDARG;
		UINT32 count = 0;
		result = layout->GetLineMetrics(nullptr, 0, &count);
		if (FAILED(result) && result != E_NOT_SUFFICIENT_BUFFER) return result;
		std::vector<DWRITE_LINE_METRICS> nativeLines(count);
		result = layout->GetLineMetrics(nativeLines.data(), count, &count);
		if (FAILED(result)) return result;
		UINT32 start = 0;
		float top = 0;
		for (const auto &line : nativeLines) {
			const size_t byteStart = BytePosition(start);
			const size_t byteEnd = BytePosition(start + line.length);
			lines.push_back({byteStart, byteEnd - byteStart, top, line.height, line.baseline,
				byteEnd - BytePosition(start + line.length - line.trailingWhitespaceLength)});
			top += line.height;
			start += line.length;
		}
		if (lines.empty()) lines.push_back({0, 0, 0, options.lineHeight, options.baseline, 0});
		return S_OK;
	}

	size_t CaretLine(ParagraphCaret caret) const noexcept {
		size_t position = std::min(caret.position, text.size());
		if (caret.trailing && position) --position;
		const auto after = std::upper_bound(lines.begin(), lines.end(), position,
			[](size_t value, const ParagraphLine &line) { return value < line.start; });
		return after == lines.begin() ? 0 : static_cast<size_t>(std::prev(after) - lines.begin());
	}

	ParagraphPoint Point(ParagraphCaret caret) const {
		if (!layout && !analyzed) return {};
		UINT32 position = WidePosition(std::min(caret.position, text.size()));
		BOOL trailing = FALSE;
		if (caret.trailing && position) {
			// Asking about the previous UTF-16 unit is safe inside a surrogate or
			// combining cluster: DirectWrite returns that whole cluster's edge.
			--position;
			trailing = TRUE;
		}
		FLOAT x = 0, y = 0;
		DWRITE_HIT_TEST_METRICS metrics{};
		if (analyzed) x = analyzed->TextPosition(position, trailing != FALSE);
		else if (FAILED(layout->HitTestTextPosition(position, trailing, &x, &y, &metrics))) return {};
		// DirectWrite's hit box follows font metrics and may start above the
		// uniform row, particularly with fallback fonts or a small baseline.
		// Scintilla's caret and selection use the display row's bounds instead.
		const auto &line = lines[CaretLine(caret)];
		return {x + DrawOriginX(), line.top, line.height};
	}

	std::vector<ParagraphRect> Rectangles(size_t start, size_t end) const {
		std::vector<ParagraphRect> rectangles;
		if (!layout && !analyzed) return rectangles;
		if (start > end) std::swap(start, end);
		start = std::min(start, text.size());
		end = std::min(end, text.size());
		const UINT32 first = WidePosition(start);
		const UINT32 length = WidePosition(end) - first;
		if (!length) return rectangles;
		if (analyzed) {
			const auto metrics = analyzed->Range(first, length);
			rectangles.reserve(metrics.size());
			for (const auto &metric : metrics) {
				const size_t startByte = BytePosition(metric.textPosition);
				rectangles.push_back({metric.left + DrawOriginX(), 0, metric.width, options.lineHeight,
					startByte, BytePosition(metric.textPosition + metric.length) - startByte});
			}
			return rectangles;
		}
		std::array<DWRITE_HIT_TEST_METRICS, 8> localMetrics{};
		UINT32 count = 0;
		HRESULT result = layout->HitTestTextRange(first, length, 0, 0, localMetrics.data(),
			static_cast<UINT32>(localMetrics.size()), &count);
		std::vector<DWRITE_HIT_TEST_METRICS> large;
		const DWRITE_HIT_TEST_METRICS *metrics = localMetrics.data();
		if (result == E_NOT_SUFFICIENT_BUFFER) {
			large.resize(count);
			result = layout->HitTestTextRange(first, length, 0, 0, large.data(), count, &count);
			metrics = large.data();
		}
		if (FAILED(result)) return rectangles;
		rectangles.reserve(count);
		for (UINT32 i = 0; i < count; ++i) {
			const auto &rectangle = metrics[i];
			const size_t byteStart = BytePosition(rectangle.textPosition);
			const auto &line = lines[CaretLine({byteStart, false})];
			rectangles.push_back({rectangle.left + DrawOriginX(), line.top, rectangle.width, line.height,
				byteStart, BytePosition(rectangle.textPosition + rectangle.length) - byteStart});
		}
		return rectangles;
	}

	HRESULT BuildStops() {
		stops.assign(lines.size(), {});
		visualClusters.assign(lines.size(), {});
		logicalStops.clear();
		const auto appendCluster = [&](UINT32 start, UINT32 end, float leading, float trailing, bool rtl, size_t rowIndex) {
			const size_t startByte = BytePosition(start);
			const size_t endByte = BytePosition(end);
			auto &row = stops[rowIndex];
			row.push_back({{startByte, false}, leading, rtl ? 1 : -1, rowIndex});
			row.push_back({{endByte, true}, trailing, rtl ? -1 : 1, rowIndex});
			visualClusters[rowIndex].push_back({{startByte, endByte - startByte}, (leading + trailing) / 2});
		};
		if (analyzed) {
			const auto runs = analyzed->GlyphRuns();
			for (const auto &cluster : analyzed->GlyphClusters()) {
				if (std::abs(cluster.right - cluster.left) <= 0.001f) continue;
				const bool rtl = (runs[cluster.run].glyphs.bidiLevel & 1) != 0;
				appendCluster(cluster.start, cluster.start + cluster.length,
					(rtl ? cluster.right : cluster.left) + DrawOriginX(),
					(rtl ? cluster.left : cluster.right) + DrawOriginX(), rtl, 0);
			}
		} else {
			UINT32 count = 0;
			HRESULT result = layout->GetClusterMetrics(nullptr, 0, &count);
			if (FAILED(result) && result != E_NOT_SUFFICIENT_BUFFER) return result;
			std::vector<DWRITE_CLUSTER_METRICS> clusters(count);
			if (count) {
				result = layout->GetClusterMetrics(clusters.data(), count, &count);
				if (FAILED(result)) return result;
			}
			std::vector<UINT32> clusterStarts;
			clusterStarts.reserve(clusters.size() + 1);
			UINT32 position = 0;
			for (const auto &cluster : clusters) {
				clusterStarts.push_back(position);
				position += cluster.length;
			}
			clusterStarts.push_back(position);
			UINT32 runCount = 0;
			result = layout->HitTestTextRange(0, static_cast<UINT32>(wide.size()), 0, 0, nullptr, 0, &runCount);
			if (FAILED(result) && result != E_NOT_SUFFICIENT_BUFFER) return result;
			std::vector<DWRITE_HIT_TEST_METRICS> runs(runCount);
			if (runCount) {
				result = layout->HitTestTextRange(0, static_cast<UINT32>(wide.size()), 0, 0, runs.data(), runCount, &runCount);
				if (FAILED(result)) return result;
			}
			for (const auto &run : runs) {
				const bool rtl = (run.bidiLevel & 1) != 0;
				float edge = run.left + DrawOriginX() + (rtl ? run.width : 0);
				const auto first = std::lower_bound(clusterStarts.begin(), clusterStarts.end(), run.textPosition);
				const size_t rowIndex = CaretLine({BytePosition(run.textPosition), false});
				for (size_t i = static_cast<size_t>(first - clusterStarts.begin());
					i < clusters.size() && clusterStarts[i] < run.textPosition + run.length; ++i) {
					const auto &cluster = clusters[i];
					const float next = edge + (rtl ? -cluster.width : cluster.width);
					// The full-range rectangles already carry resolved bidi levels and
					// origins. Cluster advances avoid a quadratic hit-test per glyph.
					if (cluster.width > 0.001f) {
						appendCluster(clusterStarts[i], clusterStarts[i + 1], edge, next, rtl, rowIndex);
					}
					edge = next;
				}
			}
		}
		for (size_t i = 0; i < stops.size(); ++i) {
			auto &row = stops[i];
			if (row.empty()) {
				const ParagraphCaret caret{lines[i].start, false};
				row.push_back({caret, Point(caret).x, 0, i});
			}
			std::stable_sort(row.begin(), row.end(), [](const Stop &a, const Stop &b) {
				return a.x < b.x;
			});
			for (const auto &stop : row) logicalStops.push_back(&stop);
		}
		std::sort(logicalStops.begin(), logicalStops.end(), [](const Stop *a, const Stop *b) {
			return a->byte < b->byte ||
				(a->byte == b->byte && a->trailing < b->trailing);
		});
		return S_OK;
	}

	void EnsureStops() {
		std::call_once(stopsReady, [this]() {
			if (FAILED(BuildStops())) throw std::runtime_error("DirectWrite visual caret layout failed.");
		});
	}

	const Stop *FindStop(ParagraphCaret caret) const noexcept {
		const auto match = std::lower_bound(logicalStops.begin(), logicalStops.end(), caret,
			[](const Stop *stop, ParagraphCaret position) {
				return stop->byte < position.position ||
					(stop->byte == position.position && stop->trailing < position.trailing);
			});
		if (match != logicalStops.end() && (*match)->Caret() == caret) return *match;
		return nullptr;
	}

	ParagraphCaret Edge(size_t line, bool right) const noexcept {
		if (stops.empty()) return {};
		const auto &row = stops[std::min(line, stops.size() - 1)];
		const float x = right ? row.back().x : row.front().x;
		auto first = std::lower_bound(row.begin(), row.end(), x - 0.001f,
			[](const Stop &stop, float value) { return stop.x < value; });
		for (; first != row.end() && std::abs(first->x - x) < 0.001f; ++first) {
			if (first->approach == (right ? 1 : -1)) return first->Caret();
		}
		return right ? row.back().Caret() : row.front().Caret();
	}
};

DirectWriteParagraph::DirectWriteParagraph(IDWriteFactory *factory, std::string_view text,
	IDWriteTextFormat *defaultFormat, std::span<const ParagraphFontRun> fonts,
	std::span<const ParagraphRepresentation> representations, ParagraphOptions options) :
	impl(std::make_unique<Impl>()) {
	impl->status = impl->Create(factory, text, defaultFormat, fonts, representations, options);
	if (FAILED(impl->status)) {
		impl->layout.Reset();
		impl->analyzed.reset();
		impl->lines.clear();
		impl->stops.clear();
	}
}

DirectWriteParagraph::~DirectWriteParagraph() = default;

bool DirectWriteParagraph::NeedsLayout(std::string_view text) {
	std::array<wchar_t, 128> buffer{};
	std::array<WORD, 128> types{};
	size_t count = 0;
	auto containsBidi = [&]() {
		if (!count) return false;
		if (!GetStringTypeW(CT_CTYPE2, buffer.data(), static_cast<int>(count), types.data())) return true;
		return std::any_of(types.begin(), types.begin() + count, [](WORD type) {
			return type == C2_RIGHTTOLEFT || type == C2_ARABICNUMBER;
		});
	};
	for (size_t i = 0; i < text.size();) {
		const auto character = Decode(text, i);
		i += character.length;
		if (DirectionControl(character.value) || SupplementaryBidiBlock(character.value)) return true;
		if (character.value < 0x80) continue;
		if (count + 2 > buffer.size()) {
			if (containsBidi()) return true;
			count = 0;
		}
		if (character.value > 0xffff) {
			const uint32_t supplementary = character.value - 0x10000;
			buffer[count++] = static_cast<wchar_t>(0xd800 + (supplementary >> 10));
			buffer[count++] = static_cast<wchar_t>(0xdc00 + (supplementary & 0x3ff));
		} else {
			buffer[count++] = static_cast<wchar_t>(character.value);
		}
	}
	return containsBidi();
}

int32_t DirectWriteParagraph::Status() const noexcept { return impl->status; }
bool DirectWriteParagraph::IsValid() const noexcept { return SUCCEEDED(impl->status); }
IDWriteTextLayout *DirectWriteParagraph::Layout() const noexcept { return impl->layout.Get(); }
std::span<const ParagraphGlyphRun> DirectWriteParagraph::GlyphRuns() const noexcept {
	return impl->analyzed ? impl->analyzed->GlyphRuns() : std::span<const ParagraphGlyphRun>{};
}
float DirectWriteParagraph::DrawOriginX() const noexcept { return impl->DrawOriginX(); }
std::span<const ParagraphLine> DirectWriteParagraph::Lines() const noexcept { return impl->lines; }
float DirectWriteParagraph::WidthIncludingTrailingWhitespace() const noexcept { return impl->textWidth; }
float DirectWriteParagraph::WidthIncludingPadding() const noexcept {
	return impl->textWidth + impl->options.leadingPadding + impl->options.trailingPadding;
}
float DirectWriteParagraph::LayoutWidth() const noexcept { return impl->options.width; }
size_t DirectWriteParagraph::Length() const noexcept { return impl->text.size(); }
size_t DirectWriteParagraph::CaretLine(ParagraphCaret caret) const noexcept { return impl->CaretLine(caret); }
uint32_t DirectWriteParagraph::Utf16Position(size_t bytePosition) const noexcept { return impl->WidePosition(bytePosition); }
size_t DirectWriteParagraph::BytePosition(uint32_t utf16Position) const noexcept { return impl->BytePosition(utf16Position); }

ParagraphCaret DirectWriteParagraph::HitTest(float x, float y, bool charPosition) const {
	if (!IsValid()) return {};
	BOOL trailing = FALSE, inside = FALSE;
	DWRITE_HIT_TEST_METRICS metrics{};
	if (impl->analyzed) {
		bool hitTrailing = false;
		metrics = impl->analyzed->HitPoint(x - impl->DrawOriginX(), hitTrailing);
		trailing = hitTrailing;
	} else if (FAILED(impl->layout->HitTestPoint(x - impl->DrawOriginX(), y, &trailing, &inside, &metrics))) return {};
	if (charPosition) return {impl->BytePosition(metrics.textPosition), false};
	return {impl->BytePosition(metrics.textPosition + (trailing ? metrics.length : 0)), trailing != FALSE};
}

ParagraphPoint DirectWriteParagraph::CaretPoint(ParagraphCaret caret) const { return impl->Point(caret); }
std::vector<ParagraphRect> DirectWriteParagraph::RangeRectangles(size_t start, size_t end) const {
	return impl->Rectangles(start, end);
}

std::vector<ParagraphRange> DirectWriteParagraph::VisualRanges(size_t line, float left, float right) const {
	std::vector<ParagraphRange> ranges;
	if (!IsValid() || line >= impl->lines.size() || left == right) return ranges;
	if (left > right) std::swap(left, right);
	impl->EnsureStops();
	for (const auto &cluster : impl->visualClusters[line]) {
		if (cluster.centre >= left && cluster.centre < right) ranges.push_back(cluster.Range());
	}
	std::sort(ranges.begin(), ranges.end(), [](ParagraphRange a, ParagraphRange b) { return a.start < b.start; });
	size_t retained = 0;
	for (const auto range : ranges) {
		if (retained && ranges[retained - 1].start + ranges[retained - 1].length == range.start)
			ranges[retained - 1].length += range.length;
		else ranges[retained++] = range;
	}
	ranges.resize(retained);
	return ranges;
}

std::optional<ParagraphCaret> DirectWriteParagraph::VisualNeighbor(ParagraphCaret caret, int direction) const {
	if (!IsValid() || !direction) return std::nullopt;
	impl->EnsureStops();
	const auto currentStop = impl->FindStop(caret);
	const auto point = currentStop ? ParagraphPoint{} : impl->Point(caret);
	const float currentX = currentStop ? currentStop->x : point.x;
	const size_t line = currentStop ? currentStop->line : impl->CaretLine(caret);
	const auto &row = impl->stops[line];
	const Impl::Stop *candidate = nullptr;
	if (direction > 0) {
		auto next = std::upper_bound(row.begin(), row.end(), currentX + 0.001f,
			[](float x, const Impl::Stop &stop) { return x < stop.x; });
		if (next != row.end()) {
			candidate = &*next;
			for (; next != row.end() && std::abs(next->x - candidate->x) < 0.001f; ++next) {
				if (next->approach == 1) { candidate = &*next; break; }
			}
		}
	} else {
		auto next = std::lower_bound(row.begin(), row.end(), currentX - 0.001f,
			[](const Impl::Stop &stop, float x) { return stop.x < x; });
		if (next != row.begin()) {
			--next;
			candidate = &*next;
			for (;;) {
				if (std::abs(next->x - candidate->x) >= 0.001f) break;
				if (next->approach == -1) { candidate = &*next; break; }
				if (next == row.begin()) break;
				--next;
			}
		}
	}
	if (candidate) return candidate->Caret();
	const int rowDirection = impl->options.rtl ? -direction : direction;
	if (rowDirection > 0 && line + 1 < impl->lines.size()) return impl->Edge(line + 1, direction < 0);
	if (rowDirection < 0 && line > 0) return impl->Edge(line - 1, direction < 0);
	return std::nullopt;
}

ParagraphCaret DirectWriteParagraph::VisualEdge(size_t line, bool right) const {
	if (!IsValid()) return {};
	impl->EnsureStops();
	return impl->Edge(line, right);
}

}
