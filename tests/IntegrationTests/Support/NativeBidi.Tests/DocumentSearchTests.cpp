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
#include "Selection.h"
#include "TextStoreConversions.h"

size_t CheckTextStoreOvertypeAdmission() {
    using namespace Scintilla;
    using namespace Scintilla::Internal;
    using namespace Scintilla::Internal::TextStoreConversions;
    size_t checks = 0;
    const auto require = [&](bool valid) {
        if (!valid) throw std::runtime_error("Text-store overtype admission regression.");
        ++checks;
    };
    struct Fixture { std::string_view bytes; Sci::Position position; LONG removed; };
    for (const auto fixture : std::array{
        Fixture{"x", 0, 1}, Fixture{"p\xe4\xb8\xadz", 1, 1},
        Fixture{"p\xf0\x9f\x98\x80z", 1, 2}, Fixture{"pe\xcc\x81z", 1, 1},
        Fixture{"p\tz", 1, 1}, Fixture{"p\x80z", 1, 1},
        Fixture{"p\r\nz", 1, 0}, Fixture{"p\r\nz", 2, 0},
        Fixture{"p\rz", 1, 0}, Fixture{"p\nz", 1, 0}, Fixture{"p", 1, 0}}) {
        struct Input { std::string_view bytes; LONG units; };
        for (const auto input : std::array{Input{"q", 1}, Input{"\xf0\x9f\x98\x80", 2}, Input{"abc", 3}}) {
            for (const LONG headroom : {0L, 1L, 3L}) {
                for (const bool nearEnd : {false, true}) {
                    Document document(DocumentOption::Default);
                    document.SetDBCSCodePage(CpUtf8);
                    document.InsertString(0, fixture.bytes);
                    const auto smallLength = document.CountUTF16(0, document.Length());
                    // Model the unchanged prefix/suffix numerically, while
                    // using a real Document for the overwritten scalar and edit.
                    // This reaches the ACP boundary without allocating 2 GiB.
                    const auto length = MaximumAcp - headroom;
                    const auto prefix = nearEnd ? length - smallLength : 0;
                    const auto suffix = length - prefix - smallLength;
                    const auto start = static_cast<LONG>(prefix + document.CountUTF16(0, fixture.position));
                    Selection selection;
                    selection.SetSelection(SelectionRange(SelectionPosition(fixture.position)));
                    size_t mapped = 0;
                    const auto map = [&](LONG acp, bool *withinSurrogatePair) {
                        require(acp == start);
                        *withinSurrogatePair = false;
                        ++mapped;
                        return fixture.position;
                    };
                    Replacement result{42, 43, 44};
                    const bool admitted = TryInputReplacement(document, selection, true,
                        length, start, start, input.units, map, result);
                    const bool expected = input.units - fixture.removed <= headroom;
                    require(admitted == expected);
                    if (!admitted) {
                        require(result.end == 42 && result.length == 43 && result.overwritten == 44);
                        continue;
                    }
                    const bool fallback = input.units > headroom;
                    require(mapped == (fallback ? 1U : 0U));
                    require(result.overwritten == (fallback ? fixture.removed : 0));
                    require(result.end == start + input.units);
                    require(result.length == length + input.units - result.overwritten);
                    if (fixture.removed) document.DelChar(fixture.position);
                    document.InsertString(fixture.position, input.bytes);
                    const auto actualLength = prefix + document.CountUTF16(0, document.Length()) + suffix;
                    require(actualLength == length + input.units - fixture.removed && actualLength <= MaximumAcp);
                    if (fallback) {
                        require(result.length == actualLength && ValidRange(result.end, result.end, result.length));
                        TextChangeResult change{};
                        require(TryTextChange(length, start, result.end, result.overwritten, input.units, change));
                        require(change.change.acpStart == start && change.change.acpOldEnd == start + fixture.removed &&
                            change.change.acpNewEnd == result.end && change.insertionStart == start);
                    }
                }
            }
        }
    }

    Document document(DocumentOption::Default);
    document.SetDBCSCodePage(CpUtf8);
    document.InsertString(0, "abc");
    // A single main-caret deletion must not hide growth from extra carets,
    // rectangle reprojection, or realization of virtual spaces.
    for (int shape = 0; shape < 7; ++shape) {
        Selection selection;
        selection.SetSelection(SelectionRange(SelectionPosition(0)));
        if (shape == 1) selection.AddSelection(SelectionRange(SelectionPosition(2)));
        if (shape == 2) selection.selType = Selection::SelTypes::rectangle;
        if (shape == 3) selection.selType = Selection::SelTypes::thin;
        if (shape == 4) selection.SetSelection(SelectionRange(SelectionPosition(0, 2)));
        if (shape == 5) selection.SetSelection(SelectionRange(SelectionPosition(0, 2), SelectionPosition(0, 1)));
        if (shape == 6) selection.SetSelection(SelectionRange(SelectionPosition(1), SelectionPosition(0)));
        size_t mapped = 0;
        const auto unexpectedMap = [&](LONG, bool *withinPair) {
            ++mapped;
            *withinPair = false;
            return Sci::Position{0};
        };
        Replacement result{42, 43, 44};
        require(!TryInputReplacement(document, selection, shape != 0,
            MaximumAcp, 0, 0, 1, unexpectedMap, result));
        require(result.end == 42 && result.length == 43 && result.overwritten == 44);
        require(TryInputReplacement(document, selection, true, 3, 0, 0, 1, unexpectedMap, result));
        require(result.end == 1 && result.length == 4 && result.overwritten == 0);
        require(TryInputReplacement(document, selection, true, MaximumAcp, 0, 1, 1, unexpectedMap, result));
        require(result.end == 1 && result.length == MaximumAcp && result.overwritten == 0);
        require(mapped == 0);
    }
    // A half-surrogate ACP maps beyond the scalar. It cannot justify deleting
    // that next character, nor can any invalid range reach the mapper.
    Selection selection;
    selection.SetSelection(SelectionRange(SelectionPosition(0)));
    Replacement result{42, 43, 44};
    require(!TryInputReplacement(document, selection, true, MaximumAcp, 1, 1, 1,
        [](LONG, bool *withinPair) { *withinPair = true; return Sci::Position{1}; }, result));
    require(result.end == 42 && result.length == 43 && result.overwritten == 44);
    struct Invalid { std::int64_t length; LONG start, end; std::int64_t inserted; };
    for (const auto invalid : std::array{
        Invalid{-1, 0, 0, 1}, Invalid{MaximumAcp + 1, 0, 0, 1}, Invalid{3, -1, -1, 1},
        Invalid{3, 4, 4, 1}, Invalid{3, 2, 1, 1}, Invalid{3, 0, 0, -1},
        Invalid{MaximumAcp, 0, 0, INT64_MAX}, Invalid{MaximumAcp, 2147483647, 2147483647, 1}}) {
        size_t mapped = 0;
        require(!TryInputReplacement(document, selection, true, invalid.length, invalid.start, invalid.end, invalid.inserted,
            [&](LONG, bool *withinPair) { ++mapped; *withinPair = false; return Sci::Position{0}; }, result));
        require(result.end == 42 && result.length == 43 && result.overwritten == 44);
        require(mapped == 0);
    }
    return checks;
}

