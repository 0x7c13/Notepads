// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Resources/Math/NativeMathGeometry.h"
#include "Resources/Math/NativeMathMemory.h"

namespace Notepads::Markdown::Native
{
    enum class NativeMathBuildStage : uint32_t
    {
        Acquire,
        Read,
        Geometry,
        Finalize,
        Complete
    };

    struct NativeMathArtifactResult
    {
        int32_t status{NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
        notepads_markdown_core_error error{NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(notepads_markdown_core_error)};
        HRESULT graphicsError{S_OK};
        NativeMathBuildStage stage{NativeMathBuildStage::Acquire};
        notepads_markdown_math_summary summary{notepads_markdown_core_header{NOTEPADS_MARKDOWN_MATH_ABI_VERSION, sizeof(notepads_markdown_math_summary)}};
        MathMemoryHandle memory;
        std::shared_ptr<NativeMathDrawing> drawing;
    };

    // Private post-reaper builder. The caller has observed the actual worker
    // thread handle signaled and exclusively owns post-join FFI access. Supply
    // a factory usable on this calling thread and by the eventual render target.
    // No application callback or user-controlled execution/profile is accepted.
    // A complete result is funded CPU geometry; publication still needs the
    // coordinator's current source/layout/theme/device and profile permission.
    // Unknown builtin profile produces its original typed failure before any
    // allocation authority or geometry allocation is created.
    class NativeMathArtifact final
    {
      public:
        static NativeMathArtifactResult BuildAfterThreadExit(notepads_markdown_math_job *job, ID2D1Factory *factory) noexcept;
    };
}
