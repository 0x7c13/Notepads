// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "NativeSvgFontSessionTests.h"
#include <bcrypt.h>
#include <algorithm>
#include <cstring>
#include <limits>
#include <new>
#include <thread>
#include <dwrite_3.h>
#include <wrl/client.h>
#include <cwchar>
#include <iterator>

namespace Notepads::Markdown::Native
{
    namespace
    {
        notepads_markdown_core_error Error() noexcept
        {
            notepads_markdown_core_error error{};
            error.header = {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(error)};
            return error;
        }
        void Require(bool condition)
        {
            if (!condition)
                throw E_UNEXPECTED;
        }
        void Check(HRESULT hr)
        {
            if (hr != S_OK)
                throw hr;
        }
        class Buffer final
        {
          public:
            Buffer(notepads_markdown_svg_resources *resources, uint32_t kind, size_t bytes, bool &released)
                : _resources(resources), _released(released)
            {
                if (!bytes)
                    return;
                auto error = Error();
                Require(notepads_markdown_svg_resources_reserve(resources, kind, bytes, &_ticket, &error) == NOTEPADS_MARKDOWN_CORE_OK);
                _data = ::operator new(bytes, std::nothrow);
                if (!_data)
                {
                    (void)notepads_markdown_svg_resources_release_after_free(resources, &_ticket, &error);
                    throw E_OUTOFMEMORY;
                }
                if (notepads_markdown_svg_resources_admit(resources, &_ticket, bytes, &error) != NOTEPADS_MARKDOWN_CORE_OK)
                {
                    ::operator delete(_data);
                    _data = nullptr;
                    (void)notepads_markdown_svg_resources_release_after_free(resources, &_ticket, &error);
                    throw E_UNEXPECTED;
                }
            }
            ~Buffer()
            {
                if (!_data)
                    return;
                ::operator delete(_data);
                auto error = Error();
                _released &= notepads_markdown_svg_resources_release_after_free(_resources, &_ticket, &error) == NOTEPADS_MARKDOWN_CORE_OK;
            }
            template <class T> T *Data() noexcept
            {
                return static_cast<T *>(_data);
            }
            Buffer(Buffer const &) = delete;
            Buffer &operator=(Buffer const &) = delete;

          private:
            notepads_markdown_svg_resources *_resources{};
            bool &_released;
            notepads_markdown_svg_work_token _ticket{};
            void *_data{};
        };
        bool FontSha(uint8_t const *data, uint32_t bytes, uint8_t const expected[32])
        {
            BCRYPT_HASH_HANDLE hash{};
            Require(BCryptCreateHash(BCRYPT_SHA256_ALG_HANDLE, &hash, nullptr, 0, nullptr, 0, 0) >= 0);
            uint8_t actual[32]{};
            auto add = BCryptHashData(hash, const_cast<PUCHAR>(data), bytes, 0);
            auto finish = add >= 0 ? BCryptFinishHash(hash, actual, sizeof(actual), 0) : add;
            BCryptDestroyHash(hash);
            return add >= 0 && finish >= 0 && !std::memcmp(expected, actual, sizeof(actual));
        }
        bool Partition(MsFrPlan const &plan, uint32_t textLength)
        {
            if (!plan.generation || !plan.file_count || !plan.face_count || !plan.run_count)
                return false;
            uint32_t end{};
            for (uint32_t i = 0; i < plan.run_count; ++i)
            {
                auto const &run = plan.runs[i];
                if (run.utf8_start != end || !run.utf8_length || run.utf8_length > textLength - end ||
                    !run.face_ticket || run.face_ticket > plan.face_count)
                    return false;
                end += run.utf8_length;
            }
            return end == textLength;
        }

