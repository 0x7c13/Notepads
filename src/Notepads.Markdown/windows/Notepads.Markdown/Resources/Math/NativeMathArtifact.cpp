// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Resources/Math/NativeMathArtifact.h"
#include <array>
#include <new>

namespace Notepads::Markdown::Native
{
    namespace
    {
        void Geometry(bool value)
        {
            if (!value)
                throw MathMemoryFailure{NOTEPADS_MARKDOWN_MATH_INVALID_GEOMETRY};
        }

        void Read(notepads_markdown_math_job *job, uint32_t table, uint64_t start, uint64_t count, uint64_t total, uint32_t rowSize,
                  void *buffer)
        {
            notepads_markdown_math_read_result result{};
            result.header = {NOTEPADS_MARKDOWN_MATH_ABI_VERSION, sizeof(result)};
            notepads_markdown_core_error error{NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(notepads_markdown_core_error)};
            auto status = notepads_markdown_math_job_read(job, table, start, count, rowSize, buffer, &result, &error);
            if (status != NOTEPADS_MARKDOWN_CORE_OK)
                throw MathMemoryFailure{status, error};
            Geometry(result.written == count && result.next == start + count && result.reserved == 0 &&
                     result.done == static_cast<uint32_t>(result.next == total));
        }

        MathPathCommand Command(notepads_markdown_math_command const &value, double baseline)
        {
            Geometry(value.reserved == 0);
            MathPathVerb verb;
            switch (value.kind)
            {
            case NOTEPADS_MARKDOWN_MATH_MOVE:
                verb = MathPathVerb::Move;
                break;
            case NOTEPADS_MARKDOWN_MATH_LINE:
                verb = MathPathVerb::Line;
                break;
            case NOTEPADS_MARKDOWN_MATH_QUAD:
                verb = MathPathVerb::Quad;
                break;
            case NOTEPADS_MARKDOWN_MATH_CUBIC:
                verb = MathPathVerb::Cubic;
                break;
            case NOTEPADS_MARKDOWN_MATH_CLOSE:
                verb = MathPathVerb::Close;
                break;
            default:
                throw MathMemoryFailure{NOTEPADS_MARKDOWN_MATH_INVALID_GEOMETRY};
            }
            MathPathCommand result{verb, value.x1, value.y1, value.x2, value.y2, value.x3, value.y3};
            // ABI2 carries the producer's global top-left display coordinates;
            // our native DTO is baseline-relative. Translate once at this
            // boundary, including curve controls, preserving unused wire zeros.
            if (verb != MathPathVerb::Close)
                result.y1 -= baseline;
            if (verb == MathPathVerb::Quad || verb == MathPathVerb::Cubic)
                result.y2 -= baseline;
            if (verb == MathPathVerb::Cubic)
                result.y3 -= baseline;
            return result;
        }

        MathPrimitive Primitive(notepads_markdown_math_primitive const &value)
        {
            Geometry(value.reserved == 0 && (value.flags & ~(NOTEPADS_MARKDOWN_MATH_FILL | NOTEPADS_MARKDOWN_MATH_THEME_FOREGROUND)) == 0);
            // Paint role is carried explicitly by the private ABI. Color values
            // never establish provenance: explicit black can remain explicit.
            return {value.command_start,
                    value.command_count,
                    (value.flags & NOTEPADS_MARKDOWN_MATH_FILL) != 0,
                    value.stroke_width,
                    (value.flags & NOTEPADS_MARKDOWN_MATH_THEME_FOREGROUND) != 0 ? MathPaintRole::Foreground : MathPaintRole::Rgba,
                    {value.r, value.g, value.b, value.a}};
        }
    }

