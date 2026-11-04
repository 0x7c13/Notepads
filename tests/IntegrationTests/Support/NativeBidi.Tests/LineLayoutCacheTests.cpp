// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

#include <algorithm>
#include <array>
#include <cstddef>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <cmath>
#include <iterator>
#include <map>
#include <memory>
#include <mutex>
#include <optional>
#include <set>
#include <stdexcept>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

#include "ScintillaTypes.h"
#include "ScintillaMessages.h"
#include "ILoader.h"
#include "ILexer.h"
#include "Debugging.h"
#include "Geometry.h"
#include "Platform.h"
#include "CharacterType.h"
#include "CharacterCategoryMap.h"
#include "Position.h"
#include "UniqueString.h"
#include "SplitVector.h"
#include "Partitioning.h"
#include "RunStyles.h"
#include "ContractionState.h"
#include "CellBuffer.h"
#include "KeyMap.h"
#include "Indicator.h"
#include "LineMarker.h"
#include "Style.h"
#include "ViewStyle.h"
#include "CharClassify.h"
#include "Decoration.h"
#include "CaseFolder.h"
#include "Document.h"
#include "UniConversion.h"
#include "DBCS.h"
#include "Selection.h"
#include "PositionCache.h"
#include "DirectWriteParagraph.h"

Scintilla::Internal::ViewStyle MakeGeometryTestStyle();

