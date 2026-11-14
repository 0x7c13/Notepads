// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

#include <array>
#include <cstdint>
#include <limits>
#include <stdexcept>
#include "TextStoreConversions.h"

size_t CheckTextStoreConversions() {
    using namespace Scintilla::Internal::TextStoreConversions;
    size_t checks = 0;
    const auto require = [&](bool valid) {
        if (!valid) throw std::runtime_error("Text-store conversion boundary regression.");
        ++checks;
    };
    LONG result = 42;
    for (const auto value : {int64_t{0}, int64_t{1}, int64_t{2147483647}}) {
        require(TryAcp(value, result) && result == value);
    }
    for (const auto value : {int64_t{-1}, int64_t{2147483648}, INT64_MIN, INT64_MAX}) {
        result = 42;
        require(!TryAcp(value, result) && result == 42);
    }
    struct Addition { LONG start; int64_t delta; bool valid; LONG expected; };
    for (const auto test : std::array{
        Addition{0, 2147483647, true, 2147483647}, Addition{2147483647, 1, false, 0},
        Addition{2147483647, -2147483647, true, 0}, Addition{0, -1, false, 0},
        Addition{8, -3, true, 5}, Addition{8, 3, true, 11},
        Addition{1, INT64_MAX, false, 0}, Addition{2147483647, INT64_MIN, false, 0},
        Addition{-1, 1, false, 0}, Addition{2147483640, 7, true, 2147483647}}) {
        result = 42;
        require(TryAcpAdd(test.start, test.delta, result) == test.valid);
        require(result == (test.valid ? test.expected : 42));
    }
    require(ValidRange(0, 0, 0));
    require(ValidRange(2147483647, 2147483647, 2147483647));
    require(!ValidRange(0, 0, -1) && !ValidRange(0, 0, int64_t{2147483648}));
    require(!ValidRange(-1, 0, 10) && !ValidRange(5, 4, 10) && !ValidRange(0, 11, 10));

    struct ReplacementCase { int64_t length; LONG start, end; int64_t inserted; bool valid; LONG newEnd, newLength; };
    for (const auto test : std::array{
        ReplacementCase{10, 2, 5, 2, true, 4, 9},
        ReplacementCase{10, 2, 5, 0, true, 2, 7},
        ReplacementCase{2147483647, 2147483646, 2147483647, 1, true, 2147483647, 2147483647},
        ReplacementCase{2147483647, 2147483645, 2147483647, 2, true, 2147483647, 2147483647},
        ReplacementCase{2147483647, 0, 2147483647, 1, true, 1, 1},
        ReplacementCase{2147483647, 2147483647, 2147483647, 1, false, 0, 0},
        ReplacementCase{2147483647, 0, 0, 1, false, 0, 0},
        ReplacementCase{0, 0, 0, 0, true, 0, 0},
        ReplacementCase{10, 5, 4, 1, false, 0, 0},
        ReplacementCase{10, -1, 4, 1, false, 0, 0},
        ReplacementCase{10, 0, 11, 1, false, 0, 0},
        ReplacementCase{10, 0, 0, -1, false, 0, 0},
        ReplacementCase{10, 0, 0, INT64_MAX, false, 0, 0},
        ReplacementCase{10, 0, 0, 4294967295, false, 0, 0},
        ReplacementCase{2147483648, 0, 1, 0, false, 0, 0}}) {
        Replacement actual{42, 43};
        require(TryReplacement(test.length, test.start, test.end, test.inserted, actual) == test.valid);
        require(actual.end == (test.valid ? test.newEnd : 42) && actual.length == (test.valid ? test.newLength : 43));
    }

    struct Change { int64_t length; LONG oldEnd; int64_t actualEnd, removed, inserted; TS_TEXTCHANGE expected; LONG insertionStart{}; };
    for (const auto test : std::array{
        Change{10, 5, 4, 3, 2, {2, 5, 4}, 2}, // Ordinary replacement.
        Change{10, 5, 2, 3, 0, {2, 5, 2}, 2}, // Deletion.
        Change{10, 10, 12, 0, 2, {10, 10, 12}, 10}, // Append.
        Change{10, 7, 7, 2, 1, {6, 8, 7}, 6}, // Another row shifted the main selection.
        Change{10, 7, 5, 2, 1, {4, 6, 5}, 4}, // Another row shortened the prefix.
        Change{10, 10, 12, 1, 1, {0, 10, 12}, 11}, // Corrected old end is outside the old document.
        Change{9, 9, 11, 0, 1, {0, 9, 11}, 10}, // EOF insertion plus another caret: retain [10,11), not [0,11).
        Change{6, 6, 10, 0, 2, {0, 6, 10}, 8}, // A surrogate pair inserted at both carets.
        Change{2147483647, 2147483647, 2147483640, 20, 1, {0, 2147483647, 2147483640}, 2147483639},
        Change{10, 5, 1, 5, 2, {0, 5, 1}, 0}}) { // An input transformation shortened the insertion.
        TextChangeResult actual{};
        require(TryTextChange(test.length, test.oldEnd, test.actualEnd, test.removed, test.inserted, actual));
        require(actual.change.acpStart == test.expected.acpStart && actual.change.acpOldEnd == test.expected.acpOldEnd &&
            actual.change.acpNewEnd == test.expected.acpNewEnd);
        // The Classic TSF caller returns this range independently of change.
        // Using change.acpStart here would incorrectly include existing text.
        require(actual.insertionStart == test.insertionStart && actual.change.acpNewEnd == test.actualEnd);
    }
    for (const auto test : std::array{
        Change{10, 11, 5, 0, 1, {}}, Change{-1, 0, 0, 0, 0, {}},
        Change{10, 5, -1, 0, 1, {}}, Change{10, 5, 2147483648, 0, 1, {}},
        Change{10, 5, 5, -1, 1, {}}, Change{10, 5, 5, 0, INT64_MIN, {}}}) {
        TextChangeResult actual{{1, 2, 3}, 4};
        require(!TryTextChange(test.length, test.oldEnd, test.actualEnd, test.removed, test.inserted, actual));
        require(actual.change.acpStart == 1 && actual.change.acpOldEnd == 2 && actual.change.acpNewEnd == 3 && actual.insertionStart == 4);
    }

    struct Bounds { double left, top, right, bottom; RECT expected; };
    for (const auto test : std::array{
        Bounds{1, 2, 3, 4, {1, 2, 3, 4}},
        Bounds{-10.25, -4.75, 20.25, 15.5, {-11, -5, 21, 16}},
        Bounds{-1.9, -2.1, -0.1, -0.01, {-2, -3, 0, 0}},
        Bounds{0, 0, 0, 0, {0, 0, 0, 0}},
        Bounds{-2147483648.0, -2147483648.0, 2147483647.0, 2147483647.0,
            {-2147483647L - 1, -2147483647L - 1, 2147483647, 2147483647}}}) {
        RECT actual{};
        require(EnclosingRect(test.left, test.top, test.right, test.bottom, actual));
        require(EqualRect(&actual, &test.expected) != FALSE);
    }
    const double nan = std::numeric_limits<double>::quiet_NaN();
    const double infinity = std::numeric_limits<double>::infinity();
    for (const auto test : std::array{
        Bounds{nan, 0, 1, 1, {}}, Bounds{0, nan, 1, 1, {}},
        Bounds{0, 0, nan, 1, {}}, Bounds{0, 0, 1, nan, {}},
        Bounds{-infinity, 0, 1, 1, {}}, Bounds{0, 0, infinity, 1, {}},
        Bounds{-2147483648.25, 0, 1, 1, {}}, Bounds{0, 0, 2147483647.25, 1, {}},
        Bounds{0, -2147483648.25, 1, 1, {}}, Bounds{0, 0, 1, 2147483647.25, {}},
        Bounds{2, 0, 1, 1, {}}, Bounds{0, 2, 1, 1, {}}}) {
        RECT actual{1, 2, 3, 4};
        require(!EnclosingRect(test.left, test.top, test.right, test.bottom, actual));
        require(actual.left == 1 && actual.top == 2 && actual.right == 3 && actual.bottom == 4);
    }
    return checks;
}
