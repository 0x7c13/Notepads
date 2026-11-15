// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "NativeReadingOutlineTests.h"
#include "Reading/NativeReadingOutline.h"
#include "Rendering/NativeDocumentRenderer.h"
#include <array>
#include <cmath>
#include <exception>
#include <winrt/Windows.Foundation.Collections.h>

namespace Notepads::Markdown::Native::ReadingOutlineTests
{
    namespace
    {
        void Require(bool valid, wchar_t const *detail)
        {
            if (!valid)
                throw winrt::hresult_error(E_FAIL, detail);
        }

        template <typename T> T Pod() noexcept
        {
            T value{};
            value.header = {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(T)};
            return value;
        }

        template <typename Body> void ExpectHresult(HRESULT expected, Body &&body)
        {
            try
            {
                body();
            }
            catch (winrt::hresult_error const &error)
            {
                Require(error.code().value == expected, L"outline exact expected HRESULT");
                return;
            }
            throw winrt::hresult_error(E_FAIL, L"outline expected failure was not raised");
        }

        struct CancelOwner
        {
            notepads_markdown_core_cancel *value{};
            ~CancelOwner() noexcept
            {
                if (value)
                    (void)notepads_markdown_core_cancel_destroy(value);
            }
        };

        struct Reservation
        {
            notepads_markdown_preview_allocation *value{};
            ~Reservation() noexcept
            {
                if (value)
                    (void)notepads_markdown_preview_allocation_destroy(value);
            }
            void Reset()
            {
                Require(notepads_markdown_preview_allocation_destroy(value) == NOTEPADS_MARKDOWN_CORE_OK, L"outline reservation retirement");
                value = nullptr;
            }
        };

        uint64_t Headroom(notepads_markdown_core_model *model, notepads_markdown_core_cancel *flag)
        {
            uint64_t low{}, high = 256ull * 1024 * 1024;
            while (low < high)
            {
                auto requested = low + (high - low + 1) / 2;
                auto error = Pod<notepads_markdown_core_error>();
                Reservation reservation;
                auto status = notepads_markdown_preview_model_reserve(model, flag, requested, &reservation.value, &error);
                if (status == NOTEPADS_MARKDOWN_CORE_OK)
                {
                    Require(reservation.value != nullptr, L"outline headroom has a real reservation");
                    reservation.Reset();
                    low = requested;
                }
                else
                {
                    Require(status == NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT && error.status == status &&
                                error.limit_kind == NOTEPADS_MARKDOWN_CORE_AUXILIARY_CAPACITY && !reservation.value,
                            L"outline headroom only interprets actual auxiliary exhaustion as a capacity limit");
                    high = requested - 1;
                }
            }
            return low;
        }

        void CheckDirectory(std::shared_ptr<ReadingOutline const> const &outline,
                            MainTextSnapshot const &main, Results &results)
        {
            Require(outline && outline->Memory() == main.Memory() &&
                        outline->TextGeneration() == main.TextGeneration(),
                    L"outline transports the actual same-model reading identity");
            ++results.directoryChecks;
            struct Expected
            {
                std::wstring_view title;
                std::wstring_view anchor;
                uint32_t start;
                uint32_t end;
            };
            constexpr std::array expected{Expected{L"a", L"a", 0, 1}, Expected{L"a", L"a-1", 2, 3},
                                          Expected{L"a-1", L"a-1-1", 4, 7}, Expected{L"a", L"a-2", 8, 9}};
            auto rows = outline->Headings();
            Require(rows.size() == expected.size(), L"outline count matches the original suffix-collision test");
            ++results.directoryChecks;
            for (size_t index = 0; index < rows.size(); ++index)
            {
                auto const &row = rows[index];
                auto const &oracle = expected[index];
                Require(row.node != 0 && row.level == 1 && row.text.start == oracle.start && row.text.end == oracle.end &&
                            std::wstring_view{row.title} == oracle.title && std::wstring_view{row.anchor} == oracle.anchor,
                        L"outline row/order/title/range/anchor matches the independent upstream oracle");
                Require(outline->FindAnchor(oracle.anchor) == index, L"anchor resolves the document-order row");
                results.directoryChecks += 2;
            }
            for (std::wstring_view absent : {L"A", L"#a", L"a-3", L"a-1-2", L""})
            {
                Require(!outline->FindAnchor(absent), L"outline lookup is exact, without case folding or slugifying");
                ++results.directoryChecks;
            }
        }

