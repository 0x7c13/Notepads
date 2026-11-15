// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Resources/Images/NativeImageResources.h"
#include "Resources/Svg/NativeSvgFontSession.h"

namespace Notepads::Markdown::Native
{
    struct ImageLoadTask;
    struct NativeSvgJobState;
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
    enum class NativeSvgJobTestFault : uint8_t
    {
        None,
        PayloadFree,
        MetadataFree,
        PartialMetadata
    };
#endif

    // Fixed evidence survives consumed handles and actual node free. The task
    // owns this POD directly; failures never allocate a diagnostic record.
    struct NativeSvgJobReceipt
    {
        notepads_markdown_svg_job_retirement engine{};
        NativeSvgFontRetirement native;
        SvgCoverageStatistics coverage{};
        notepads_markdown_svg_job_destroyed destroyed{};
        notepads_markdown_svg_resources_retirement resources{};
        ReadingFreeReceipt workerFree;
        ReadingFreeReceipt discardFree;
        uint64_t stackReserved{}, stackCommitted{}, stackGuard{}, stackHeadroom{};
        uint64_t nodeBytes{};
        uint32_t workerThread{}, reaperThread{};
        uint32_t destroyAttempts{}, activeCancelCalls{};
        int32_t cancellationStatus{};
        HRESULT cancellationError{S_OK};
        bool threadSignaled{}, cancelEntryRevoked{}, cancelLeasesZero{}, nodeFreed{};
    };

    // Called only after DecodeImageAsync has entered its existing background
    // MTA. This synchronous entry has no suspension: its call returns only after
    // the unique reaper has finished or permanently quarantined the task. A
    // canceled parent's await_transform cannot abandon a live child operation.
    // One existing physical image task owns the worker and its retirement.
    // Completion is withheld until the real worker has exited and the unique
    // reaper has destroyed all job owners. Failure keeps the original task pinned.
    // Reuse callers first store their cloned source in task.svgEncoded under
    // its mutex, then pass an empty encoded argument. The entry keeps that
    // canonical owner through all validation and early initialization failures.
    void RenderSvgImageOnBackground(std::shared_ptr<ImageLoadTask> task, std::shared_ptr<ReadingMemory> memory,
                                    NativeEncodedImageHandle encoded, NativeSvgImageTarget target,
                                    std::shared_ptr<NativeImageDecodeResult> result);

    // Pre-entry coroutine/scheduling failure only. No allocation, await, COM
    // apartment requirement or worker join. Caller holds no task mutex. It
    // retires a canonical unstarted input; only true authorizes caller Finish.
    bool RetireUnstartedSvgInput(std::shared_ptr<ImageLoadTask> const &task) noexcept;

    // Acquires task.mutex internally; never waits for a worker or reaper.
    void CancelSvgImageTask(ImageLoadTask &task) noexcept;
    // Caller holds task.mutex. Canonical unstarted input/cleanup also blocks.
    bool IsSvgImageTaskRetiredLocked(ImageLoadTask const &task) noexcept;
}