size_t CheckUtf16EndIndex() {
    using namespace Scintilla;
    using namespace Scintilla::Internal;
    size_t checks = 0;
    const auto require = [&](bool valid) {
        if (!valid) throw std::runtime_error("The maintained UTF-16 end index must match document contents.");
        ++checks;
    };
    struct Fixture { std::string_view bytes; Sci::Position units; };
    for (const auto fixture : std::array{
        Fixture{"", 0}, Fixture{"a", 1}, Fixture{"one\rtwo\n", 8},
        Fixture{"\xe4\xb8\xad\xf0\x9f\x98\x80" "e\xcc\x81\r\n\xd7\x91", 8},
        Fixture{"\x80", 1}}) {
        Document document(DocumentOption::Default);
        document.SetDBCSCodePage(CpUtf8);
        document.AllocateLineCharacterIndex(LineCharacterIndexType::Utf16);
        document.InsertString(0, fixture.bytes);
        document.DeleteUndoHistory();
        const auto checkLength = [&](Sci::Position expected) {
            require(document.IndexLineStart(document.LinesTotal(), LineCharacterIndexType::Utf16) == expected);
            require(document.CountUTF16(0, document.Length()) == expected);
        };
        checkLength(fixture.units);
        document.InsertString(document.Length(), "\xf0\x9f\x98\x80");
        checkLength(fixture.units + 2);
        document.Undo();
        checkLength(fixture.units);
        document.Redo();
        checkLength(fixture.units + 2);
        document.DeleteChars(document.Length() - 4, 4);
        checkLength(fixture.units);
        document.ReleaseLineCharacterIndex(LineCharacterIndexType::Utf16);
    }
    return checks;
}