        using Microsoft::WRL::ComPtr;
        using FontIdentity = NativeSvgFontTestIdentity;
        bool SameIdentity(FontIdentity const &left, FontIdentity const &right)
        {
            return left.bytes == right.bytes && left.faceIndex == right.faceIndex &&
                   left.simulations == right.simulations && left.axisCount == right.axisCount &&
                   !std::memcmp(left.sha256, right.sha256, sizeof(left.sha256)) &&
                   !std::memcmp(left.axes, right.axes, left.axisCount * sizeof(MsFrAxis));
        }
        FontIdentity ReferenceIdentity(IDWriteFontFace5 *face, NativeSvgFontTestResult &result)
        {
            FontIdentity identity{};
            identity.faceIndex = face->GetIndex();
            identity.simulations = face->GetSimulations();
            identity.axisCount = face->GetFontAxisValueCount();
            Require(identity.axisCount <= std::size(identity.axes));
            DWRITE_FONT_AXIS_VALUE axes[16]{};
            Check(face->GetFontAxisValues(axes, identity.axisCount));
            for (uint32_t i = 0; i < identity.axisCount; ++i)
            {
                auto tag = static_cast<uint32_t>(axes[i].axisTag);
                identity.axes[i] = {(tag << 24) | ((tag & 0xff00) << 8) | ((tag >> 8) & 0xff00) | (tag >> 24),
                                    axes[i].value};
            }
            std::sort(std::begin(identity.axes), std::begin(identity.axes) + identity.axisCount,
                      [](auto const &a, auto const &b) { return a.tag_be < b.tag_be; });
            uint32_t count{};
            Check(face->GetFiles(&count, nullptr));
            Require(count == 1);
            ComPtr<IDWriteFontFile> file;
            Check(face->GetFiles(&count, file.GetAddressOf()));
            void const *key{};
            uint32_t keyBytes{};
            Check(file->GetReferenceKey(&key, &keyBytes));
            ComPtr<IDWriteFontFileLoader> loader;
            Check(file->GetLoader(&loader));
            ComPtr<IDWriteFontFileStream> stream;
            Check(loader->CreateStreamFromKey(key, keyBytes, &stream));
            Check(stream->GetFileSize(&identity.bytes));
            Require(identity.bytes && identity.bytes <= 64ull * 1024 * 1024);
            struct Hash
            {
                BCRYPT_HASH_HANDLE handle{};
                ~Hash()
                {
                    if (handle)
                        BCryptDestroyHash(handle);
                }
            } hash;
            Require(BCryptCreateHash(BCRYPT_SHA256_ALG_HANDLE, &hash.handle, nullptr, 0, nullptr, 0, 0) >= 0);
            for (uint64_t offset = 0; offset < identity.bytes;)
            {
                auto bytes = static_cast<uint32_t>((std::min)(65536ull, identity.bytes - offset));
                void const *fragment{};
                void *context{};
                Check(stream->ReadFileFragment(&fragment, offset, bytes, &context));
                ++result.referenceFragmentReads;
                auto status =
                    BCryptHashData(hash.handle, const_cast<PUCHAR>(static_cast<uint8_t const *>(fragment)), bytes, 0);
                stream->ReleaseFileFragment(context);
                ++result.referenceFragmentReleases;
                Require(status >= 0);
                offset += bytes;
            }
            Require(BCryptFinishHash(hash.handle, identity.sha256, sizeof(identity.sha256), 0) >= 0);
            return identity;
        }
        ComPtr<IDWriteFontFace5> NamedReference(IDWriteFontCollection *collection, wchar_t const *name)
        {
            uint32_t index{};
            BOOL exists{};
            Check(collection->FindFamilyName(name, &index, &exists));
            if (!exists)
                throw DWRITE_E_NOFONT;
            ComPtr<IDWriteFontFamily> family;
            Check(collection->GetFontFamily(index, &family));
            ComPtr<IDWriteFont> font;
            Check(family->GetFirstMatchingFont(DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_STRETCH_NORMAL,
                                               DWRITE_FONT_STYLE_NORMAL, &font));
            ComPtr<IDWriteFontFace> base;
            Check(font->CreateFontFace(&base));
            ComPtr<IDWriteFontFace5> face;
            Check(base.As(&face));
            return face;
        }
        void Coverage(IDWriteFontFace *face, uint32_t scalar, bool expected)
        {
            uint16_t glyph{};
            Check(face->GetGlyphIndices(&scalar, 1, &glyph));
            Require((glyph != 0) == expected);
        }
        // Independent one-scalar system-fallback control, with no list builder
        // or product parser. This proves the partial-list case distinguishes
        // selecting the second author family from prematurely using the system.
        class HanAnalysis final : public IDWriteTextAnalysisSource
        {
          public:
            HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid, void **output) override
            {
                if (!output)
                    return E_POINTER;
                *output = nullptr;
                if (iid != __uuidof(IUnknown) && iid != __uuidof(IDWriteTextAnalysisSource))
                    return E_NOINTERFACE;
                *output = static_cast<IDWriteTextAnalysisSource *>(this);
                AddRef();
                return S_OK;
            }
            ULONG STDMETHODCALLTYPE AddRef() override
            {
                return ++refs;
            }
            ULONG STDMETHODCALLTYPE Release() override
            {
                return --refs;
            }
            HRESULT STDMETHODCALLTYPE GetTextAtPosition(UINT32 position, WCHAR const **text, UINT32 *length) override
            {
                if (!text || !length || position > 1)
                    return E_INVALIDARG;
                *length = 1 - position;
                *text = position ? nullptr : L"\x4e2d";
                return S_OK;
            }
            HRESULT STDMETHODCALLTYPE GetTextBeforePosition(UINT32 position, WCHAR const **text,
                                                            UINT32 *length) override
            {
                if (!text || !length || position > 1)
                    return E_INVALIDARG;
                *length = position;
                *text = position ? L"\x4e2d" : nullptr;
                return S_OK;
            }
            DWRITE_READING_DIRECTION STDMETHODCALLTYPE GetParagraphReadingDirection() override
            {
                return DWRITE_READING_DIRECTION_LEFT_TO_RIGHT;
            }
            HRESULT STDMETHODCALLTYPE GetLocaleName(UINT32 position, UINT32 *length, WCHAR const **locale) override
            {
                if (!locale || !length || position > 1)
                    return E_INVALIDARG;
                *length = 1 - position;
                *locale = L"en-us";
                return S_OK;
            }
            HRESULT STDMETHODCALLTYPE GetNumberSubstitution(UINT32 position, UINT32 *length,
                                                            IDWriteNumberSubstitution **substitution) override
            {
                if (!substitution || !length || position > 1)
                    return E_INVALIDARG;
                *length = 1 - position;
                *substitution = nullptr;
                return S_OK;
            }