size_t CheckLineLayoutCache(IDWriteFactory *factory, IDWriteTextFormat *format) {
    using namespace Scintilla;
    using namespace Scintilla::Internal;
    constexpr uint64_t budget = 256 * 1024;
    constexpr int quarter = 64 * 1024 - 1; // Allocation includes one terminal byte.
    size_t checks = 0;
    const auto require = [&](bool condition, const char *message) {
        if (!condition) throw std::runtime_error(message);
        ++checks;
    };
    const auto retainedCapacity = [](const auto &entries) {
        uint64_t total = 0;
        for (const auto &entry : entries)
            if (const auto live = entry.lock()) total += static_cast<uint64_t>(live->maxLineLength) + 1;
        return total;
    };

    // Geometry layouts are owned by the physical line, with two immutable
    // contexts and no render-target state. Verify admission, keys and release.
    {
        class Geometry final : public IScreenLineLayout {
            bool cacheable;
        public:
            explicit Geometry(bool cacheable_ = true) : cacheable(cacheable_) {}
            bool IsCacheable() const noexcept override { return cacheable; }
            size_t PositionFromX(XYPOSITION, bool) override { return 0; }
            XYPOSITION XFromPosition(size_t) override { return 0; }
            std::vector<Interval> FindRangeIntervals(size_t, size_t) override { return {}; }
        };
        LineLayout row(0, 32);
        std::fill_n(row.styles.get(), 33, static_cast<unsigned char>(0));
        row.validity = LineLayout::ValidLevel::lines;
        const ScreenLineLayoutKey first{factory, format, 0, 16, 500, 20, 15, 40, 2, nullptr, 0};
        int creations = 0;
        auto get = [&](const ScreenLineLayoutKey &key) {
            if (auto cached = row.CachedScreenLayout(key)) return cached;
            ++creations;
            std::shared_ptr<IScreenLineLayout> layout = std::make_shared<Geometry>();
            row.CacheScreenLayout(key, layout);
            return layout;
        };
        const std::weak_ptr<IScreenLineLayout> firstWeak = get(first);
        for (int i = 0; i < 2000; ++i) require(get(first) == firstWeak.lock(), "Repeated geometry queries reuse one layout");
        require(creations == 1, "Geometry construction count is independent of indicator/query count");
        auto second = first;
        second.width = 600;
        const std::weak_ptr<IScreenLineLayout> secondWeak = get(second);
        require(get(first) == firstWeak.lock(), "Caret and indicator widths coexist");
        auto third = first;
        third.width = 700;
        get(third);
        require(secondWeak.expired() && !firstWeak.expired(), "Only two contexts are retained and least recent geometry is released");
        for (int field = 0; field < 11; ++field) {
            auto changed = first;
            switch (field) {
            case 0: changed.backend = &row; break;
            case 1: changed.style = &row; break;
            case 2: changed.start++; break;
            case 3: changed.length++; break;
            case 4: changed.width++; break;
            case 5: changed.height++; break;
            case 6: changed.baseline++; break;
            case 7: changed.tabWidth++; break;
            case 8: changed.tabMinimum++; break;
            case 9: changed.paragraph = reinterpret_cast<const IParagraphLayout *>(&row); break;
            case 10: changed.paragraphLine++; break;
            }
            row.ClearScreenLayouts();
            get(first);
            require(!row.CachedScreenLayout(changed), "Every geometry dependency participates in the key");
        }
        row.ClearScreenLayouts();
        auto uncached = first;
        uncached.backend = nullptr;
        get(uncached);
        require(!row.CachedScreenLayout(uncached), "Backends must explicitly opt into immutable geometry retention");
        auto healthy = get(first);
        auto failed = std::make_shared<Geometry>(false);
        const std::weak_ptr<IScreenLineLayout> failedWeak = failed;
        row.CacheScreenLayout(second, failed);
        require(!row.CachedScreenLayout(second) && row.CachedScreenLayout(first) == healthy,
            "Failed or incomplete geometry neither poisons a slot nor evicts a healthy layout");
        failed.reset();
        require(failedWeak.expired(), "A failed geometry fallback is released after its caller finishes");
        auto recovered = get(second);
        require(row.CachedScreenLayout(second) == recovered,
            "The next successful geometry construction is admitted and reused after a failure");
        healthy.reset();
        auto borrowed = get(first);
        row.Invalidate(LineLayout::ValidLevel::checkTextAndStyle);
        require(row.CachedScreenLayout(first) == borrowed, "An unchanged style-clock validation can retain geometry");
        row.Invalidate(LineLayout::ValidLevel::invalid);
        require(!row.CachedScreenLayout(first) && borrowed.use_count() == 1, "Invalidation releases the cache while in-flight queries remain owned");
        get(first);
        row.ClearPositions();
        require(!row.CachedScreenLayout(first), "Remeasuring text clears geometry");
        get(first);
        row.Resize(64);
        require(!row.CachedScreenLayout(first), "Growing source storage clears geometry");
        get(first);
        row.ReSet(1, 32);
        require(!row.CachedScreenLayout(first), "Reusing a cache slot for another line clears geometry");
        std::fill_n(row.styles.get(), 65, static_cast<unsigned char>(0));
        const Sci::Position braces[2]{2, 6};
        get(first);
        row.SetBracesHighlight(ForwardRange(0, 16), braces, StyleBraceLight, 0, false);
        require(!row.CachedScreenLayout(first), "Temporary brace fonts invalidate geometry");
        get(first);
        row.RestoreBracesHighlight(ForwardRange(0, 16), braces, false);
        require(!row.CachedScreenLayout(first), "Restoring brace fonts invalidates geometry");
        auto stable = get(first);
        row.SetBracesHighlight(ForwardRange(0, 16), braces, StyleBraceLight, 0, true);
        require(row.CachedScreenLayout(first) == stable, "Indicator-only brace changes retain geometry");
    }

    // Independent cumulative-width oracle for the compact 5.6.7 positions.
    // Include coordinates beyond float's exact integer range, tabs and an
    // empty terminal row; callers must never read a synthetic character -1.
    {
        class TabStops final : public TabStopProvider {
            XYPOSITION NextTabstopPos(Sci::Line, XYPOSITION x, XYPOSITION width) const noexcept override {
                return (std::floor(x / width) + 1) * width;
            }
        } tabs;
        auto style = MakeGeometryTestStyle();
        style.tabWidth = 64;
        style.lastSegItalicsOffset = 3;
        for (const int length : {0, 1, 4096, 1000000}) {
            LineLayout row(0, length);
            row.numCharsInLine = row.numCharsBeforeEOL = length;
            std::vector<double> expected(static_cast<size_t>(length) + 1);
            double x = 0;
            for (int index = 0; index < length; ++index) {
                const bool tab = index % 257 == 256;
                const float width = tab ? 0.0f : (index % 3 == 0 ? 38.125f : 31.75f);
                row.chars[index] = tab ? '\t' : 'x';
                row.styles[index] = 0;
                row.SetValue(index, width);
                x = tab ? (std::floor(x / 64) + 1) * 64 : x + width;
                expected[index + 1] = x;
            }
            row.chars[length] = 0;
            row.styles[length] = 0;
            row.CalculatePositions(0, tabs, style, true);
            require(row.GetPosition(0) == 0, "Empty and nonempty lines start at zero");
            for (int index = 0; index <= length; ++index) {
                require(std::abs(row.GetPosition(index) - expected[index]) < 0.01,
                    "Compact positions retain subpixel geometry on very long lines");
            }
            require(row.FindPositionFromX(x, Range(0, length), false) == length,
                "Hit testing the final edge returns the final byte");
        }
        LineLayout italic(0, 1);
        italic.chars[0] = 'x';
        italic.styles[0] = 0;
        italic.numCharsInLine = italic.numCharsBeforeEOL = 1;
        italic.SetValue(0, 12.5f);
        style.styles[0].italic = true;
        italic.CalculatePositions(0, tabs, style, true);
        require(italic.GetPosition(1) == 15.5, "Italic terminal ink keeps its trailing allowance");
        italic.numCharsInLine = italic.numCharsBeforeEOL = 0;
        italic.CalculatePositions(0, tabs, style, true);
        require(italic.GetPosition(0) == 0, "An empty italic row has no trailing glyph allowance");

        style.styles[0].italic = false;
        LineLayout tab(0, 1);
        tab.chars[0] = '\t';
        tab.styles[0] = 0;
        tab.numCharsInLine = tab.numCharsBeforeEOL = 1;
        style.styles[0].visible = false;
        tab.SetValue(0, 0);
        tab.CalculatePositions(0, tabs, style, true);
        require(tab.GetPosition(1) == 0, "An invisible tab does not expand to the next stop");
        tab.SetValue(0, 7);
        tab.CalculatePositions(0, tabs, style, true);
        require(tab.GetPosition(1) == 7, "An invisible tab keeps its replacement glyph width");
        style.styles[0].visible = true;
        tab.SetValue(0, 7);
        tab.CalculatePositions(0, tabs, style, false);
        require(tab.GetPosition(1) == 7, "A cleared tab representation keeps the measured width");
        tab.SetValue(0, 7);
        style.tabDrawMode = TabDrawMode::ControlChar;
        tab.CalculatePositions(0, tabs, style, true);
        require(tab.GetPosition(1) == 7, "A control-symbol tab keeps its measured width");
    }

    // The real repeated-measure/paint workload: retain long rows from the
    // visible page, regardless of the direction metadata stored on each row.
    for (const bool rtl : {false, true}) {
        LineLayoutCache cache;
        cache.SetLevel(LineCache::Page);
        std::array<std::weak_ptr<LineLayout>, 15> rows;
        for (size_t index = 0; index < rows.size(); ++index) {
            auto row = cache.Retrieve(static_cast<Sci::Line>(index), 0, 8191, 1, 32, 200);
            row->paragraphRightToLeft = rtl;
            row->validity = LineLayout::ValidLevel::lines;
            rows[index] = row;
        }
        for (size_t index = 0; index < rows.size(); ++index) {
            auto row = cache.Retrieve(static_cast<Sci::Line>(index), 0, 8191, 1, 32, 200);
            require(row == rows[index].lock() && row->validity == LineLayout::ValidLevel::lines &&
                row->paragraphRightToLeft == rtl, "A measured page reuses every long layout while painting");
        }
        require(cache.GetLevel() == LineCache::Page, "Long layouts do not permanently change the configured cache mode");
        require(retainedCapacity(rows) <= budget, "Visible page source capacity stays within the retained budget");
        auto retained = rows[3].lock();
        cache.Retrieve(3, 0, 8191, 2, 32, 200);
        require(retained->validity == LineLayout::ValidLevel::checkTextAndStyle,
            "A reused budgeted layout still receives style-clock invalidation");
        retained.reset();
        cache.Deallocate();
        require(retainedCapacity(rows) == 0, "Deallocate releases every cache-owned layout");
    }

    {
        LineLayoutCache cache;
        cache.SetLevel(LineCache::Page);
        std::array<std::weak_ptr<LineLayout>, 5> rows;
        for (size_t index = 0; index < 4; ++index)
            rows[index] = cache.Retrieve(static_cast<Sci::Line>(index), 0, quarter, 1, 32, 200);
        require(retainedCapacity(rows) == budget, "Four exact quarter-budget capacities fit without eviction");
        rows[4] = cache.Retrieve(4, 0, quarter, 1, 32, 200);
        require(!rows[0].expired() && rows[1].expired() && !rows[2].expired() && !rows[3].expired() && !rows[4].expired(),
            "Budget pressure evicts the farthest non-caret row and preserves the requested row");
        require(retainedCapacity(rows) == budget, "A fifth layout cannot silently exceed the source-capacity budget");
        const auto saved = rows[4].lock();
        require(cache.Retrieve(4, 0, 12, 1, 32, 200) == saved && saved->maxLineLength == quarter,
            "A shorter text request reuses and charges the retained allocation capacity");
    }

    {
        LineLayoutCache cache;
        cache.SetLevel(LineCache::Page);
        std::array<std::weak_ptr<LineLayout>, 3> rows;
        for (size_t index = 0; index < rows.size(); ++index)
            rows[index] = cache.Retrieve(static_cast<Sci::Line>(index), 0, quarter, 1, 32, 200);
        const auto old = rows[0];
        rows[0] = cache.Retrieve(0, 0, 200 * 1024, 1, 32, 200);
        require(old.expired() && rows[1].expired() && rows[2].expired(),
            "Growing one layout releases its old capacity and makes room before retaining the replacement");
        require(retainedCapacity(rows) == 200 * 1024 + 1, "Replacement capacity is counted exactly once");
        cache.Retrieve(0, 0, 12, 1, 32, 200);
        rows[1] = cache.Retrieve(1, 0, quarter, 1, 32, 200);
        require(rows[0].expired() && retainedCapacity(rows) == 64 * 1024,
            "A short request does not disguise a large retained capacity when another layout is admitted");
    }

    {
        LineLayoutCache cache;
        cache.SetLevel(LineCache::Page);
        std::array<std::weak_ptr<LineLayout>, 3> rows;
        rows[0] = cache.Retrieve(0, 0, quarter, 1, 32, 200);
        rows[1] = cache.Retrieve(1, 0, quarter, 1, 32, 200);
        rows[2] = cache.Retrieve(2, 0, 400 * 1024, 1, 32, 200);
        require(rows[0].expired() && rows[1].expired() && !rows[2].expired(),
            "An oversized current line is the only retained layout, including eviction of the caret row");
        const auto oversized = rows[2].lock();
        require(cache.Retrieve(2, 0, 400 * 1024, 1, 32, 200) == oversized,
            "An oversized current line is reusable instead of being reshaped on every query");
    }

    {
        LineLayoutCache cache;
        cache.SetLevel(LineCache::Page);
        auto active = cache.Retrieve(0, 0, 400 * 1024, 1, 32, 200);
        const std::weak_ptr<LineLayout> old = active;
        active->chars[0] = '!';
        auto next = cache.Retrieve(1, 0, 1024, 1, 32, 200);
        auto replacement = cache.Retrieve(0, 0, 1024, 1, 32, 200);
        require(active != replacement && active->chars[0] == '!' && active->maxLineLength == 400 * 1024,
            "Eviction preserves an active paint owner's original object and buffers");
        active.reset();
        require(old.expired(), "An evicted oversized layout is released after its final active owner finishes");
        require(next == cache.Retrieve(1, 0, 1024, 1, 32, 200),
            "Small-page reuse resumes after the oversized-line exception");
    }

    {
        LineLayoutCache cache;
        cache.SetLevel(LineCache::Page);
        std::array<std::weak_ptr<LineLayout>, 5> rows;
        for (size_t index = 0; index < 4; ++index)
            rows[index] = cache.Retrieve(static_cast<Sci::Line>(index), 0, quarter, 1, 32, 200);
        rows[4] = cache.Retrieve(4, 0, quarter, 1, 100, 200);
        require(retainedCapacity(rows) <= budget, "Growing page slot count preserves the capacity budget");
        cache.Retrieve(4, 0, quarter, 1, 5, 200);
        require(retainedCapacity(rows) <= budget && cache.GetLevel() == LineCache::Page,
            "Shrinking page slot count preserves the budget without changing cache mode");
        cache.SetLevel(LineCache::Caret);
        require(retainedCapacity(rows) == 0, "Changing cache mode releases the previous page entries");
        cache.SetLevel(LineCache::None);
        const auto uncached = cache.Retrieve(0, 0, 1024, 1, 32, 200);
        require(uncached != cache.Retrieve(0, 0, 1024, 1, 32, 200), "Explicit None mode still bypasses retention");
    }
    {
        LineLayoutCache cache;
        cache.SetLevel(LineCache::Page);
        auto row = cache.Retrieve(0, 0, 64, 1, 32, 200);
        const auto attachParagraph = [&] {
            ParagraphOptions options;
            options.width = 620;
            options.lineHeight = 34;
            options.baseline = 26;
            options.tabWidth = 48;
            auto paragraph = std::make_shared<DirectWriteParagraph>(factory, "abc", format,
                std::span<const ParagraphFontRun>{}, std::span<const ParagraphRepresentation>{}, options);
            require(paragraph->IsValid(), "Cache lifetime fixture owns a valid shaped paragraph");
            row->paragraph = paragraph;
            return std::weak_ptr<IParagraphLayout>(paragraph);
        };
        row->validity = LineLayout::ValidLevel::lines;
        auto paragraph = attachParagraph();
        cache.Retrieve(0, 0, 64, 2, 32, 200);
        require(!paragraph.expired() && row->validity == LineLayout::ValidLevel::checkTextAndStyle,
            "Style-clock revalidation retains an existing paragraph until text and styles are compared");
        cache.Retrieve(0, 0, 64, 3, 32, 200);
        require(!paragraph.expired(), "Repeated pending style checks do not discard an unchanged paragraph");
        row->Invalidate(LineLayout::ValidLevel::positions);
        require(paragraph.expired(), "A geometry invalidation discards a paragraph even while validation is pending");
        for (const auto previous : {LineLayout::ValidLevel::invalid, LineLayout::ValidLevel::positions}) {
            row->validity = previous;
            paragraph = attachParagraph();
            row->Invalidate(LineLayout::ValidLevel::checkTextAndStyle);
            require(paragraph.expired(), "A style check cannot resurrect a previously invalid paragraph");
        }
        row->validity = LineLayout::ValidLevel::lines;
        paragraph = attachParagraph();
        row->Invalidate(LineLayout::ValidLevel::invalid);
        require(paragraph.expired(), "A document or font invalidation releases the shaped paragraph immediately");
    }
    return checks;
}