size_t CheckPreparedReplacementDecorations() {
    using namespace Scintilla;
    using namespace Scintilla::Internal;
    size_t checks = 0;
    const auto require = [&](bool condition) {
        if (!condition) throw std::runtime_error("Prepared replacement decoration regression.");
        ++checks;
    };
    const auto text = [](Document &document) {
        std::string result(static_cast<size_t>(document.Length()), '\0');
        document.GetCharRange(result.data(), 0, document.Length());
        return result;
    };
    const auto checkRuns = [&](Document &document) {
        document.decorations->SetCurrentIndicator(8);
        if (document.Length()) {
            document.decorations->FillRange(document.Length() - 1, 1, 1);
            require(document.decorations->ValueAt(8, document.Length() - 1) == 1);
        }
        for (const auto *decoration : document.decorations->View()) {
            require(decoration->Length() == document.Length());
            // The paint loop must progress even across unmarked runs and EOLs.
            for (Sci::Position position = 0; position < document.Length();) {
                const auto end = decoration->EndRun(position);
                require(end > position && end <= document.Length());
                position = end;
            }
        }
    };
    const std::array<std::string_view, 4> contents{"", "a", "b", "cat Cat scatter cat\rsecond line\rthird"};
    for (const auto before : contents) {
        for (const auto after : contents) {
            Document document(DocumentOption::Default);
            document.InsertString(0, before);
            document.SetSavePoint();
            checkRuns(document);
            Document candidate(DocumentOption::Default);
            candidate.SetUndoCollection(false);
            require(candidate.AddData(after.data(), static_cast<Sci_Position>(after.size())) == static_cast<int>(Status::Ok));
            checkRuns(candidate);
            document.PrepareWholeDocumentReplacement(candidate);
            document.ReplaceWithPreparedDocument(candidate);
            require(text(document) == after);
            checkRuns(document);
            if (!before.empty() || !after.empty()) {
                document.Undo();
                require(text(document) == before);
                checkRuns(document);
                document.Redo();
                require(text(document) == after);
                checkRuns(document);
            }
            // Also materialize a decoration after an empty replacement.
            document.InsertString(document.Length(), "!");
            checkRuns(document);
        }
    }
    return checks;
}