        void CheckFragments(std::shared_ptr<ReadingMemory> const &memory, Results &results)
        {
            struct Positive
            {
                std::wstring_view input;
                std::wstring_view expected;
            };
            constexpr std::array positives{
                Positive{L"#", L""}, Positive{L"#hi", L"hi"}, Positive{L"#Hi", L"Hi"},
                Positive{L"#%68%69", L"hi"}, Positive{L"#%E4%B8%AD", L"中"}, Positive{L"#中文", L"中文"},
                Positive{L"#中%e6%96%87", L"中文"}, Positive{L"#%F0%9F%98%80", L"😀"}, Positive{L"#😀", L"😀"},
                Positive{L"#e%CC%81", L"e\u0301"}, Positive{L"#e\u0301", L"e\u0301"},
                Positive{L"#%252D", L"%2D"}, Positive{L"#%25E4%25B8%25AD", L"%E4%B8%AD"},
                Positive{L"#%23", L"#"}, Positive{L"##", L"#"}, Positive{L"#+", L"+"}, Positive{L"#%2B", L"+"},
                Positive{L"#%20", L" "}, Positive{L"#x+y", L"x+y"}, Positive{L"#%2f", L"/"}, Positive{L"#%3F", L"?"},
                Positive{L"#%26amp%3B", L"&amp;"}, Positive{L"#%EF%BB%BF", L"\uFEFF"},
                Positive{L"#%C2%80", L"\u0080"}, Positive{L"#%F4%8F%BF%BF", L"\U0010FFFF"},
                Positive{L"#%ED%9F%BF", L"\uD7FF"}, Positive{L"#%EE%80%80", L"\uE000"},
                Positive{L"#%7f", L"\x007F"}, Positive{L"#%30", L"0"}};
            for (auto const &test : positives)
            {
                auto actual = DecodeReadingFragment(test.input, memory);
                Require(actual && std::wstring_view{*actual} == test.expected,
                        L"fragment strict single decode matches its independent scalar oracle");
                ++results.fragmentChecks;
            }
            constexpr std::array<std::wstring_view, 34> negatives{
                L"", L"hi", L"other.md#hi", L"https://example/#hi", L"C:\\file#hi", L"file:#hi", L" #hi",
                L"%23hi", L"%", L"#%00", L"#%", L"#%0", L"#%0G", L"#%GG", L"#%g0", L"#%80",
                L"#%C0%AF", L"#%C1%BF", L"#%C2", L"#%C2A", L"#%E0%80%AF", L"#%ED%A0%80", L"#%ED%BF%BF",
                L"#%F4%90%80%80", L"#%F5%80%80%80", L"#%F0%9F%98", L"#%EF%BF", L"#%FE", L"#%FF",
                L"#%F8%88%80%80%80", L"#%E4中", L"#中%B8%AD", L"#%F0%9F😀", L"#%C2%"};
            for (auto test : negatives)
            {
                Require(!DecodeReadingFragment(test, memory), L"invalid/non-fragment input is refused without coercion");
                ++results.fragmentChecks;
            }
            std::array<std::array<wchar_t, 3>, 5> invalidScalars{{
                {L'#', wchar_t{0}, L'a'},
                {L'#', wchar_t{0xD800}, L'a'},
                {L'#', wchar_t{0xDC00}, L'a'},
                {L'#', L'a', wchar_t{0xDBFF}},
                {L'#', wchar_t{0xD800}, wchar_t{0xDBFF}}}};
            for (auto const &test : invalidScalars)
            {
                Require(!DecodeReadingFragment({test.data(), test.size()}, memory),
                        L"raw NUL and every unpaired UTF16 direction are refused");
                ++results.fragmentChecks;
            }
        }

        using PageHeading = winrt::Notepads::Markdown::MarkdownHeading;
        using PageView = winrt::Windows::Foundation::Collections::IVectorView<PageHeading>;
        using ExternalHeadingAbi = winrt::impl::abi_t<winrt::Notepads::Markdown::IMarkdownHeading>;

