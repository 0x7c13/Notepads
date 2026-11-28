// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "NativeMathArtifactTests.h"
#include <array>

namespace Notepads::Markdown::Native::MathArtifactTests
{
    namespace
    {
        void Require(bool value, wchar_t const *detail)
        {
            if (!value)
                throw winrt::hresult_error(E_FAIL, detail);
        }

        bool Near(double actual, double expected) noexcept
        {
            return std::isfinite(actual) && std::abs(actual - expected) < 0.002;
        }

        notepads_markdown_core_error Error() noexcept
        {
            return {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(notepads_markdown_core_error)};
        }

        notepads_markdown_math_usage Usage(notepads_markdown_math_control *control)
        {
            notepads_markdown_math_usage usage{};
            usage.header = {NOTEPADS_MARKDOWN_MATH_ABI_VERSION, sizeof(usage)};
            auto error = Error();
            Require(notepads_markdown_math_control_usage(control, &usage, &error) == NOTEPADS_MARKDOWN_CORE_OK, L"real computed math control usage");
            return usage;
        }

        void LineLimit(std::shared_ptr<NativeMathDrawing> const &drawing, MathMemoryHandle const &memory,
                       MathPlacement const &placement)
        {
            winrt::com_ptr<IDWriteFactory> factory;
            winrt::check_hresult(DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED, __uuidof(IDWriteFactory),
                                                     reinterpret_cast<IUnknown **>(factory.put())));
            winrt::com_ptr<IDWriteTextFormat> format;
            winrt::check_hresult(factory->CreateTextFormat(L"Segoe UI", nullptr, DWRITE_FONT_WEIGHT_NORMAL,
                                                           DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, 16,
                                                           L"en-US", format.put()));
            {
                auto object = NativeMathDrawing::CreateInlineObject(memory, placement, L"\ufffc", 0);
                winrt::com_ptr<IDWriteTextLayout> layout;
                winrt::check_hresult(factory->CreateTextLayout(L"\ufffc", 1, format.get(), 640, 200, layout.put()));
                winrt::check_hresult(layout->SetInlineObject(object.get(), {0, 1}));
                auto anchor = drawing->TextAnchor(layout.get(), 0);
                Require(std::isfinite(anchor.left) && std::isfinite(anchor.baseline) && anchor.baseline > 0,
                        L"finalized funded TextAnchor uses real DWrite line baseline without heap growth");
            }
            // 257 actual lines: the first contains the formula, then 256 x lines.
            // No trailing newline or wrapping ambiguity enters this boundary.
            std::array<wchar_t, 513> text{};
            text[0] = L'\ufffc';
            for (size_t index = 0; index < 256; ++index)
            {
                text[1 + index * 2] = L'\n';
                text[2 + index * 2] = L'x';
            }
            auto object = NativeMathDrawing::CreateInlineObject(memory, placement, {text.data(), text.size()}, 0);
            winrt::com_ptr<IDWriteTextLayout> layout;
            winrt::check_hresult(factory->CreateTextLayout(text.data(), static_cast<uint32_t>(text.size()),
                                                           format.get(), 640, 100000, layout.put()));
            winrt::check_hresult(layout->SetInlineObject(object.get(), {0, 1}));
            bool limited{};
            try
            {
                (void)drawing->TextAnchor(layout.get(), 0);
            }
            catch (ReadingFailure const &failure)
            {
                limited = failure.status == NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT;
            }
            Require(limited, L"257-line funded layout is typed ResourceLimit instead of a guessed anchor");
        }
    }

    Results RunComputed(notepads_markdown_math_job *job, notepads_markdown_math_control *control)
    {
        Results result{};
        Require(job && control && notepads_markdown_math_job_state(job) == NOTEPADS_MARKDOWN_MATH_COMPUTED,
                L"computed helper borrows actual post-termination job and control");
        auto before = Usage(control);
        Require(before.control.jobs == 1 && before.control.bytes >= 64ull * 1024 * 1024,
                L"native build starts with a real full 64MiB job and actual-worker slot");
        winrt::com_ptr<ID2D1Factory> factory;
        winrt::check_hresult(D2D1CreateFactory(D2D1_FACTORY_TYPE_MULTI_THREADED, factory.put()));
        auto built = NativeMathArtifact::BuildAfterThreadExit(job, factory.get());
        if (built.status != NOTEPADS_MARKDOWN_CORE_OK)
            throw MathMemoryFailure{built.status, built.error};
        Require(built.stage == NativeMathBuildStage::Complete && built.memory && built.memory->IsFinalized() &&
                    built.drawing && notepads_markdown_math_job_state(job) == NOTEPADS_MARKDOWN_MATH_FINALIZED,
                L"real batch conversion, D2D geometry and one retained transition complete");
        auto memory = built.memory;
        auto view = built.drawing->Artifact();
        Require(!view.commands.empty() && view.primitives.size() == 2 &&
                    view.commands.size() == built.summary.commands &&
                    view.primitives.size() == built.summary.primitives,
                L"both ordinary letters preserve every wire command and produce their two actual native paths");
        result.commands = view.commands.size();
        result.primitives = view.primitives.size();
        result.geometryBuilt = true;

        // Independent pinned font metrics, fixed before executing this case:
        // Math-Italic a width=0.52859, b width=0.42917; b height=0.69444,
        // both depth=0. Math whitespace adds no inter-letter advance. These
        // constants come from the fixed KaTeX metric table, not this run's DTO.
        // vendor/ratex-bounded/ratex-font/src/data/metrics_data.rs MATH_ITALIC.
        Require(Near(view.advanceEm, 0.95776) && Near(view.ascentEm, 0.69444) && Near(view.descentEm, 0),
                L"a b metrics match the independently fixed Math-Italic numeric oracle");
        result.advance = view.advanceEm;
        result.ascent = view.ascentEm;
        result.descent = view.descentEm;
        result.numericOracle = true;
        // Independent original Math-Italic glyf bounds: a/b yMax=694,
        // yMin=-11 at 1000 units/em. Native paths are baseline-relative y-down.
        // Logical dimensions alone cannot detect a top-left/baseline mismatch:
        // the old adapter inflated its occupied box and still passed containment.
        Require(Near(view.inkEm.top, -0.694) && Near(view.inkEm.bottom, 0.011),
                L"actual a b ink must match the independent baseline-relative TTF bounds, not top-left wire y");
        auto occupied = built.drawing->OccupiedEm();
        Require(occupied.left <= 0 && occupied.top <= -view.ascentEm && occupied.right >= view.advanceEm &&
                    occupied.bottom >= view.descentEm && occupied.right > occupied.left &&
                    occupied.bottom > occupied.top,
                L"funded D2D path bounds contain the independent logical box");

        result.actualRetained = memory->FinalActualBytes();
        result.liveWithDrawing = memory->LiveBytes();
        Require(result.actualRetained == built.summary.prepaid_metadata_bytes + result.liveWithDrawing &&
                    result.actualRetained <= 32ull * 1024 * 1024,
                L"retained actual includes real live native capacities and prepaid job context exactly once");
        auto retained = Usage(control);
        Require(retained.control.jobs == 0 &&
                    retained.control.retained_bytes == before.control.retained_bytes + result.actualRetained &&
                    retained.control.retained_bytes <= 32ull * 1024 * 1024 &&
                    retained.control.bytes + 64ull * 1024 * 1024 == before.control.bytes + result.actualRetained,
                L"full job converts once to the same control's actual bounded retained grant");
        result.retainedBounded = true;

        for (auto family : {MathMemoryFamily::Path, MathMemoryFamily::Mirror, MathMemoryFamily::Metadata})
        {
            auto error = Error();
            void *allocation{};
            auto status = memory->TryAllocate(32, alignof(std::max_align_t), family, &allocation, &error);
            Require(status == NOTEPADS_MARKDOWN_CORE_INVALID_STATE && !allocation && memory->LiveBytes() == result.liveWithDrawing,
                    L"finalized owner rejects ordinary heap growth in every native family");
        }
        result.heapGrowthRejected = true;

        auto placement = built.drawing->Place(20, 640, MathPlacementMode::Inline);
        Require(placement.status == MathPlacementStatus::Ready, L"funded numeric formula fits its native inline box");
        {
            std::array<winrt::com_ptr<IDWriteInlineObject>, NativeMathMemory::InlineSlots> objects;
            for (auto &object : objects)
                object = NativeMathDrawing::CreateInlineObject(memory, placement, L"\ufffc", 0);
            Require(memory->InlineInUse() == NativeMathMemory::InlineSlots &&
                        memory->LiveBytes() == result.liveWithDrawing,
                    L"all sixteen late wrappers use only the already charged fixed arena");
            bool exhausted{};
            try
            {
                auto overflow = NativeMathDrawing::CreateInlineObject(memory, placement, L"\ufffc", 0);
                (void)overflow;
            }
            catch (MathMemoryFailure const &failure)
            {
                exhausted = failure.status == NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT;
            }
            Require(exhausted && memory->InlineInUse() == NativeMathMemory::InlineSlots,
                    L"seventeenth live inline wrapper is typed ResourceLimit");
            result.arenaExhausted = true;
            auto lastReference = objects[0];
            objects[0] = nullptr;
            Require(memory->InlineInUse() == NativeMathMemory::InlineSlots,
                    L"a copied COM reference keeps its real arena slot alive");
            lastReference = nullptr;
            Require(memory->InlineInUse() == NativeMathMemory::InlineSlots - 1,
                    L"only the final COM release frees the slot");
        }
        Require(memory->InlineInUse() == 0, L"all late wrapper slots actually released");
        for (size_t cycle = 0; cycle < 64; ++cycle)
        {
            auto object = NativeMathDrawing::CreateInlineObject(memory, placement, L"\ufffc", 0);
            Require(memory->InlineInUse() == 1, L"released arena capacity is reusable without heap growth");
        }
        Require(memory->InlineInUse() == 0 && memory->LiveBytes() == result.liveWithDrawing,
                L"arena reuse preserves real known requested capacity");
        result.arenaReused = true;
        LineLimit(built.drawing, memory, placement);
        Require(memory->InlineInUse() == 0 && memory->LiveBytes() == result.liveWithDrawing,
                L"real DWrite anchor checks left no late allocations alive");
        result.lineLimitTyped = true;

        std::weak_ptr<NativeMathDrawing> weak = built.drawing;
        built.drawing.reset();
        Require(weak.expired(), L"drawing object dies when its last strong reference is released");
        result.liveWithWeak = memory->LiveBytes();
        Require(result.liveWithWeak < result.liveWithDrawing && result.liveWithWeak > sizeof(NativeMathMemory),
                L"weak shared control block remains physically allocated after geometry arrays die");
        weak.reset();
        result.liveAfterLastWeak = memory->LiveBytes();
        Require(result.liveAfterLastWeak == sizeof(NativeMathMemory) &&
                    result.liveAfterLastWeak < result.liveWithWeak && memory->FreeFailures() == 0,
                L"last weak owner truly frees its funded combined control block before owner lease release");
        result.lastWeakFreed = true;
        built.memory.Reset();
        memory.Reset();
        // The reaper and fixture still own borrowed job references. Its retained
        // grant remains until both return/release; shrinking it here would be an
        // ownership bug. The caller verifies final baseline after actual reap.
        auto held = Usage(control);
        Require(held.control.jobs == 0 && held.control.retained_bytes == retained.control.retained_bytes &&
                    held.control.bytes == retained.control.bytes,
                L"final native owner drop leaves the retained lease with the real borrowed job until reap");
        result.nativeOwnersDropped = true;
        return result;
    }
}