size_t CheckDocumentSearchBounds() {
    using namespace Scintilla;
    using namespace Scintilla::Internal;
    using namespace std::literals;
    constexpr auto text = "abcabcd\0c"sv;
    constexpr std::array queries{"ab"sv, "bc"sv, "cd"sv, "c"sv, "abcd"sv,
        "BC"sv, "c\0"sv, "d\0"sv, "\0c"sv, "\0"sv, "abcabcd\0c?"sv};
    size_t checks = 0;
    auto require = [&](bool condition, const std::string &message) {
        if (!condition) throw std::runtime_error(message);
        ++checks;
    };
    auto lower = [](std::string_view value) {
        std::string result(value);
        for (auto &ch : result) if (ch >= 'A' && ch <= 'Z') ch = static_cast<char>(ch + ('a' - 'A'));
        return result;
    };

    for (const int codePage : {0, CpUtf8}) {
        for (size_t gap = 0; gap <= text.size(); ++gap) {
            Document document(DocumentOption::Default);
            document.SetDBCSCodePage(codePage);
            if (codePage == CpUtf8) document.SetCaseFolder(std::make_unique<CaseFolderUnicode>());
            else document.SetCaseFolder(std::make_unique<CaseFolderTable>());
            document.InsertString(0, text);
            document.InsertString(static_cast<Sci::Position>(gap), "!");
            document.DeleteChars(static_cast<Sci::Position>(gap), 1);
            require(document.GapPosition() == static_cast<Sci::Position>(gap), "Search fixture places the buffer gap");
            for (size_t start = 0; start <= text.size(); ++start) {
                for (size_t end = start; end <= text.size(); ++end) {
                    for (const auto query : queries) {
                        for (const bool backward : {false, true}) {
                            for (const bool ignoreCase : {false, true}) {
                                // A bounded standard-library view is independent of Scintilla's
                                // split buffer, candidate scan and out-of-document CharAt behavior.
                                const auto range = text.substr(start, end - start);
                                const auto haystack = ignoreCase ? lower(range) : std::string(range);
                                const auto needle = ignoreCase ? lower(query) : std::string(query);
                                const auto found = backward ? haystack.rfind(needle) : haystack.find(needle);
                                const auto expected = found == std::string::npos ? Sci::Position{-1} :
                                    static_cast<Sci::Position>(start + found);
                                auto length = static_cast<Sci::Position>(query.size());
                                const auto actual = document.FindText(
                                    static_cast<Sci::Position>(backward ? end : start),
                                    static_cast<Sci::Position>(backward ? start : end), query.data(),
                                    ignoreCase ? FindOption::None : FindOption::MatchCase, &length);
                                require(actual == expected && (actual < 0 || length == static_cast<Sci::Position>(query.size())),
                                    "Search bounds: range=" + std::to_string(start) + ":" + std::to_string(end) +
                                    " gap=" + std::to_string(gap) + " backward=" + std::to_string(backward) +
                                    " ignoreCase=" + std::to_string(ignoreCase) + " queryBytes=" + std::to_string(query.size()) +
                                    " expected=" + std::to_string(expected) + " actual=" + std::to_string(actual));
                            }
                        }
                    }
                }
            }
        }
    }
    // Candidate limits constrain starts only: matches and whole-word checks
    // retain the complete document range, including across the buffer gap.
    for (const auto haystack : {"a\0b a\0b"sv, " prefix prefixX PREFIX "sv, "\xc3\x9f SS ss "sv, "\xe4\xb8\xad\xe6\x96\x87 x"sv}) {
        for (const auto query : {"a\0b"sv, "prefix"sv, "SS"sv, "\xc3\x9f"sv, "\xe4\xb8\xad\xe6\x96\x87"sv}) {
            Document document(DocumentOption::Default);
            document.SetDBCSCodePage(CpUtf8);
            document.SetCaseFolder(std::make_unique<CaseFolderUnicode>());
            document.InsertString(0, haystack);
            const auto gap = document.MovePositionOutsideChar(document.Length() / 2, 1, false);
            document.InsertString(gap, "!");
            document.DeleteChars(gap, 1);
            require(document.GapPosition() == gap, "Candidate-limit fixture places the buffer gap.");
            for (const auto flags : {FindOption::None, FindOption::MatchCase, FindOption::WholeWord}) {
                for (Sci::Position start = 0; start < document.Length(); start = document.NextPosition(start, 1)) {
                    auto expectedLength = static_cast<Sci::Position>(query.size());
                    const auto expected = document.FindText(start, document.Length(), query.data(), flags, &expectedLength);
                    for (auto end = start; end <= document.Length(); ++end) {
                        auto length = static_cast<Sci::Position>(query.size());
                        const auto actual = document.FindText(start, document.Length(), query.data(), flags, &length, end);
                        require(actual == (expected >= end ? -1 : expected) && (actual < 0 || length == expectedLength),
                            "Forward candidate limits preserve Unicode folding, NUL, whole words and crossing matches.");
                    }
                }
            }
        }
    }
    return checks;
}