        struct PageReentrancyState
        {
            PageView *externalPage{};
            notepads_markdown_core_model *model{};
            notepads_markdown_core_cancel *allocationFlag{};
            uint64_t occupiedHeadroom{};
            bool armed{};
            bool entered{};
            bool externalReferenceCleared{};
            bool remainedPaidInCallback{};
            bool callbackFinished{};
            bool foreignDestroyed{};
            std::exception_ptr failure;

            void OnUnknownQuery() noexcept
            {
                if (!armed || entered)
                    return;
                entered = true;
                try
                {
                    Require(Headroom(model, allocationFlag) == occupiedHeadroom,
                            L"actual page remains funded immediately before the foreign identity callback");
                    // This is the caller's only page reference. State stores a
                    // borrowed address, not another COM/page/iterator owner.
                    *externalPage = nullptr;
                    externalReferenceCleared = !*externalPage;
                    remainedPaidInCallback = Headroom(model, allocationFlag) == occupiedHeadroom;
                    Require(externalReferenceCleared && remainedPaidInCallback,
                            L"IndexOf self pin retains the actual paid page after its last external owner is cleared");
                    callbackFinished = true;
                }
                catch (...)
                {
                    // COM QI must not throw across the ABI. The UI test rethrows
                    // this exact failure after the real IndexOf call returns.
                    failure = std::current_exception();
                }
            }
        };

        // A legitimate separate Heading COM identity, with all value methods
        // delegated to a live actual Heading. Only QI(IUnknown) adds the
        // deliberate callback. The caller owns the foreign object through a
        // projected Heading for the entire raw-ABI IndexOf invocation.
        class ReentrantHeading final : public ExternalHeadingAbi
        {
          public:
            ReentrantHeading(PageHeading value, PageReentrancyState &state) noexcept
                : _value(std::move(value)), _state(state)
            {
            }
            ~ReentrantHeading()
            {
                _state.foreignDestroyed = true;
            }
            int32_t __stdcall QueryInterface(winrt::guid const &id, void **object) noexcept override
            {
                if (!object)
                    return E_POINTER;
                *object = nullptr;
                if (id != winrt::guid_of<winrt::Windows::Foundation::IUnknown>() &&
                    id != winrt::guid_of<winrt::Windows::Foundation::IInspectable>() &&
                    id != winrt::guid_of<winrt::Notepads::Markdown::IMarkdownHeading>())
                    return E_NOINTERFACE;
                if (id == winrt::guid_of<winrt::Windows::Foundation::IUnknown>())
                {
                    _state.OnUnknownQuery();
                    if (_state.failure)
                        return E_UNEXPECTED;
                }
                *object = static_cast<ExternalHeadingAbi *>(this);
                AddRef();
                return S_OK;
            }
            uint32_t __stdcall AddRef() noexcept override
            {
                return _references.fetch_add(1, std::memory_order_relaxed) + 1;
            }
            uint32_t __stdcall Release() noexcept override
            {
                auto remaining = _references.fetch_sub(1, std::memory_order_acq_rel) - 1;
                if (!remaining)
                    delete this;
                return remaining;
            }
            int32_t __stdcall GetIids(uint32_t *count, winrt::guid **ids) noexcept override
            {
                return Value()->GetIids(count, ids);
            }
            int32_t __stdcall GetRuntimeClassName(void **name) noexcept override
            {
                return Value()->GetRuntimeClassName(name);
            }
            int32_t __stdcall GetTrustLevel(winrt::Windows::Foundation::TrustLevel *level) noexcept override
            {
                return Value()->GetTrustLevel(level);
            }
            int32_t __stdcall get_DocumentId(winrt::guid *value) noexcept override
            {
                return Value()->get_DocumentId(value);
            }
            int32_t __stdcall get_Revision(uint64_t *value) noexcept override
            {
                return Value()->get_Revision(value);
            }
            int32_t __stdcall get_TextGeneration(uint64_t *value) noexcept override
            {
                return Value()->get_TextGeneration(value);
            }
            int32_t __stdcall get_HeadingId(uint32_t *value) noexcept override
            {
                return Value()->get_HeadingId(value);
            }
            int32_t __stdcall get_Level(uint32_t *value) noexcept override
            {
                return Value()->get_Level(value);
            }
            int32_t __stdcall get_Start(uint64_t *value) noexcept override
            {
                return Value()->get_Start(value);
            }
            int32_t __stdcall get_Length(uint64_t *value) noexcept override
            {
                return Value()->get_Length(value);
            }
            int32_t __stdcall get_Title(void **value) noexcept override
            {
                return Value()->get_Title(value);
            }
            int32_t __stdcall get_Anchor(void **value) noexcept override
            {
                return Value()->get_Anchor(value);
            }

