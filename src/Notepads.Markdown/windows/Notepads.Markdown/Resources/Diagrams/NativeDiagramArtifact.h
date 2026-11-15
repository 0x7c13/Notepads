// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Resources/Diagrams/NativeDiagramPresentation.h"
#include <objidl.h>
#include <dwrite.h>

namespace Notepads::Markdown::Native
{
    // Raw native capacity is transferred to the prepaid Node on every return,
    // including read/finalize failures. There is no destructor that could hide
    // a failed free or discard its corresponding FFI funding obligation.
    struct NativeDiagramArtifactResult
    {
        int32_t status{NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
        notepads_markdown_core_error error{NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(notepads_markdown_core_error)};
        notepads_markdown_diagram_summary summary{notepads_markdown_core_header{NOTEPADS_MARKDOWN_DIAGRAM_ABI_VERSION, sizeof(notepads_markdown_diagram_summary)}};
        uint8_t *bytes{};
        uint64_t capacity{};
        bool finalized{};
    };

    class NativeDiagramArtifact final
    {
      public:
        static NativeDiagramArtifactResult BuildAfterThreadExit(notepads_markdown_diagram_job *job) noexcept;
        // Independent read cursors occupy fixed prepaid Node slots. The stream
        // retains the Node through its last real COM Release; no byte copy or
        // heap delegate is made after finalization.
        static HRESULT OpenReadStream(DiagramPresentationHandle const &payload, IStream **output) noexcept;
        static HRESULT CreateInlineObject(DiagramPresentationHandle const &payload, double width, double height,
                                          double baseline, bool terminal, IDWriteInlineObject **output) noexcept;
        static constexpr size_t StreamBytes = 128;
        static constexpr uint32_t StreamSlots = 16;
    };
}
