// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

#pragma once
#include <array>
#include <cstddef>
#include <cstdint>
#include <memory>
#include <span>
#include <string_view>
#include <vector>

namespace Scintilla::Internal { class Document; class UndoHistory; }

namespace WinUIEditor {

// Transient live-transfer state. Recovery content and viewport anchors are separate.
inline constexpr uint64_t MaximumEditorStateBytes = 256ULL * 1024 * 1024;
inline constexpr uint64_t MaximumEditorStateActions = 1000000;
inline constexpr uint64_t MaximumEditorStateSelections = 65536;

struct EditorStatePosition {
    uint64_t position = 0;
    uint64_t virtualSpace = 0;
    bool trailing = false;
    bool operator==(const EditorStatePosition &) const noexcept = default;
};
struct EditorStateRange {
    EditorStatePosition caret;
    EditorStatePosition anchor;
    bool operator==(const EditorStateRange &) const noexcept = default;
};
struct EditorViewState {
    std::vector<EditorStateRange> selections;
    EditorStateRange rectangle;
    uint32_t main = 0;
    uint32_t mode = 1;
    bool moveExtends = false;
    uint32_t tabWidth = 4;
    uint32_t indent = 4;
    bool readOnly = false;
    bool useTabs = false;
    bool overtype = false;
    bool additionalSelectionTyping = false;
    uint32_t virtualSpaceOptions = 0;
    uint32_t multiPaste = 0;
    bool hasPreferredX = false;
    double preferredXDips = 0;
    bool preferredFromRight = false;
    bool operator==(const EditorViewState &) const noexcept = default;
};

// Calls Read only on the document's UI thread, while a native read lease is held.
// Each action text is borrowed, never copied into another whole-history buffer.
class EditorStateWriter {
    Scintilla::Internal::Document &_document;
    std::vector<uint8_t> _prefix;
    std::array<uint8_t, 24> _actionHeader{};
    std::string_view _actionText;
    size_t _prefixOffset = 0;
    size_t _actionOffset = 0;
    int _action = 0;
    int _actions = 0;
    uint64_t _length = 0;
    uint64_t _written = 0;
    bool _actionLoaded = false;
public:
    EditorStateWriter(Scintilla::Internal::Document &document, uint64_t sequence,
        const EditorViewState &view);
    uint64_t Length() const noexcept { return _length; }
    size_t Read(std::span<uint8_t> output);
};

struct EditorState {
    uint64_t documentLength = 0;
    uint64_t sequence = 0;
    bool collectingUndo = true;
    EditorViewState view;
    std::unique_ptr<Scintilla::Internal::UndoHistory> undo;
    EditorState();
    ~EditorState();
    EditorState(EditorState &&) noexcept;
    EditorState &operator=(EditorState &&) noexcept;
};

// Hash verification belongs to the stream adapter. Nothing is published by Feed
// or Finish: the caller installs this detached state only after its hash matches.
class EditorStateReader {
    enum class Part { Header, Selection, ActionHeader, ActionText, Done };
    Part _part = Part::Header;
    EditorState _state;
    std::array<uint8_t, 216> _record{};
    size_t _recordOffset = 0;
    uint64_t _expectedBytes;
    uint64_t _received = 0;
    uint64_t _actions = 0;
    uint64_t _action = 0;
    uint64_t _selections = 0;
    uint64_t _textRemaining = 0;
    int _current = 0;
    int _save = 0;
    int _detach = -1;
    void ReadRecord();
    void NextAction() noexcept;
public:
    explicit EditorStateReader(uint64_t expectedBytes);
    void Feed(std::span<const uint8_t> input);
    EditorState Finish(uint64_t expectedDocumentLength, uint64_t expectedSequence);
};

}
