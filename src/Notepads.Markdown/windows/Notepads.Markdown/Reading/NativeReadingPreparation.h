// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Rendering/NativeDocumentRenderer.h"
#include <utility>

namespace Notepads::Markdown::Native
{
    // Keep the same source fault boundary around production measurement and
    // controlled native callback recovery diagnostics, without a runtime hook.
    template <typename Work>
    void PrepareReadingSource(std::shared_ptr<ReadingSourceState> const &reading, Work &&work)
    {
        try
        {
            std::forward<Work>(work)();
        }
        catch (ReadingFailure const &failure)
        {
            if (failure.status != NOTEPADS_MARKDOWN_CORE_CANCELLED && failure.status != NOTEPADS_MARKDOWN_CORE_STALE_RESULT && !reading->fault)
                reading->fault = failure.status;
            throw;
        }
        catch (PreviewFailure const &failure)
        {
            if (failure.status != NOTEPADS_MARKDOWN_CORE_CANCELLED && failure.status != NOTEPADS_MARKDOWN_CORE_STALE_RESULT && !reading->fault)
                reading->fault = failure.status;
            throw;
        }
        catch (std::bad_alloc const &)
        {
            if (!reading->fault)
                reading->fault = NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY;
            throw;
        }
        catch (winrt::hresult_error const &error)
        {
            // A COM font callback can translate PreviewFailure into HRESULT.
            // Restore explicit cancellation before recording a source fault:
            // retiring one layout must not poison its still-current Main.
            if (error.code() == NativeStatusHresult(NOTEPADS_MARKDOWN_CORE_CANCELLED))
                throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_CANCELLED, L"reading preparation"};
            if (!reading->fault)
                reading->fault = error.code().value;
            throw;
        }
        catch (...)
        {
            if (!reading->fault)
                reading->fault = NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT;
            throw;
        }
    }
}