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
#include <iterator>
#include <map>
#include <memory>
#include <optional>
#include <stdexcept>
#include <string>
#include <string_view>
#include <vector>

#include "ScintillaTypes.h"
#include "ILoader.h"
#include "ILexer.h"
#include "Debugging.h"
#include "CharacterType.h"
#include "CharacterCategoryMap.h"
#include "Position.h"
#include "SplitVector.h"
#include "Partitioning.h"
#include "RunStyles.h"
#include "CellBuffer.h"
#include "CharClassify.h"
#include "Decoration.h"
#include "CaseFolder.h"
#include "Document.h"

namespace {
using namespace Scintilla;
using namespace Scintilla::Internal;

struct ExpectedRestore {
    std::string_view text;
    Sci::Position position;
    bool insertionEnd;
    size_t textSteps;
    size_t containerSteps = 0;
    bool finalContainer = false;
};

class UndoCaretFixture final : public DocWatcher {
    size_t &checks;
    std::string_view name;
    size_t completions = 0;
    size_t textSteps = 0;
    size_t containerSteps = 0;
    Sci::Position restoredPosition = -2;
    bool restoredInsertionEnd = false;
    ModificationFlags finalFlags = ModificationFlags::None;

    void Require(bool condition, std::string_view message) {
        if (!condition) throw std::runtime_error(std::string(name) + ": " + std::string(message));
        ++checks;
    }

public:
    Document document{DocumentOption::Default};

    UndoCaretFixture(size_t &checks_, std::string_view name_, std::string_view text) :
        checks(checks_), name(name_) {
        document.SetDBCSCodePage(CpUtf8);
        Require(document.InsertString(0, text) == static_cast<Sci::Position>(text.size()), "Load initial UTF-8 bytes");
        document.DeleteUndoHistory();
        Require(document.AddWatcher(this, nullptr), "Attach the real document watcher");
    }

    ~UndoCaretFixture() override { document.RemoveWatcher(this, nullptr); }

    void Restore(bool undo, const ExpectedRestore &expected) {
        completions = textSteps = containerSteps = 0;
        restoredPosition = -2;
        Require(undo ? document.CanUndo() : document.CanRedo(), "The requested history operation is available");
        const auto returnedPosition = undo ? document.Undo() : document.Redo();
        Require(returnedPosition == expected.position, "History returns the expected logical caret position");
        Require(completions == 1, "One user operation publishes exactly one final restoration notification");
        Require(restoredPosition == expected.position, "The final watcher notification retains that logical position");
        Require(restoredInsertionEnd == expected.insertionEnd, "The restoration edge belongs to the expected surviving text");
        Require(textSteps == expected.textSteps && containerSteps == expected.containerSteps,
            "The complete text/container group was replayed");
        Require(FlagSet(finalFlags, undo ? ModificationFlags::Undo : ModificationFlags::Redo),
            "The final notification identifies the requested history direction");
        if (expected.finalContainer) {
            Require(!FlagSet(finalFlags, ModificationFlags::InsertText | ModificationFlags::DeleteText),
                "A final container step carries restoration metadata without pretending to edit text");
        }
        std::string actual(static_cast<size_t>(document.Length()), '\0');
        document.GetCharRange(actual.data(), 0, document.Length());
        Require(actual == expected.text, "The complete document bytes match the expected history state");
    }

    void RequireHistoryBoundary(bool undone) {
        Require(document.CanUndo() == !undone && document.CanRedo() == undone,
            "History reaches the expected undo or redo boundary");
    }

