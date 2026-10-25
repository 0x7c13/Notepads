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
#include "UndoHistory.h"
#include "../../../../src/Notepads.Native/WinUIEditor/NativeEditorState.h"

namespace {
using namespace Scintilla;
using namespace Scintilla::Internal;
using namespace WinUIEditor;

void Require(bool condition, size_t &checks) {
    if (!condition) throw std::runtime_error("Live editor state transfer regression.");
    ++checks;
}
std::string Text(Document &document) {
    std::string text(static_cast<size_t>(document.Length()), '\0');
    document.GetCharRange(text.data(), 0, document.Length());
    return text;
}
std::vector<uint8_t> Encode(Document &document, const EditorViewState &view, size_t chunk, size_t &checks) {
    EditorStateWriter writer(document, 17, view);
    std::vector<uint8_t> bytes(static_cast<size_t>(writer.Length()));
    for (size_t offset = 0; offset < bytes.size();) {
        const auto count = writer.Read(std::span(bytes).subspan(offset, std::min(chunk, bytes.size() - offset)));
        Require(count > 0, checks);
        offset += count;
    }
    std::array<uint8_t, 1> extra{};
    Require(writer.Read(extra) == 0, checks);
    return bytes;
}
EditorState Decode(const std::vector<uint8_t> &bytes, uint64_t length, size_t chunk) {
    EditorStateReader reader(bytes.size());
    for (size_t i = 0; i < bytes.size(); i += chunk)
        reader.Feed(std::span(bytes).subspan(i, std::min(chunk, bytes.size() - i)));
    return reader.Finish(length, 17);
}
void Reject(const std::vector<uint8_t> &bytes, uint64_t length, size_t &checks) {
    bool rejected = false;
    try { auto ignored = Decode(bytes, length, 7); }
    catch (const std::invalid_argument &) { rejected = true; }
    catch (const std::runtime_error &) { rejected = true; }
    Require(rejected, checks);
}
void Word(std::vector<uint8_t> &bytes, size_t word, uint64_t value) {
    for (size_t i = 0; i < 8; ++i) bytes[word * 8 + i] = static_cast<uint8_t>(value >> (i * 8));
}
}

size_t CheckEditorStateTransfer() {
    size_t checks = 0;
    EditorViewState view;
    view.selections = {{{4, 0, true}, {1, 0, false}}, {{10, 2, false}, {10, 7, true}}};
    view.rectangle = view.selections[1];
    view.mode = 2; view.main = 1; view.moveExtends = true;
    view.hasPreferredX = true; view.preferredXDips = -32.5; view.preferredFromRight = true;
    view.tabWidth = 8; view.indent = 2; view.virtualSpaceOptions = 3; view.multiPaste = 1;
    view.overtype = true; view.additionalSelectionTyping = true;

    Document source(DocumentOption::Default);
    source.SetDBCSCodePage(CpUtf8);
    source.InsertString(0, "abc אבג\r中文"); source.DeleteUndoHistory();
    source.BeginUndoAction();
    source.DeleteChars(0, 3); source.InsertString(0, "XYZ");
    source.EndUndoAction(); source.SetSavePoint();
    source.BeginUndoAction(); source.InsertString(3, "!"); source.EndUndoAction();
    source.Undo(); // Retain a redo tail and a reachable save point.
    const auto bytes = Encode(source, view, 13, checks);
    for (const size_t chunk : {size_t(1), size_t(7), size_t(65536)}) {
        auto state = Decode(bytes, static_cast<uint64_t>(source.Length()), chunk);
        Require(state.view == view, checks); // Includes affinity, unlike SelectionPosition::operator==.
        Require(state.undo->Current() == source.UndoCurrent() && state.undo->SavePoint() == source.UndoSavePoint(), checks);
        Document target(DocumentOption::Default);
        target.SetDBCSCodePage(CpUtf8); target.InsertString(0, Text(source));
        target.ReplaceUndoHistory(std::move(state.undo), state.collectingUndo);
        Require(target.IsSavePoint(), checks);
        target.Undo(); Require(Text(target) == "abc אבג\r中文", checks);
        target.Redo(); Require(Text(target) == "XYZ אבג\r中文", checks);
        target.Redo(); Require(Text(target) == "XYZ! אבג\r中文", checks);
        target.Undo(); Require(Text(target) == "XYZ אבג\r中文" && target.IsSavePoint(), checks);
    }
    for (size_t end = 0; end < bytes.size(); ++end)
        Reject({bytes.begin(), bytes.begin() + static_cast<ptrdiff_t>(end)}, source.Length(), checks);
    for (const auto [word, value] : std::array<std::pair<size_t, uint64_t>, 16>{{
        {0, 0}, {1, 2}, {2, UINT64_MAX}, {3, 18}, {4, MaximumEditorStateActions + 1},
        {5, UINT64_MAX}, {8, 0}, {8, MaximumEditorStateSelections + 1}, {9, 2}, {10, 5},
        {11, 2}, {12, 0}, {14, 16}, {18, 0x7ff0000000000000}, {21, UINT64_MAX}, {23, 2}}}) {
        auto invalid = bytes; Word(invalid, word, value); Reject(invalid, source.Length(), checks);
    }
    // The first action follows the header and two selections. Reject unknown action
    // kinds, negative positions, and histories whose delete exceeds the real extent.
    for (const auto [word, value] : std::array<std::pair<size_t, uint64_t>, 3>{{
        {39, 2}, {40, UINT64_MAX}, {40, static_cast<uint64_t>(source.Length() + 1)}}}) {
        auto invalid = bytes; Word(invalid, word, value); Reject(invalid, source.Length(), checks);
    }
    auto extra = bytes; extra.push_back(0); Reject(extra, source.Length(), checks);

    source.Redo(); source.SetSavePoint(); source.Undo();
    source.InsertString(0, "new"); // Branch away from a saved redo tail: detached save point.
    auto detached = Decode(Encode(source, view, 64, checks), source.Length(), 11);
    Require(detached.undo->SavePoint() == source.UndoSavePoint() &&
        detached.undo->DetachPoint() == source.UndoDetach(), checks);

    Document empty(DocumentOption::Default);
    EditorViewState emptyView; emptyView.selections.push_back({});
    empty.SetUndoCollection(false);
    auto emptyState = Decode(Encode(empty, emptyView, 1, checks), 0, 1);
    Require(emptyState.undo->Actions() == 0 && !emptyState.collectingUndo, checks);
    empty.BeginUndoAction();
    bool rejected = false;
    try { EditorStateWriter writer(empty, 17, emptyView); }
    catch (const std::invalid_argument &) { rejected = true; }
    empty.EndUndoAction(); Require(rejected, checks);

    Document large(DocumentOption::Default);
    large.InsertString(0, std::string(200000, 'x'));
    auto largeState = Decode(Encode(large, emptyView, 65536, checks), large.Length(), 1023);
    Require(largeState.undo->Text(0) == std::string(200000, 'x'), checks);
    return checks;
}