    NativeMathArtifactResult NativeMathArtifact::BuildAfterThreadExit(notepads_markdown_math_job *job, ID2D1Factory *factory) noexcept
    {
        NativeMathArtifactResult result;
        if (!job || !factory)
        {
            result.status = NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT;
            result.error.status = result.status;
            return result;
        }
        try
        {
            // Create first reads a genuine successful Computed artifact summary.
            // Failed/Unknown jobs return their typed status without native work.
            auto memory = NativeMathMemory::Create(job);
            result.summary = memory->Summary();
            Geometry(result.summary.state == NOTEPADS_MARKDOWN_MATH_COMPUTED);
            auto baseline = result.summary.metrics.ascent;
            Geometry(std::isfinite(baseline) && baseline >= 0);
            if (result.summary.commands > NativeMathDrawing::MaximumCommands ||
                result.summary.primitives > NativeMathDrawing::MaximumPrimitives)
                throw MathMemoryFailure{NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT};
            std::shared_ptr<NativeMathDrawing> drawing;
            {
                result.stage = NativeMathBuildStage::Read;
                MathVector<MathPathCommand> commands{MathAllocator<MathPathCommand>{memory, MathMemoryFamily::Path}};
                MathVector<MathPrimitive> primitives{MathAllocator<MathPrimitive>{memory, MathMemoryFamily::Mirror}};
                commands.reserve(static_cast<size_t>(result.summary.commands));
                primitives.reserve(static_cast<size_t>(result.summary.primitives));
                // Fixed wire buffers have no heap allocation or callback. Each
                // actual native vector capacity and its replacement is charged
                // before allocation while Rust's original arrays remain live.
                std::array<notepads_markdown_math_command, NOTEPADS_MARKDOWN_MATH_MAX_BATCH> commandBatch{};
                for (uint64_t start = 0; start < result.summary.commands;)
                {
                    auto count = (std::min)(uint64_t{NOTEPADS_MARKDOWN_MATH_MAX_BATCH}, result.summary.commands - start);
                    Read(job, NOTEPADS_MARKDOWN_MATH_COMMANDS, start, count, result.summary.commands, sizeof(notepads_markdown_math_command),
                         commandBatch.data());
                    for (size_t index = 0; index < count; ++index)
                        commands.push_back(Command(commandBatch[index], baseline));
                    start += count;
                }
                std::array<notepads_markdown_math_primitive, NOTEPADS_MARKDOWN_MATH_MAX_BATCH> primitiveBatch{};
                for (uint64_t start = 0; start < result.summary.primitives;)
                {
                    auto count = (std::min)(uint64_t{NOTEPADS_MARKDOWN_MATH_MAX_BATCH}, result.summary.primitives - start);
                    Read(job, NOTEPADS_MARKDOWN_MATH_PRIMITIVES, start, count, result.summary.primitives, sizeof(notepads_markdown_math_primitive),
                         primitiveBatch.data());
                    for (size_t index = 0; index < count; ++index)
                        primitives.push_back(Primitive(primitiveBatch[index]));
                    start += count;
                }
                result.stage = NativeMathBuildStage::Geometry;
                auto const &metrics = result.summary.metrics;
                drawing = NativeMathDrawing::Create(
                    memory, factory,
                    {metrics.advance,
                     metrics.ascent,
                     metrics.descent,
                     {metrics.left, metrics.top - baseline, metrics.right, metrics.bottom - baseline},
                     primitives,
                     commands});
            }
            // Converted temporary vectors are physically freed before finalize.
            // Rust finalize drops its own artifact and applies this same job's
            // cumulative ledger once; no source reservation or family resets.
            result.stage = NativeMathBuildStage::Finalize;
            result.status = memory->Finalize(&result.error);
            if (result.status != NOTEPADS_MARKDOWN_CORE_OK)
                return result;
            result.memory = std::move(memory);
            result.drawing = std::move(drawing);
            result.stage = NativeMathBuildStage::Complete;
            return result;
        }
        catch (MathMemoryFailure const &failure)
        {
            result.status = failure.status;
            result.error = failure.diagnostic;
        }
        catch (ReadingFailure const &failure)
        {
            result.status = failure.status;
            result.error.status = result.status;
        }
        catch (winrt::hresult_error const &failure)
        {
            result.status = NOTEPADS_MARKDOWN_MATH_INVALID_GEOMETRY;
            result.error.status = result.status;
            result.graphicsError = failure.code();
        }
        catch (std::bad_alloc const &)
        {
            result.status = NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY;
            result.error.status = result.status;
        }
        catch (...)
        {
            result.status = NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT;
            result.error.status = result.status;
        }
        return result;
    }
}