          private:
            ModuleLifetime _module;
            std::atomic<uint32_t> _references{1};
            PageHeading _value;
            PageReentrancyState &_state;
            ExternalHeadingAbi *Value() const noexcept
            {
                return static_cast<ExternalHeadingAbi *>(winrt::get_abi(_value));
            }
        };
    }

    Results Run(std::shared_ptr<MainTextSnapshot const> const &main, notepads_markdown_core_cancel *sourceFlag)
    {
        Results results;
        Require(main && sourceFlag && main->Text() == FixtureMain, L"native outline uses the fixed actual Main fixture");
        auto model = main->ModelOwner().Get();
        auto error = Pod<notepads_markdown_core_error>();
        CancelOwner allocationFlag;
        Require(notepads_markdown_core_model_cancel_create(model, &allocationFlag.value, &error) == NOTEPADS_MARKDOWN_CORE_OK,
                L"outline root probes use an independently live same-root flag");
        auto headroom = Headroom(model, allocationFlag.value);
        auto memoryReferences = main->Memory().use_count();
        {
            auto outline = ReadingOutline::Build(*main, sourceFlag);
            CheckDirectory(outline, *main, results);
            CheckFragments(main->Memory(), results);
            Require(Headroom(model, allocationFlag.value) < headroom,
                    L"actual native outline payload remains charged while its snapshot is held");
        }
        Require(Headroom(model, allocationFlag.value) == headroom && main->Memory().use_count() == memoryReferences,
                L"last directory/fragment temporary release restores exact real root capacity and memory owners");
        results.capacityReleased = true;

        {
            Reservation occupied;
            Require(notepads_markdown_preview_model_reserve(model, allocationFlag.value, headroom, &occupied.value, &error) == NOTEPADS_MARKDOWN_CORE_OK,
                    L"native outline quota negative control holds all actual free root capacity");
            ExpectHresult(HRESULT_FROM_WIN32(ERROR_NOT_ENOUGH_QUOTA),
                          [&] { (void)ReadingOutline::Build(*main, sourceFlag); });
            ExpectHresult(HRESULT_FROM_WIN32(ERROR_NOT_ENOUGH_QUOTA),
                          [&] { (void)DecodeReadingFragment(L"#valid", main->Memory()); });
            occupied.Reset();
        }
        {
            auto recovered = ReadingOutline::Build(*main, sourceFlag);
            Require(recovered->Headings().size() == 4 && recovered->FindAnchor(L"a-1-1") == size_t{2},
                    L"native directory construction recovers after actual quota is restored");
        }
        Require(Headroom(model, allocationFlag.value) == headroom && main->Memory().use_count() == memoryReferences,
                L"failed and recovered construction leave no native rows, UTF8 blob or allocation owner");
        results.fundingFailure = true;

        {
            CancelOwner source;
            Require(notepads_markdown_core_model_cancel_create(model, &source.value, &error) == NOTEPADS_MARKDOWN_CORE_OK,
                    L"outline cancellation control owns a distinct actual source-work flag");
            auto retained = ReadingOutline::Build(*main, source.value);
            Require(notepads_markdown_core_cancel_request(source.value) == NOTEPADS_MARKDOWN_CORE_OK, L"outline source flag actually cancelled");
            ExpectHresult(HRESULT_FROM_WIN32(ERROR_CANCELLED),
                          [&] { (void)ReadingOutline::Build(*main, source.value); });
            Require(retained->Headings().size() == 4 && retained->FindAnchor(L"a-2") == size_t{3},
                    L"cancelling original source work does not revoke immutable directory reads");
            auto decoded = DecodeReadingFragment(L"#%61-2", retained->Memory());
            Require(decoded && std::wstring_view{*decoded} == L"a-2",
                    L"retained directory export uses its independent same-root allocation flag");
        }
        Require(Headroom(model, allocationFlag.value) == headroom && main->Memory().use_count() == memoryReferences,
                L"cancelled source and retained readonly directory retire every actual root charge");
        results.cancelledSourceKeepsReadOnly = true;
        results.passed = results.directoryChecks == 15 && results.fragmentChecks == 68 && results.fundingFailure &&
                         results.capacityReleased && results.cancelledSourceKeepsReadOnly;
        results.detail = L"Original Comrak suffix-collision rows/ranges/anchors checked=" +
                         std::to_wstring(results.directoryChecks) + L"; fixed strict UTF8/UTF16 fragment oracles=" +
                         std::to_wstring(results.fragmentChecks) +
                         L"; actual quota rejection/recovery, exact headroom refund and source-cancel readonly "
                         L"exports checked. Current-view identity, empty-block geometry and WinRT pages are host gates.";
        return results;
    }