    void NotifyModifyAttempt(Document *, void *) override {}
    void NotifySavePoint(Document *, void *, bool) override {}
    void NotifyDeleted(Document *, void *) noexcept override {}
    void NotifyStyleNeeded(Document *, void *, Sci::Position) override {}
    void NotifyErrorOccurred(Document *, void *, Status) override { Require(false, "Document reported an unexpected error"); }
    void NotifyGroupCompleted(Document *, void *) noexcept override {}
    void NotifyModified(Document *, DocModification modification, void *) override {
        if (!FlagSet(modification.modificationType, ModificationFlags::Undo | ModificationFlags::Redo)) return;
        if (FlagSet(modification.modificationType, ModificationFlags::InsertText | ModificationFlags::DeleteText)) ++textSteps;
        if (FlagSet(modification.modificationType, ModificationFlags::Container)) ++containerSteps;
        if (FlagSet(modification.modificationType, ModificationFlags::LastStepInUndoRedo)) {
            ++completions;
            restoredPosition = modification.newPos;
            restoredInsertionEnd = modification.newPosAtInsertionEnd;
            finalFlags = modification.modificationType;
        }
    }
};
}

size_t CheckUndoCaretRestoration() {
    size_t checks = 0;
    {
        UndoCaretFixture fixture(checks, "Insertion", "abc אבג");
        fixture.document.InsertString(4, "X");
        fixture.Restore(true, {"abc אבג", 4, false, 1});
        fixture.RequireHistoryBoundary(true);
        fixture.Restore(false, {"abc Xאבג", 5, true, 1});
        fixture.RequireHistoryBoundary(false);
    }
    {
        UndoCaretFixture fixture(checks, "Deletion", "abc Xאבג");
        fixture.document.DeleteChars(4, 1);
        fixture.Restore(true, {"abc Xאבג", 5, true, 1});
        fixture.RequireHistoryBoundary(true);
        fixture.Restore(false, {"abc אבג", 4, false, 1});
        fixture.RequireHistoryBoundary(false);
    }
    {
        UndoCaretFixture fixture(checks, "Consecutive Delete keys", "abc XYאבג");
        fixture.document.DeleteChars(4, 1);
        fixture.document.DeleteChars(4, 1);
        // Undo inserts Y and then X at byte 4. The final caret belongs after
        // the whole restored XY range, not merely after the last insertion X.
        fixture.Restore(true, {"abc XYאבג", 6, true, 2});
        fixture.RequireHistoryBoundary(true);
        fixture.Restore(false, {"abc אבג", 4, false, 2});
        fixture.RequireHistoryBoundary(false);
    }
    for (const bool replacement : {false, true}) {
        UndoCaretFixture fixture(checks, replacement ? "Container-wrapped replacement" : "Container-wrapped deletion", "abc Xאבג");
        fixture.document.BeginUndoAction();
        fixture.document.AddUndoAction(11, true);
        fixture.document.DeleteChars(4, 1);
        if (replacement) fixture.document.InsertString(4, "YZ");
        fixture.document.AddUndoAction(22, true);
        fixture.document.EndUndoAction();
        // Each history direction ends in a container action. Restoration must
        // carry the last text step's edge through that non-text notification.
        const size_t edits = replacement ? 2 : 1;
        fixture.Restore(true, {"abc Xאבג", 5, true, edits, 2, true});
        fixture.RequireHistoryBoundary(true);
        fixture.Restore(false, {replacement ? "abc YZאבג" : "abc אבג",
            replacement ? 6 : 4, replacement, edits, 2, true});
        fixture.RequireHistoryBoundary(false);
    }
    {
        UndoCaretFixture fixture(checks, "Container-only after text history", "abc אבג");
        fixture.document.BeginUndoAction();
        fixture.document.InsertString(4, "X");
        fixture.document.EndUndoAction();
        fixture.Restore(true, {"abc אבג", 4, false, 1});
        fixture.Restore(false, {"abc Xאבג", 5, true, 1});
        fixture.document.AddUndoAction(33, false);
        fixture.Restore(true, {"abc Xאבג", -1, false, 0, 1, true});
        fixture.Restore(true, {"abc אבג", 4, false, 1});
        fixture.RequireHistoryBoundary(true);
        fixture.Restore(false, {"abc Xאבג", 5, true, 1});
        fixture.Restore(false, {"abc Xאבג", -1, false, 0, 1, true});
        fixture.RequireHistoryBoundary(false);
    }
    return checks;
}
