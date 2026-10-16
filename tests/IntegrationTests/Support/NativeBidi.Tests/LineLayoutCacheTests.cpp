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