          private:
            ULONG refs{1};
        };
        NativeSvgFontTestResult RunMalformedFamilyTest(notepads_markdown_svg_resources *resources, uint32_t testCase) noexcept
        {
            NativeSvgFontTestResult result{};
            NativeSvgFontSession session;
            bool rejected{};
            try
            {
                static char const *const malformed[] = {"\"Bad\\q\", \"Arial\"", "\"Arial\", ",
                                                        "\"Arial\", \"unterminated", "\"Bad\\00D800 \", \"Arial\""};
                auto family = malformed[testCase - 25];
                uint8_t strings[128]{};
                auto length = static_cast<uint32_t>(std::strlen(family));
                Require(length + 3 < sizeof(strings));
                std::memcpy(strings, family, length);
                std::memcpy(strings + length, "und", 3);
                result.prerequisitesReady = true;
                result.prerequisiteStatus = S_OK;
                result.prerequisiteStage = 6;
                result.createStatus = NativeSvgFontSession::Create(resources, &session);
                Check(result.createStatus);
                auto callbacks = session.Callbacks();
                result.session = callbacks.session;
                MsFrStyle style{0, 1, 0, length, length, 3, 400, 0, 5, 0, 0, 0, 0, 18};
                MsFrLogicalSpan span{0, 1, 0x4c61746e, length, 3, 0, 0};
                MsFrDraft draft{};
                draft.size = sizeof(draft);
                draft.version = NOTEPADS_MARKDOWN_FR_ABI_VERSION;
                draft.draft_id = 1;
                draft.text = reinterpret_cast<uint8_t const *>("A");
                draft.text_length = 1;
                draft.styles = &style;
                draft.style_count = 1;
                draft.logical_spans = &span;
                draft.logical_span_count = 1;
                draft.strings = strings;
                draft.strings_length = length + 3;
                MsFrPlan plan{};
                result.workStatus = callbacks.plan(callbacks.session, &draft, &plan);
                result.failureOutputUntouched =
                    plan.size == 0 && plan.files == nullptr && plan.faces == nullptr && plan.runs == nullptr;
                notepads_markdown_svg_fault fault{};
                auto error = Error();
                Require(notepads_markdown_svg_resources_read_fault(resources, &fault, &error) == NOTEPADS_MARKDOWN_CORE_OK);
                result.originalFaultPreserved =
                    fault.kind == 1 && fault.native_hr_present && fault.native_hr == result.workStatus;
                rejected = result.workStatus == E_INVALIDARG && result.failureOutputUntouched &&
                           result.originalFaultPreserved && session.Statistics().fragmentReads == 0;
            }
            catch (...)
            {
                rejected = false;
            }
            NativeSvgFontRetirement retired{};
            result.cleanupStatus = session.FinishOnWorker(&retired);
            result.cleanupSucceeded = result.cleanupStatus == S_OK && retired.retired &&
                                      retired.remainingNativeBytes == 0 && retired.failedReleases == 0;
            result.fragmentReads = retired.fragmentReads;
            result.fragmentReleases = retired.fragmentReleases;
            result.fragmentPairing = retired.fragmentReads == retired.fragmentReleases;
            result.failedReleases = retired.failedReleases;
            result.passed = rejected && result.createStatus == S_OK && result.cleanupSucceeded &&
                            result.fragmentPairing && retired.firstWorkFault == result.workStatus;
            return result;
        }
        NativeSvgFontTestResult RunFamilyTest(notepads_markdown_svg_resources *resources, uint32_t testCase) noexcept
        {
            NativeSvgFontTestResult result{};
            NativeSvgFontSession session;
            bool released = true;
            bool workValidated = false;
            try
            {
                // Source syntax is tested in Rust. These are the reviewed wire
                // bytes, deliberately independent of the production encoder.
                static char const *const families[] = {"\"Arial\", \"Courier New\"",
                                                       "\"Courier New\", \"Arial\"",
                                                       "\"Notepads Absent Family 2026\", \"Courier New\"",
                                                       "\"Arial\", \"SimSun\"",
                                                       "serif",
                                                       "sans-serif",
                                                       "monospace",
                                                       "cursive",
                                                       "fantasy",
                                                       "\"serif\", \"Courier New\"",
                                                       "\"Notepads,Absent\", \"Courier New\"",
                                                       "\"Notepads\\\"Absent\", \"Courier New\"",
                                                       "\"Notepads\\\\Absent\", \"Courier New\"",
                                                       "\"Notepads\xE4\xB8\xAD\", \"Courier New\"",
                                                       "\"Notepads\\000001 Absent\", \"Courier New\"",
                                                       "\"Arial\", \"SimSun\", \"Segoe UI Emoji\"",
                                                       "\"Arial\", \"Courier New\""};
                static wchar_t const *const names[] = {
                    L"Arial",       L"Courier New", L"Courier New",   L"Arial",       L"Times New Roman",
                    L"Arial",       L"Courier New", L"Comic Sans MS", L"Impact",      L"Courier New",
                    L"Courier New", L"Courier New", L"Courier New",   L"Courier New", L"Courier New",
                    L"Arial",       L"Arial"};
                static wchar_t const *const absent[] = {L"Notepads Absent Family 2026",
                                                        L"serif",
                                                        L"Notepads,Absent",
                                                        L"Notepads\"Absent",
                                                        L"Notepads\\Absent",
                                                        L"Notepads\x4e2d",
                                                        L"Notepads\x0001"
                                                        L"Absent"};
                auto family = families[testCase - 8];
                char const *text = "A";
                if (testCase == 11)
                    text = "A\xE4\xB8\xAD";
                else if (testCase == 23)
                    text = "Ae\xCC\x81\xE4\xB8\xAD\xF0\x9F\x98\x80";
                else if (testCase == 24)
                    text = "\xD8\xA7\xD9\x84\xD8\xB9\xD8\xB1\xD8\xA8\xD9\x8A\xD8\xA9";
                auto textLength = static_cast<uint32_t>(std::strlen(text));
                result.prerequisiteStage = 1; // Independent factory/collection.
                ComPtr<IDWriteFactory2> factory;
                Check(DWriteCreateFactory(DWRITE_FACTORY_TYPE_ISOLATED, __uuidof(IDWriteFactory2),
                                          reinterpret_cast<IUnknown **>(factory.GetAddressOf())));
                ComPtr<IDWriteFontCollection> collection;
                Check(factory->GetSystemFontCollection(&collection));
                if (testCase == 10 || (testCase >= 17 && testCase <= 22))
                {
                    result.prerequisiteStage = 2; // Named first item must actually be absent.
                    auto missing = testCase == 10 ? absent[0] : absent[testCase - 16];
                    uint32_t index{};
                    BOOL exists{};
                    Check(collection->FindFamilyName(missing, &index, &exists));
                    Require(!exists);
                }
                result.witnessCount = testCase == 23 ? 3u : testCase == 11 ? 2u : 1u;
                result.prerequisiteStage = 3; // Named expected faces, bytes and axes.
                ComPtr<IDWriteFontFace5> expectedFaces[3];
                for (uint32_t i = 0; i < result.witnessCount; ++i)
                {
                    auto name = i == 2 ? L"Segoe UI Emoji" : i == 1 ? L"SimSun" : names[testCase - 8];
                    auto &witness = result.witnesses[i];
                    Require(std::wcslen(name) < std::size(witness.expectedFamily));
                    std::copy_n(name, std::wcslen(name) + 1, witness.expectedFamily);
                    expectedFaces[i] = NamedReference(collection.Get(), name);
                    witness.expected = ReferenceIdentity(expectedFaces[i].Get(), result);
                    witness.utf8Start = i == 2 ? 7u : i == 1 ? (testCase == 23 ? 4u : 1u) : 0u;
                    witness.utf8Length = i == 2                    ? 4u
                                         : i == 1                  ? 3u
                                         : result.witnessCount > 1 ? (testCase == 23 ? 4u : 1u)
                                                                   : textLength;
                }
                result.prerequisiteStage = 4; // Independent nominal coverage.
                Coverage(expectedFaces[0].Get(), testCase == 24 ? 0x627u : 0x41u, true);
                if (testCase == 11 || testCase == 23)
                {
                    Coverage(expectedFaces[0].Get(), 0x4e2d, false);
                    Coverage(expectedFaces[1].Get(), 0x4e2d, true);
                }
                if (testCase == 23)
                {
                    Coverage(expectedFaces[0].Get(), 0x65, true);
                    Coverage(expectedFaces[0].Get(), 0x301, true);
                    Coverage(expectedFaces[0].Get(), 0x1f600, false);
                    Coverage(expectedFaces[1].Get(), 0x1f600, false);
                    Coverage(expectedFaces[2].Get(), 0x1f600, true);
                }
                if (testCase == 24)
                    for (auto scalar : {0x627u, 0x644u, 0x639u, 0x631u, 0x628u, 0x64au, 0x629u})
                        Coverage(expectedFaces[0].Get(), scalar, true);
                if (testCase == 11)
                {
                    result.prerequisiteStage = 5; // Second family differs from system fallback.
                    ComPtr<IDWriteFontFallback> fallback;
                    Check(factory->GetSystemFontFallback(&fallback));
                    HanAnalysis analysis;
                    uint32_t mapped{};
                    float scale{};
                    ComPtr<IDWriteFont> font;
                    Check(fallback->MapCharacters(&analysis, 0, 1, collection.Get(), L"Arial",
                                                  DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_STYLE_NORMAL,
                                                  DWRITE_FONT_STRETCH_NORMAL, &mapped, &font, &scale));
                    Require(mapped == 1 && font && scale > 0);
                    ComPtr<IDWriteFontFace> base;
                    Check(font->CreateFontFace(&base));
                    ComPtr<IDWriteFontFace5> face;
                    Check(base.As(&face));
                    result.systemFallback = ReferenceIdentity(face.Get(), result);
                    result.systemFallbackDistinct = !SameIdentity(result.systemFallback, result.witnesses[1].expected);
                    Require(result.systemFallbackDistinct);
                }
                result.prerequisiteStatus = S_OK;
                result.prerequisitesReady = true;
                result.prerequisiteStage = 6; // All expectations frozen before product Create/Plan.
                result.createStatus = NativeSvgFontSession::Create(resources, &session);
                Check(result.createStatus);
                auto callbacks = session.Callbacks();
                result.session = callbacks.session;
                uint8_t strings[512]{};
                auto familyLength = static_cast<uint32_t>(std::strlen(family));
                Require(familyLength + 5 < sizeof(strings));
                std::memcpy(strings, family, familyLength);
                std::memcpy(strings + familyLength, "en-us", 5);
                MsFrStyle style{
                    0, textLength, 0, familyLength, familyLength, 5, 400, 0, 5, testCase == 24 ? 1u : 0u, 0, 0, 0, 18};
                MsFrLogicalSpan spans[3]{};
                for (uint32_t i = 0; i < result.witnessCount; ++i)
                    spans[i] = {result.witnesses[i].utf8Start,
                                result.witnesses[i].utf8Length,
                                i == 2           ? 0x5a797979u
                                : i == 1         ? 0x48616e69u
                                : testCase == 24 ? 0x41726162u
                                                 : 0x4c61746eu,
                                familyLength,
                                5,
                                testCase == 24 ? 1u : 0u,
                                0};
                MsFrDraft draft{};
                draft.size = sizeof(draft);
                draft.version = NOTEPADS_MARKDOWN_FR_ABI_VERSION;
                draft.draft_id = 1;
                draft.text = reinterpret_cast<uint8_t const *>(text);
                draft.text_length = textLength;
                draft.styles = &style;
                draft.style_count = 1;
                draft.logical_spans = spans;
                draft.logical_span_count = result.witnessCount;
                draft.strings = strings;
                draft.strings_length = familyLength + 5;
                MsFrPlan plan{};
                result.workStatus = callbacks.plan(callbacks.session, &draft, &plan);
                if (result.workStatus != S_OK)
                {
                    result.failureOutputUntouched =
                        plan.size == 0 && plan.files == nullptr && plan.faces == nullptr && plan.runs == nullptr;
                    notepads_markdown_svg_fault fault{};
                    auto error = Error();
                    Require(notepads_markdown_svg_resources_read_fault(resources, &fault, &error) == NOTEPADS_MARKDOWN_CORE_OK);
                    result.originalFaultPreserved = fault.native_hr_present && fault.native_hr == result.workStatus;
                }
                Check(result.workStatus);
                result.planPartition = Partition(plan, textLength);
                Require(result.planPartition);
                result.files = plan.file_count;
                result.faces = plan.face_count;
                result.runs = plan.run_count;
                result.actualFontSha = true;
                for (uint32_t i = 0; i < plan.file_count; ++i)
                {
                    auto const &file = plan.files[i];
                    Require(file.byte_length && file.byte_length <= 64ull * 1024 * 1024);
                    Buffer bytes(resources, NOTEPADS_MARKDOWN_SVG_RESOURCE_FONT, static_cast<size_t>(file.byte_length), released);
                    uint64_t written{};
                    Check(callbacks.fill_font(callbacks.session, file.ticket, bytes.Data<uint8_t>(), file.byte_length,
                                              &written));
                    result.actualFontSha &=
                        written == file.byte_length &&
                        FontSha(bytes.Data<uint8_t>(), static_cast<uint32_t>(file.byte_length), file.sha256);
                }
                Require(result.actualFontSha && released);
                result.selectedIdentitiesMatch = true;
                for (uint32_t i = 0; i < result.witnessCount; ++i)
                {
                    auto &witness = result.witnesses[i];
                    uint32_t covered{};
                    witness.matched = true;
                    for (uint32_t j = 0; j < plan.run_count; ++j)
                    {
                        auto const &run = plan.runs[j];
                        if (run.utf8_start >= witness.utf8Start + witness.utf8Length ||
                            run.utf8_start + run.utf8_length <= witness.utf8Start)
                            continue;
                        Require(run.utf8_start >= witness.utf8Start &&
                                run.utf8_length <= witness.utf8Start + witness.utf8Length - run.utf8_start);
                        auto const &face = plan.faces[run.face_ticket - 1];
                        Require(face.file_ticket && face.file_ticket <= plan.file_count && face.axis_count <= 16 &&
                                face.axis_start <= plan.axis_count &&
                                face.axis_count <= plan.axis_count - face.axis_start);
                        auto const &file = plan.files[face.file_ticket - 1];
                        FontIdentity actual{};
                        std::memcpy(actual.sha256, file.sha256, sizeof(actual.sha256));
                        actual.bytes = file.byte_length;
                        actual.faceIndex = face.face_index;
                        actual.simulations = face.simulations;
                        actual.axisCount = face.axis_count;
                        if (face.axis_count)
                            std::copy_n(plan.axes + face.axis_start, face.axis_count, actual.axes);
                        if (!covered)
                            witness.actual = actual;
                        witness.matched &= SameIdentity(witness.expected, actual) && run.fallback_scale == 1 &&
                                           run.bidi_level == spans[i].bidi_level && run.script_be == spans[i].script_be;
                        covered += run.utf8_length;
                    }
                    witness.matched &= covered == witness.utf8Length;
                    result.selectedIdentitiesMatch &= witness.matched;
                }
                Require(result.selectedIdentitiesMatch);
                workValidated = true;
            }
            catch (HRESULT hr)
            {
                if (!result.prerequisitesReady)
                    result.prerequisiteStatus = hr;
            }
            catch (...)
            {
                if (!result.prerequisitesReady)
                    result.prerequisiteStatus = E_FAIL;
            }
            if (result.prerequisitesReady)
            {
                NativeSvgFontRetirement retired{};
                result.cleanupStatus = session.FinishOnWorker(&retired);
                result.cleanupSucceeded = result.cleanupStatus == S_OK && retired.retired &&
                                          retired.remainingNativeBytes == 0 && retired.failedReleases == 0;
                result.fragmentReads = retired.fragmentReads;
                result.fragmentReleases = retired.fragmentReleases;
                result.failedReleases = retired.failedReleases;
                result.fragmentPairing = retired.fragmentReads == retired.fragmentReleases;
            }
            result.passed = workValidated && result.prerequisitesReady && result.createStatus == S_OK &&
                            result.workStatus == S_OK && result.planPartition && result.actualFontSha &&
                            result.selectedIdentitiesMatch && result.cleanupSucceeded && result.fragmentPairing &&
                            released && result.referenceFragmentReads == result.referenceFragmentReleases;
            return result;
        }
    }

    NativeSvgFontTestResult RunNativeSvgFontSessionTest(notepads_markdown_svg_resources *resources, notepads_markdown_core_cancel const *cancel,
                                                        uint32_t testCase) noexcept
    {
        NativeSvgFontTestResult result{};
        if (!resources || testCase >= NativeSvgFontTestCaseCount || ((testCase == 4 || testCase == 7) && !cancel))
            return result;
        if (testCase >= 25)
            return RunMalformedFamilyTest(resources, testCase);
        if (testCase >= 8)
            return RunFamilyTest(resources, testCase);
        NativeSvgFontSession session;
        bool released = true;
        try
        {
            if (testCase == 7)
                Require(notepads_markdown_core_cancel_request(cancel) == NOTEPADS_MARKDOWN_CORE_OK);
            result.createStatus = NativeSvgFontSession::Create(resources, &session);
            if (testCase == 7)
            {
                Require(result.createStatus != S_OK);
                result.workStatus = result.createStatus;
                NativeSvgFontRetirement retired{};
                result.cleanupStatus = session.FinishOnWorker(&retired);
                result.cleanupSucceeded = result.cleanupStatus == S_OK && retired.retired &&
                                          retired.remainingNativeBytes == 0 && retired.failedReleases == 0;
                notepads_markdown_svg_fault fault{};
                auto error = Error();
                Require(notepads_markdown_svg_resources_read_fault(resources, &fault, &error) == NOTEPADS_MARKDOWN_CORE_OK);
                result.originalFaultPreserved = fault.kind == 4 && retired.firstWorkFault == result.createStatus;
                result.fragmentPairing = retired.fragmentReads == retired.fragmentReleases;
                result.passed = result.cleanupSucceeded && result.originalFaultPreserved && result.fragmentPairing;
                return result;
            }
            Check(result.createStatus);
            auto callbacks = session.Callbacks();
            Require(callbacks.size == sizeof(callbacks) && callbacks.version == NOTEPADS_MARKDOWN_SVG_FONT_CALLBACKS_VERSION);
            result.session = callbacks.session;
            static uint8_t const latin[] = "Notepads A";
            static uint8_t const mixed[] = "A\xE4\xB8\xAD\xD8\xA7\xD9\x84\xD8\xB9\xD8\xB1\xD8\xA8\xD9\x8A\xD8\xA9"
                                           "e\xCC\x81\xF0\x9F\x98\x80";
            static uint8_t const ordinaryStrings[] = "\"Segoe UI\"und";
            static uint8_t const syntheticStrings[] = "\"Segoe UI Symbol\"und";
            bool synthetic = testCase == 2;
            auto text = testCase == 1 ? mixed : latin;
            uint32_t textLength = static_cast<uint32_t>(testCase == 1 ? sizeof(mixed) - 1 : sizeof(latin) - 1);
            uint32_t familyLength = static_cast<uint32_t>((synthetic ? sizeof(syntheticStrings) : sizeof(ordinaryStrings)) - sizeof("und"));
            MsFrStyle style{
                0, textLength, 0, familyLength, familyLength, 3, synthetic ? 700u : 400u, synthetic ? 1u : 0u, 5, 0, 0,
                0, 0,          18};
            MsFrLogicalSpan span{0, textLength, 0x4c61746e, familyLength, 3, 0, 0};
            MsFrLogicalSpan mixedSpans[] = {
                {0, 1, 0x4c61746e, familyLength, 3, 0, 0},  {1, 3, 0x48616e69, familyLength, 3, 0, 0},
                {4, 14, 0x41726162, familyLength, 3, 1, 0}, {18, 3, 0x4c61746e, familyLength, 3, 0, 0},
                {21, 4, 0x5a797979, familyLength, 3, 0, 0},
            };
            MsFrDraft draft{};
            draft.size = sizeof(draft);
            draft.version = NOTEPADS_MARKDOWN_FR_ABI_VERSION;
            draft.draft_id = 1;
            draft.text = text;
            draft.text_length = textLength;
            draft.styles = &style;
            draft.style_count = 1;
            draft.logical_spans = testCase == 1 ? mixedSpans : &span;
            draft.logical_span_count = testCase == 1 ? static_cast<uint32_t>(std::size(mixedSpans)) : 1;
            draft.strings = synthetic ? syntheticStrings : ordinaryStrings;
            draft.strings_length = familyLength + 3;
            MsFrPlan plan{};
            if (testCase == 3)
                draft.version = 999;
            if (testCase == 4)
                Require(notepads_markdown_core_cancel_request(cancel) == NOTEPADS_MARKDOWN_CORE_OK);
            if (testCase == 5)
            {
                HRESULT foreignPlan{}, foreignFinish{};
                std::thread thread(
                    [&]
                    {
                        MsFrPlan foreign{};
                        foreignPlan = callbacks.plan(callbacks.session, &draft, &foreign);
                        NativeSvgFontRetirement unchanged{};
                        unchanged.session = 0x7788;
                        foreignFinish = session.FinishOnWorker(&unchanged);
                        result.wrongThreadRejected = foreignPlan == RO_E_CLOSED && foreignFinish == RO_E_CLOSED &&
                                                     foreign.size == 0 && unchanged.session == 0x7788;
                    });
                thread.join();
                Require(result.wrongThreadRejected);
            }
            result.workStatus = callbacks.plan(callbacks.session, &draft, &plan);
            if (testCase == 3 || testCase == 4)
            {
                Require(result.workStatus != S_OK && plan.size == 0 && plan.files == nullptr);
                notepads_markdown_svg_fault fault{};
                auto error = Error();
                Require(notepads_markdown_svg_resources_read_fault(resources, &fault, &error) == NOTEPADS_MARKDOWN_CORE_OK);
                result.originalFaultPreserved =
                    testCase == 3 ? (fault.kind == 1 && fault.native_hr_present && fault.native_hr == E_INVALIDARG)
                                  : fault.kind == 4;
                Require(result.originalFaultPreserved);
            }
            else
            {
                Check(result.workStatus);
                result.planPartition = Partition(plan, textLength);
                Require(result.planPartition);
                result.files = plan.file_count;
                result.faces = plan.face_count;
                result.runs = plan.run_count;
                result.actualFontSha = true;
                for (uint32_t i = 0; i < plan.file_count; ++i)
                {
                    auto const &file = plan.files[i];
                    Require(file.byte_length && file.byte_length <= 64ull * 1024 * 1024);
                    Buffer bytes(resources, NOTEPADS_MARKDOWN_SVG_RESOURCE_FONT, static_cast<size_t>(file.byte_length), released);
                    uint64_t written{};
                    Check(callbacks.fill_font(callbacks.session, file.ticket, bytes.Data<uint8_t>(), file.byte_length,
                                              &written));
                    result.actualFontSha &=
                        written == file.byte_length &&
                        FontSha(bytes.Data<uint8_t>(), static_cast<uint32_t>(file.byte_length), file.sha256);
                }
                Require(result.actualFontSha && released);
                if (synthetic)
                {
                    auto face = std::find_if(plan.faces, plan.faces + plan.face_count,
                                             [](auto const &f) { return f.simulations != 0 && f.glyph_count > 1; });
                    Require(face != plan.faces + plan.face_count);
                    Buffer metrics(resources, NOTEPADS_MARKDOWN_SVG_RESOURCE_METRICS,
                                   size_t(face->glyph_count) * sizeof(MsFrGlyphMetrics), released);
                    uint32_t written{};
                    Check(callbacks.fill_metrics(callbacks.session, face->ticket, 0, metrics.Data<MsFrGlyphMetrics>(),
                                                 face->glyph_count, &written));
                    Require(written == face->glyph_count);
                    MsFrGlyphKey key{face->ticket, (std::min)(36u, face->glyph_count - 1), 0, 0};
                    MsFrGeometryReceipt receipt{};
                    Check(callbacks.prepare_geometry(callbacks.session, &key, 1, &receipt));
                    Buffer paths(resources, NOTEPADS_MARKDOWN_SVG_RESOURCE_GEOMETRY, sizeof(MsFrGlyphPath), released);
                    Buffer commands(resources, NOTEPADS_MARKDOWN_SVG_RESOURCE_GEOMETRY,
                                    size_t(receipt.command_count) * sizeof(MsFrCommand), released);
                    uint32_t pathWritten{}, commandWritten{};
                    Check(callbacks.fill_geometry(callbacks.session, receipt.generation, paths.Data<MsFrGlyphPath>(),
                                                  1, commands.Data<MsFrCommand>(), receipt.command_count,
                                                  &pathWritten, &commandWritten));
                    Require(pathWritten == 1 && commandWritten == receipt.command_count);
                    result.geometryCommands = commandWritten;
                    Check(callbacks.retire_geometry(callbacks.session, receipt.generation));
                    result.syntheticExercised = true;
                }
            }
            NativeSvgFontRetirement retired{};
            result.cleanupStatus = session.FinishOnWorker(&retired);
            result.cleanupSucceeded = result.cleanupStatus == S_OK && retired.retired &&
                                      retired.remainingNativeBytes == 0 && retired.failedReleases == 0;
            Require(result.cleanupSucceeded && released);
            result.fragmentReads = retired.fragmentReads;
            result.fragmentReleases = retired.fragmentReleases;
            result.fragmentPairing = retired.fragmentReads == retired.fragmentReleases;
            result.failedReleases = retired.failedReleases;
            if (testCase == 6)
            {
                NativeSvgFontSession following;
                Check(NativeSvgFontSession::Create(resources, &following));
                auto next = following.Callbacks();
                MsFrPlan expired{};
                auto stale = callbacks.plan(callbacks.session, &draft, &expired);
                result.lateIsolated = stale == RO_E_CLOSED && expired.size == 0 && next.session != callbacks.session &&
                                      following.Statistics().plans == 0;
                NativeSvgFontRetirement followingReceipt{};
                Check(following.FinishOnWorker(&followingReceipt));
                Require(result.lateIsolated && followingReceipt.retired);
            }
            result.passed = result.fragmentPairing;
        }
        catch (...)
        {
            NativeSvgFontRetirement retired{};
            auto cleanup = session.FinishOnWorker(&retired);
            if (cleanup != E_INVALIDARG)
            {
                result.cleanupStatus = cleanup;
                result.cleanupSucceeded = cleanup == S_OK && retired.retired;
                result.fragmentReads = retired.fragmentReads;
                result.fragmentReleases = retired.fragmentReleases;
            }
        }
        return result;
    }
}
