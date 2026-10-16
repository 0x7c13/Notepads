// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

#include <cstddef>
#include <stdexcept>
#include <string>
#include <string_view>
#include <vector>

#include "Debugging.h"
#include "Position.h"
#include "Selection.h"

size_t CheckSelectionDisplayCaret() {
    using namespace Scintilla::Internal;
    size_t checks = 0;
    const auto require = [&](bool condition, const char *message) {
        if (!condition) throw std::runtime_error(message);
        ++checks;
    };
    Selection selection;
    // abc [Hebrew run] def: visual selection from the Hebrew left edge
    // through 'd' merges into one logical range, whose endpoints differ
    // from its physical left corner.
    selection.SetSelection(SelectionRange(SelectionPosition(4), SelectionPosition(12)));
    auto corner = SelectionPosition(10, 3);
    corner.SetTrailingEdge(true);
    selection.Rectangular() = SelectionRange(corner, SelectionPosition(12));
    selection.selType = Selection::SelTypes::rectangle;
    const auto display = selection.CaretForDisplay();
    require(display.Position() == 10 && display.TrailingEdge() && display.VirtualSpace() == 3,
        "Rectangle display caret preserves its physical corner, affinity and virtual space");
    require(selection.MainCaret() == 4 && selection.RangeMain().Start().Position() == 4 &&
        selection.RangeMain().End().Position() == 12 && selection.Length() == 8,
        "Displaying an internal rectangle corner never rewrites its logical editing range");

    selection.selType = Selection::SelTypes::stream;
    require(selection.CaretForDisplay() == selection.RangeMain().caret,
        "Stream selection keeps its logical caret for display");
    selection.selType = Selection::SelTypes::thin;
    auto projected = SelectionPosition(27, 2);
    projected.SetTrailingEdge(true);
    selection.SetSelection(SelectionRange(projected));
    require(selection.CaretForDisplay().Position() == 27 && selection.CaretForDisplay().TrailingEdge() &&
        selection.CaretForDisplay().VirtualSpace() == 2,
        "Thin rectangle uses its already projected main-row caret without consulting the old corner");
    selection.selType = Selection::SelTypes::lines;
    require(selection.CaretForDisplay() == projected, "Line selection retains the main caret");
    return checks;
}