    Results RunEmptyGeometry(std::shared_ptr<MainTextSnapshot const> const &main, notepads_markdown_core_cancel *sourceFlag)
    {
        Results results;
        auto check = [&](bool valid, wchar_t const *detail)
        {
            Require(valid, detail);
            ++results.geometryChecks;
        };
        check(main && sourceFlag && main->Text() == FixtureEmptyMain,
              L"empty-heading geometry uses the independent fixed actual Main");
        auto model = main->ModelOwner().Get();
        auto const &memory = main->Memory();
        auto error = Pod<notepads_markdown_core_error>();
        CancelOwner allocationFlag;
        Require(notepads_markdown_core_model_cancel_create(model, &allocationFlag.value, &error) == NOTEPADS_MARKDOWN_CORE_OK,
                L"empty-heading geometry root observation owns a distinct actual flag");
        auto headroom = Headroom(model, allocationFlag.value);
        auto memoryReferences = memory.use_count();
        {
            // Validate the caller's source-work flag through the real ABI before
            // rendering; do not publish a hand-built semantic/geometry table.
            auto sourceOutline = ReadingOutline::Build(*main, sourceFlag);
            check(sourceOutline->Headings().size() == 3,
                  L"the independent empty-heading source produces three actual headings");
            auto sourceRows = sourceOutline->Headings();
            check(sourceRows[0].text.start == 0 && sourceRows[0].text.end == 0 &&
                      sourceRows[1].text.start == 0 && sourceRows[1].text.end == 0 &&
                      sourceRows[2].text.start == 0 && sourceRows[2].text.end == 4,
                  L"zero-length heading coordinates and Tail range match the authored Core separator oracle");
            auto cancel = std::make_shared<PreviewCancellation>(model);
            auto reading = std::allocate_shared<ReadingSourceState>(
                ReadingAllocator<ReadingSourceState>{memory}, memory, cancel, main->SourceGeneration(),
                main->TextGeneration());
            reading->PublishMain(main);
            notepads_markdown_preview_key key{};
            key.header = {NOTEPADS_MARKDOWN_PREVIEW_ABI_VERSION, sizeof(key)};
            key.model = main->Identity();
            key.layout_generation = key.theme_generation = key.measurement_generation = 1;
            key.width = 800;
            key.indent = 24;
            key.gap = 8;
            PreviewTheme theme;
            std::atomic<uint64_t> latest{1};
            auto renderer = NativeDocumentRenderer::Create({}, cancel, key, theme, reading);
            auto first = renderer->CreateFrame(0, 1000, 1, latest);
            check(first && first->main == main && first->descriptors && first->descriptors->outline &&
                      first->navigation && first->totalBlocks == 3 && first->Blocks().size() == 3,
                  L"real Rust/DWrite renderer returns all three actual heading blocks and its shared semantics");
            auto outline = first->descriptors->outline;
            auto inspect = [&](PreviewFrame const &frame, notepads_markdown_preview_key const &expectedKey)
            {
                check(frame.main == main && frame.descriptors->outline == outline &&
                          outline->TextGeneration() == main->TextGeneration() &&
                          frame.navigation->Key().layout_generation == expectedKey.layout_generation &&
                          frame.navigation->Key().theme_generation == expectedKey.theme_generation &&
                          frame.navigation->Key().measurement_generation == expectedKey.measurement_generation &&
                          frame.navigation->Key().width == expectedKey.width,
                      L"actual Frame uses this source token and its own current geometry key");
                auto rows = outline->Headings();
                check(rows.size() == sourceRows.size(), L"renderer catalog and independent same-model ABI agree on count");
                constexpr std::array<std::wstring_view, 3> titles{L"", L"", L"Tail"};
                constexpr std::array<std::wstring_view, 3> anchors{L"", L"-1", L"tail"};
                std::array<double, 3> positions{};
                for (size_t index = 0; index < rows.size(); ++index)
                {
                    auto const &row = rows[index];
                    check(row.node == sourceRows[index].node && row.level == 1 &&
                              std::wstring_view{row.title} == titles[index] &&
                              std::wstring_view{row.anchor} == anchors[index],
                          L"renderer derives the original empty headings and Tail without replacing nodes");
                    notepads_markdown_preview_block const *actual{};
                    for (auto const &visual : frame.Blocks())
                    {
                        if (visual.block.node != row.node)
                            continue;
                        check(!actual, L"each heading maps to one actual Rust layout block");
                        actual = &visual.block;
                    }
                    check(actual && actual->kind == 2 && actual->logical_start == row.text.start &&
                              actual->logical_end == row.text.end && std::isfinite(actual->y) &&
                              std::isfinite(actual->height) && actual->y >= 0 && actual->height > 0,
                          L"empty headings remain real independently measured layout blocks");
                    positions[index] = actual->y;
                    check(frame.navigation->RevealBlockY(row.node) == actual->y,
                          L"node navigation equals the actual Rust block.y rather than an ambiguous text position");
                }
                check(rows[0].text.start == rows[1].text.start && rows[0].text.end == rows[1].text.end &&
                          rows[0].node != rows[1].node && positions[0] == 0 && positions[0] < positions[1] &&
                          positions[1] < positions[2],
                      L"two equal empty Main ranges address different actual vertical block positions");
                return positions;
            };
            auto firstPositions = inspect(*first, key);
            results.emptyBlocksDistinct = true;
            try
            {
                (void)first->navigation->RevealBlockY(0);
                throw winrt::hresult_error(E_FAIL, L"missing node unexpectedly returned current geometry");
            }
            catch (ReadingFailure const &failure)
            {
                check(failure.status == NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE,
                      L"missing current block refuses navigation instead of falling back to text offset zero");
                results.missingBlockRejected = true;
            }

            auto updatedKey = key;
            updatedKey.layout_generation = updatedKey.theme_generation = updatedKey.measurement_generation = 2;
            updatedKey.width = 275;
            updatedKey.gap = 31;
            auto updatedTheme = theme;
            updatedTheme.fontSize = 30;
            updatedTheme.lineHeight = 45;
            auto updatedCancel = std::make_shared<PreviewCancellation>(model);
            auto updatedRenderer = NativeDocumentRenderer::Create({}, updatedCancel, updatedKey, updatedTheme, reading);
            latest.store(2, std::memory_order_release);
            auto second = updatedRenderer->CreateFrame(0, 1000, 2, latest);
            check(second && second->navigation && second->descriptors == first->descriptors &&
                      second->descriptors->outline == outline && second->main == first->main &&
                      second->navigation != first->navigation && second->key.width == updatedKey.width,
                  L"resize/font remeasurement reuses the exact immutable outline/Main but creates current geometry");
            auto secondPositions = inspect(*second, updatedKey);
            check(secondPositions[1] > firstPositions[1] && secondPositions[2] > firstPositions[2],
                  L"changed real font/line heights and gaps visibly move both later heading block positions");
            results.currentLayoutGeometry = results.sameOutlineAcrossLayouts = true;
            auto retainedPositions = inspect(*first, key);
            check(retainedPositions == firstPositions,
                  L"new geometry does not mutate the retained prior Frame or its node-navigation positions");
            results.retainedFrameUnchanged = true;
        }
        check(Headroom(model, allocationFlag.value) == headroom && memory.use_count() == memoryReferences,
              L"both actual renderers/Frames, shared catalog and source state retire their real root charges");
        results.capacityReleased = true;
        results.passed = results.emptyBlocksDistinct && results.currentLayoutGeometry && results.sameOutlineAcrossLayouts &&
                         results.missingBlockRejected && results.retainedFrameUnchanged && results.capacityReleased;
        results.detail = L"Actual empty-heading Rust/DWrite Frames checked=" + std::to_wstring(results.geometryChecks) +
                         L"; same empty Main ranges have distinct block.y; width/font/gap remeasurement reuses the "
                         L"exact outline/token, targets current block.y and preserves retained old Frame geometry. "
                         L"No fabricated ReadingBlock table; real model-root charges refunded.";
        return results;
    }

