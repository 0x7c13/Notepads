// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

#include <algorithm>
#include <bit>
#include <cmath>
#include <climits>
#include <cstring>
#include <iterator>
#include <map>
#include <memory>
#include <optional>
#include <stdexcept>
#include <string>
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
#include "NativeEditorState.h"

namespace WinUIEditor {
namespace {
constexpr uint64_t Magic = 0x315453504e; // NPST1, serialized little-endian.
constexpr size_t HeaderWords = 27;
constexpr size_t RangeWords = 6;
using namespace Scintilla::Internal;

void Require(bool condition) {
    if (!condition) throw std::invalid_argument("Invalid or unsupported live editor state.");
}
void Put(std::span<uint8_t> bytes, size_t word, uint64_t value) noexcept {
    for (size_t i = 0; i < 8; ++i) bytes[word * 8 + i] = static_cast<uint8_t>(value >> (i * 8));
}
uint64_t Get(std::span<const uint8_t> bytes, size_t word) noexcept {
    uint64_t value = 0;
    for (size_t i = 0; i < 8; ++i) value |= static_cast<uint64_t>(bytes[word * 8 + i]) << (i * 8);
    return value;
}
bool Boolean(uint64_t value) { Require(value <= 1); return value != 0; }
uint32_t Bounded(uint64_t value, uint64_t maximum) {
    Require(value <= maximum);
    return static_cast<uint32_t>(value);
}
void ValidatePosition(const EditorStatePosition &position, uint64_t length) {
    Require(position.position <= length && position.virtualSpace < 800000000);
}
void ValidateRange(const EditorStateRange &range, uint64_t length) {
    ValidatePosition(range.caret, length);
    ValidatePosition(range.anchor, length);
}
void ValidateView(const EditorViewState &view, uint64_t length) {
    Require(!view.selections.empty() && view.selections.size() <= MaximumEditorStateSelections &&
        view.main < view.selections.size() && view.mode <= 4 && view.tabWidth > 0 &&
        view.tabWidth <= 256 && view.indent <= 256 && view.virtualSpaceOptions <= 7 && view.multiPaste <= 1 &&
        std::isfinite(view.preferredXDips) && std::abs(view.preferredXDips) <= 1e12);
    for (const auto &range : view.selections) ValidateRange(range, length);
    ValidateRange(view.rectangle, length);
}
void PutRange(std::span<uint8_t> bytes, size_t word, const EditorStateRange &range) noexcept {
    Put(bytes, word, range.caret.position); Put(bytes, word + 1, range.caret.virtualSpace);
    Put(bytes, word + 2, range.caret.trailing);
    Put(bytes, word + 3, range.anchor.position); Put(bytes, word + 4, range.anchor.virtualSpace);
    Put(bytes, word + 5, range.anchor.trailing);
}
EditorStateRange GetRange(std::span<const uint8_t> bytes, size_t word) {
    return {{Get(bytes, word), Get(bytes, word + 1), Boolean(Get(bytes, word + 2))},
        {Get(bytes, word + 3), Get(bytes, word + 4), Boolean(Get(bytes, word + 5))}};
}
void ValidateAction(int type, uint64_t position, uint64_t length) {
    Require((type & ~0x101) == 0 && position <= INT32_MAX && length <= MaximumEditorStateBytes);
}
void CheckReplay(const UndoHistory &undo, int current, uint64_t documentLength) {
    // Check each actual deletion extent, not just the resulting document length.
    auto step = [](uint64_t &length, uint64_t position, uint64_t count, bool insertion) {
        Require(position <= length);
        if (insertion) { Require(count <= INT32_MAX - length); length += count; }
        else { Require(count <= length - position); length -= count; }
    };
    uint64_t length = documentLength;
    for (int i = current; i-- > 0;)
        step(length, static_cast<uint64_t>(undo.Position(i)), static_cast<uint64_t>(undo.Length(i)),
            (undo.Type(i) & 1) != 0);
    length = documentLength;
    for (int i = current; i < undo.Actions(); ++i)
        step(length, static_cast<uint64_t>(undo.Position(i)), static_cast<uint64_t>(undo.Length(i)),
            (undo.Type(i) & 1) == 0);
}
}

EditorStateWriter::EditorStateWriter(Scintilla::Internal::Document &document, uint64_t sequence,
    const EditorViewState &view) : _document(document), _actions(document.UndoActions()) {
    const auto length = static_cast<uint64_t>(document.Length());
    Require(length <= INT32_MAX && _actions >= 0 && static_cast<uint64_t>(_actions) <= MaximumEditorStateActions &&
        document.UndoSequenceDepth() == 0 && !document.TentativeActive());
    ValidateView(view, length);
    const auto current = document.UndoCurrent();
    const auto save = document.UndoSavePoint();
    const auto detach = document.UndoDetach();
    Require(current >= 0 && current <= _actions && save >= -1 && save <= _actions &&
        detach >= -1 && detach <= _actions && (save < 0 || detach == -1));
    _prefix.resize((HeaderWords + RangeWords * view.selections.size()) * 8);
    const uint64_t flags = static_cast<uint64_t>(view.readOnly) | (static_cast<uint64_t>(view.useTabs) << 1) |
        (static_cast<uint64_t>(view.overtype) << 2) | (static_cast<uint64_t>(view.additionalSelectionTyping) << 3);
    const uint64_t words[] = {Magic, 1, length, sequence, static_cast<uint64_t>(_actions),
        static_cast<uint64_t>(current), static_cast<uint64_t>(save + 1), static_cast<uint64_t>(detach + 1),
        view.selections.size(), view.main, view.mode, view.moveExtends, view.tabWidth, view.indent, flags,
        view.virtualSpaceOptions, view.multiPaste, view.hasPreferredX, std::bit_cast<uint64_t>(view.preferredXDips),
        view.preferredFromRight, document.IsCollectingUndo()};
    for (size_t i = 0; i < std::size(words); ++i) Put(_prefix, i, words[i]);
    PutRange(_prefix, 21, view.rectangle);
    for (size_t i = 0; i < view.selections.size(); ++i) PutRange(_prefix, HeaderWords + i * RangeWords, view.selections[i]);
    _length = _prefix.size();
    for (int i = 0; i < _actions; ++i) {
        const auto text = document.UndoActionText(i);
        ValidateAction(document.UndoActionType(i), static_cast<uint64_t>(document.UndoActionPosition(i)), text.size());
        Require(text.size() + _actionHeader.size() <= MaximumEditorStateBytes - _length);
        _length += _actionHeader.size() + text.size();
    }
}

size_t EditorStateWriter::Read(std::span<uint8_t> output) {
    const auto capacity = output.size();
    auto copy = [&output](std::span<const uint8_t> bytes) {
        const auto count = std::min(output.size(), bytes.size());
        if (count) std::memcpy(output.data(), bytes.data(), count);
        output = output.subspan(count);
        return count;
    };
    if (_prefixOffset < _prefix.size()) _prefixOffset += copy(std::span(_prefix).subspan(_prefixOffset));
    while (!output.empty() && _action < _actions) {
        if (!_actionLoaded) {
            _actionText = _document.UndoActionText(_action);
            Put(_actionHeader, 0, static_cast<uint64_t>(_document.UndoActionType(_action)));
            Put(_actionHeader, 1, static_cast<uint64_t>(_document.UndoActionPosition(_action)));
            Put(_actionHeader, 2, _actionText.size());
            _actionOffset = 0;
            _actionLoaded = true;
        }
        if (_actionOffset < _actionHeader.size())
            _actionOffset += copy(std::span(_actionHeader).subspan(_actionOffset));
        if (_actionOffset >= _actionHeader.size()) {
            const auto offset = _actionOffset - _actionHeader.size();
            const auto data = std::span(reinterpret_cast<const uint8_t *>(_actionText.data()), _actionText.size());
            _actionOffset += copy(data.subspan(offset));
        }
        if (_actionOffset == _actionHeader.size() + _actionText.size()) { ++_action; _actionLoaded = false; }
    }
    const auto count = capacity - output.size();
    _written += count;
    Require(_written <= _length);
    return count;
}

EditorState::EditorState() = default;
EditorState::~EditorState() = default;
EditorState::EditorState(EditorState &&) noexcept = default;
EditorState &EditorState::operator=(EditorState &&) noexcept = default;

EditorStateReader::EditorStateReader(uint64_t expectedBytes) : _expectedBytes(expectedBytes) {
    Require(expectedBytes >= HeaderWords * 8 + RangeWords * 8 && expectedBytes <= MaximumEditorStateBytes);
    _state.undo = std::make_unique<Scintilla::Internal::UndoHistory>();
}
void EditorStateReader::NextAction() noexcept {
    _part = _action == _actions ? Part::Done : Part::ActionHeader;
    _recordOffset = 0;
}
void EditorStateReader::ReadRecord() {
    using namespace Scintilla::Internal;
    if (_part == Part::Header) {
        Require(Get(_record, 0) == Magic && Get(_record, 1) == 1);
        _state.documentLength = Bounded(Get(_record, 2), INT32_MAX);
        _state.sequence = Get(_record, 3);
        _actions = Bounded(Get(_record, 4), MaximumEditorStateActions);
        _current = static_cast<int>(Bounded(Get(_record, 5), _actions));
        _save = static_cast<int>(Bounded(Get(_record, 6), _actions + 1)) - 1;
        _detach = static_cast<int>(Bounded(Get(_record, 7), _actions + 1)) - 1;
        Require(_save < 0 || _detach == -1);
        _selections = Bounded(Get(_record, 8), MaximumEditorStateSelections);
        Require(_selections > 0 && HeaderWords * 8 + _selections * RangeWords * 8 + _actions * 24 <= _expectedBytes);
        auto &view = _state.view;
        view.main = Bounded(Get(_record, 9), _selections - 1);
        view.mode = Bounded(Get(_record, 10), 4);
        view.moveExtends = Boolean(Get(_record, 11));
        view.tabWidth = Bounded(Get(_record, 12), 256); Require(view.tabWidth > 0);
        view.indent = Bounded(Get(_record, 13), 256);
        const auto flags = Bounded(Get(_record, 14), 15);
        view.readOnly = (flags & 1) != 0; view.useTabs = (flags & 2) != 0;
        view.overtype = (flags & 4) != 0; view.additionalSelectionTyping = (flags & 8) != 0;
        view.virtualSpaceOptions = Bounded(Get(_record, 15), 7);
        view.multiPaste = Bounded(Get(_record, 16), 1);
        view.hasPreferredX = Boolean(Get(_record, 17));
        view.preferredXDips = std::bit_cast<double>(Get(_record, 18));
        Require(std::isfinite(view.preferredXDips) && std::abs(view.preferredXDips) <= 1e12);
        view.preferredFromRight = Boolean(Get(_record, 19));
        _state.collectingUndo = Boolean(Get(_record, 20));
        view.rectangle = GetRange(_record, 21); ValidateRange(view.rectangle, _state.documentLength);
        view.selections.reserve(static_cast<size_t>(_selections));
        _part = Part::Selection;
    } else if (_part == Part::Selection) {
        const auto range = GetRange(_record, 0);
        ValidateRange(range, _state.documentLength);
        _state.view.selections.push_back(range);
        if (_state.view.selections.size() == _selections) NextAction();
    } else {
        const auto type = Bounded(Get(_record, 0), 0x101);
        const auto position = Get(_record, 1);
        _textRemaining = Get(_record, 2);
        ValidateAction(static_cast<int>(type), position, _textRemaining);
        Require(_textRemaining <= _expectedBytes - _received);
        _state.undo->PushUndoActionType(static_cast<int>(type), static_cast<Sci::Position>(position));
        if (_textRemaining) _part = Part::ActionText;
        else { ++_action; NextAction(); }
    }
    _recordOffset = 0;
}
void EditorStateReader::Feed(std::span<const uint8_t> input) {
    Require(input.size() <= _expectedBytes - _received);
    while (!input.empty()) {
        Require(_part != Part::Done);
        if (_part == Part::ActionText) {
            const auto count = static_cast<size_t>(std::min<uint64_t>(input.size(), _textRemaining));
            _state.undo->AppendLastUndoActionText({reinterpret_cast<const char *>(input.data()), count});
            _textRemaining -= count; _received += count; input = input.subspan(count);
            if (!_textRemaining) { ++_action; NextAction(); }
        } else {
            const size_t size = _part == Part::Header ? HeaderWords * 8 : _part == Part::Selection ? RangeWords * 8 : 24;
            const auto count = std::min(size - _recordOffset, input.size());
            std::memcpy(_record.data() + _recordOffset, input.data(), count);
            _recordOffset += count; _received += count; input = input.subspan(count);
            if (_recordOffset == size) ReadRecord();
        }
    }
}
EditorState EditorStateReader::Finish(uint64_t expectedDocumentLength, uint64_t expectedSequence) {
    Require(_part == Part::Done && _received == _expectedBytes && _state.undo &&
        _state.documentLength == expectedDocumentLength && _state.sequence == expectedSequence);
    ValidateView(_state.view, _state.documentLength);
    CheckReplay(*_state.undo, _current, _state.documentLength);
    _state.undo->SetSavePoint(_save);
    _state.undo->SetDetachPoint(_detach);
    _state.undo->SetCurrent(_current, static_cast<intptr_t>(_state.documentLength));
    return std::move(_state);
}
}
