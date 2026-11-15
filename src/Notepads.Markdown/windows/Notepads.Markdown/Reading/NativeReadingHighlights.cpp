// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Rendering/NativeDocumentRenderer.h"
#include <new>

namespace Notepads::Markdown::Native
{
    namespace
    {
        int32_t HighlightStatus(winrt::hresult value) noexcept
        {
            switch (value.value)
            {
            case E_OUTOFMEMORY:
                return NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY;
            case E_ABORT:
            case HRESULT_FROM_WIN32(ERROR_CANCELLED):
                return NOTEPADS_MARKDOWN_CORE_CANCELLED;
            case HRESULT_FROM_WIN32(ERROR_TIMEOUT):
                return NOTEPADS_MARKDOWN_CORE_DEADLINE;
            case HRESULT_FROM_WIN32(ERROR_NOT_ENOUGH_QUOTA):
                return NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT;
            case HRESULT_FROM_WIN32(ERROR_ARITHMETIC_OVERFLOW):
                return NOTEPADS_MARKDOWN_CORE_ARITHMETIC_OVERFLOW;
            case E_INVALIDARG:
                return NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT;
            default:
                return NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT;
            }
        }
    }

    ReadingHighlightResult ReadingSourceState::BuildHighlights(std::span<uint32_t const> nodes,
        NativeHighlightCache *cache, PreviewCancellation const *workCancel) const noexcept
    {
        if (!Main() || !memory || !cancel)
            return {{}, NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
        try
        {
            uint32_t reused{};
            auto flag = workCancel ? workCancel->Get() : cancel->Get();
            if (auto result = NativeHighlightSnapshot::Build(memory, flag, nodes, cache, &reused))
                return {std::move(result), NOTEPADS_MARKDOWN_CORE_OK, reused};
        }
        catch (NativeFailure const &failure)
        {
            if (failure.status != NOTEPADS_MARKDOWN_CORE_OK)
                return {{}, failure.status};
        }
        catch (std::bad_alloc const &)
        {
            return {{}, NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY};
        }
        catch (winrt::hresult_error const &failure)
        {
            return {{}, HighlightStatus(failure.code())};
        }
        catch (...)
        {
        }
        return {{}, NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
    }

    void ReadingSourceState::EnsureHighlights() noexcept
    {
        // Serialized render work attempts synchronous highlighting only once.
        // Failure is optional and does not change the main reading fault.
        if (highlightingAttempted)
            return;
        highlightingAttempted = true;
        auto result = BuildHighlights();
        highlights = std::move(result.snapshot);
        highlightingStatus = result.status;
    }
}