    bool RunPageReentrancy(winrt::Notepads::Markdown::MarkdownOutline const &snapshot,
                           std::shared_ptr<ReadingMemory> const &memory)
    {
        Require(snapshot && snapshot.Count() > 0 && memory && snapshot.Revision() == memory->Identity().revision,
                L"page reentrancy uses a nonempty real same-model outline snapshot");
        {
            auto page = snapshot.GetHeadings(0, 1);
            auto first = page.GetAt(0);
            uint32_t index = UINT32_MAX;
            Require(page.IndexOf(first, index) && index == 0, L"same Heading COM identity is found");
            index = UINT32_MAX;
            Require(!page.IndexOf(PageHeading{nullptr}, index) && index == 0, L"null Heading identity is not found");
            auto anotherPage = snapshot.GetHeadings(0, 1);
            auto anotherWrapper = anotherPage.GetAt(0);
            index = UINT32_MAX;
            Require(!page.IndexOf(anotherWrapper, index) && index == 0,
                    L"equal heading data does not substitute for distinct COM wrapper identity");
            auto empty = snapshot.GetHeadings(snapshot.Count(), 1);
            index = UINT32_MAX;
            Require(empty.Size() == 0 && !empty.IndexOf(first, index) && index == 0,
                    L"empty page preserves basic IndexOf semantics");
        }
        auto model = memory->ModelOwner().Get();
        auto error = Pod<notepads_markdown_core_error>();
        CancelOwner allocationFlag;
        Require(notepads_markdown_core_model_cancel_create(model, &allocationFlag.value, &error) == NOTEPADS_MARKDOWN_CORE_OK,
                L"page reentrancy observes the real root through a distinct allocation flag");
        auto baseline = Headroom(model, allocationFlag.value);
        PageReentrancyState state;
        {
            auto page = snapshot.GetHeadings(0, 1);
            auto wrapped = page.GetAt(0);
            auto occupied = Headroom(model, allocationFlag.value);
            Require(occupied < baseline, L"actual custom page has a paid backing vector");
            state.externalPage = &page;
            state.model = model;
            state.allocationFlag = allocationFlag.value;
            state.occupiedHeadroom = occupied;
            PageHeading foreign{static_cast<ExternalHeadingAbi *>(new ReentrantHeading(wrapped, state)),
                                winrt::take_ownership_from_abi};
            Require(foreign.HeadingId() == wrapped.HeadingId() && foreign.Title() == wrapped.Title(),
                    L"external Heading ABI is live and supplies legitimate immutable heading values");
            state.armed = true;
            // Borrow the real ABI pointer while its caller reference is still
            // live. Do not copy page, call as<> or acquire an iterator: any of
            // those would conceal the missing self pin from the regression.
            auto abi = static_cast<winrt::impl::abi_t<PageView> *>(winrt::get_abi(page));
            uint32_t index = UINT32_MAX;
            bool found = true;
            auto status = abi->IndexOf(winrt::get_abi(foreign), &index, &found);
            if (state.failure)
                std::rethrow_exception(state.failure);
            winrt::check_hresult(status);
            Require(!page && state.entered && state.externalReferenceCleared && state.remainedPaidInCallback &&
                        state.callbackFinished && !found && index == 0,
                    L"real IndexOf survives foreign IUnknown QI clearing its last external page reference");
            Require(Headroom(model, allocationFlag.value) == baseline,
                    L"paid page retires after the callback and IndexOf finish, without a hidden long-lived page pin");
            Require(foreign.HeadingId() == wrapped.HeadingId() && !state.foreignDestroyed,
                    L"external Heading ownership remains valid through the entire ABI invocation");
            foreign = nullptr;
            Require(state.foreignDestroyed, L"external Heading has balanced actual AddRef/Release ownership");
        }
        Require(Headroom(model, allocationFlag.value) == baseline,
                L"reentrant identity lookup and its temporary values refund every actual paid page charge");
        auto recovered = snapshot.GetHeadings(0, 1);
        Require(recovered.Size() == 1 && recovered.GetAt(0).HeadingId() == 1 &&
                    recovered.GetAt(0).TextGeneration() == snapshot.TextGeneration(),
                L"original immutable outline still exports a correct new page after the reentrant call");
        return true;
    }
}
