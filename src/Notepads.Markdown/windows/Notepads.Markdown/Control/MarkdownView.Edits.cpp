// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Control/MarkdownView.h"
#include "Control/PreviewUpdate.h"
#include <cstring>

namespace winrt::Notepads::Markdown::implementation
{
    namespace
    {
        constexpr uint64_t MaximumSourceBytes = 4u * 1024u * 1024u;
        constexpr uint32_t MaximumEditRows = 256;

        Markdown::DocumentUpdateStatus EditStatus(int32_t status) noexcept
        {
            switch (status)
            {
            case NOTEPADS_MARKDOWN_CORE_CANCELLED:
                return Markdown::DocumentUpdateStatus::Canceled;
            case NOTEPADS_MARKDOWN_CORE_RESYNC_REQUIRED:
            case NOTEPADS_MARKDOWN_CORE_WRONG_REVISION:
            case NOTEPADS_MARKDOWN_CORE_WRONG_DOCUMENT:
                return Markdown::DocumentUpdateStatus::ResyncRequired;
            case NOTEPADS_MARKDOWN_CORE_INVALID_UTF8:
            case NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT:
            case NOTEPADS_MARKDOWN_CORE_INVALID_RANGE:
            case NOTEPADS_MARKDOWN_CORE_INVALID_BOUNDARY:
            case NOTEPADS_MARKDOWN_CORE_INVALID_EDITS:
            case NOTEPADS_MARKDOWN_CORE_ARITHMETIC_OVERFLOW:
                return Markdown::DocumentUpdateStatus::InvalidInput;
            case NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT:
            case NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY:
            case NOTEPADS_MARKDOWN_CORE_DEADLINE:
                return Markdown::DocumentUpdateStatus::ResourceLimit;
            default:
                return Markdown::DocumentUpdateStatus::Failed;
            }
        }
    }

    void MarkdownView::RecordPreviewSourceState(::Notepads::Markdown::Native::CoreSubmitResult const &result) noexcept
    {
        // Only synchronous preparation returns paid state. In particular, late
        // parse/layout receipts never write this source key or the saved bytes.
        _previewSourceState = result.sourceState;
        if (!_previewSourceState.state_valid || _previewSourceState.needs_resync)
            _editChannelNeedsResync = true;
    }

    Markdown::MarkdownSourceState MarkdownView::GetSourceState() const
    {
        CheckUiThread();
        Markdown::MarkdownSourceState result{};
        result.StateKnown = _previewSourceState.state_valid != 0;
        result.HasSource = result.StateKnown && _previewSourceState.has_source != 0;
        if (result.HasSource)
        {
            std::memcpy(&result.DocumentId, _previewSourceState.source.document, 16);
            result.Revision = _previewSourceState.source.revision;
        }
        result.NeedsResync = !result.StateKnown || _previewSourceState.needs_resync != 0 ||
                             _editChannelNeedsResync || !IsPreviewLoaded();
        return result;
    }

    Windows::Foundation::IAsyncOperation<Markdown::DocumentUpdateResult> MarkdownView::ApplyEditsAsync(
        guid document, uint64_t baseRevision, uint64_t revision, array_view<Markdown::MarkdownSourceEdit const> edits,
        array_view<uint8_t const> replacements)
    {
        CheckUiThread();
        auto update = std::make_shared<PreviewUpdate>();
        update->document = document;
        update->revision = revision;
        update->generation = _generation;
        auto reject = [&](Markdown::DocumentUpdateStatus status, int32_t error)
        {
            _editChannelNeedsResync = true;
            if (_previewUpdate)
                _previewUpdate->fullSynchronization = false;
            update->Complete(status, error);
        };
        try
        {
            if (!CaptureViewportGeometry() && _viewportTemplateState == ViewportTemplateState::Unsupported)
            {
                reject(Markdown::DocumentUpdateStatus::Failed, E_INVALIDARG);
                return AwaitPreview(update);
            }
            auto source = GetSourceState();
            if (!IsPreviewLoaded() || ActualWidth() <= 0 || ActualHeight() <= 0 || !_surfaceTarget || !_documentAccepted ||
                !_previewController || !_acceptedSource || !source.HasSource || source.NeedsResync ||
                source.DocumentId != document || source.Revision != baseRevision ||
                _acceptedDocument != document || _acceptedRevision != baseRevision)
            {
                reject(Markdown::DocumentUpdateStatus::ResyncRequired, NOTEPADS_MARKDOWN_CORE_RESYNC_REQUIRED);
                return AwaitPreview(update);
            }
            if (revision <= baseRevision || edits.empty() || edits.size() > MaximumEditRows ||
                replacements.size() > MaximumSourceBytes || _generation == UINT64_MAX ||
                _previewSourceGeneration == UINT64_MAX)
            {
                reject(Markdown::DocumentUpdateStatus::InvalidInput, NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT);
                return AwaitPreview(update);
            }

            // Neither borrowed WinRT array escapes this synchronous entry.
            // The core consumes these owned rows/pool before AwaitPreview.
            std::vector<Markdown::MarkdownSourceEdit> ownedEdits(edits.begin(), edits.end());
            std::vector<uint8_t> ownedReplacements(replacements.begin(), replacements.end());
            std::vector<notepads_markdown_core_edit_row> rows;
            rows.reserve(ownedEdits.size());
            uint64_t removed{}, inserted{}, previousStart{}, previousEnd{};
            uint64_t beforeBytes = _acceptedSource->size();
            for (auto const &edit : ownedEdits)
            {
                if (edit.Start > beforeBytes || edit.DeleteLength > beforeBytes - edit.Start ||
                    edit.ReplacementOffset > ownedReplacements.size() ||
                    edit.ReplacementLength > ownedReplacements.size() - edit.ReplacementOffset ||
                    (!rows.empty() && (edit.Start <= previousStart || edit.Start < previousEnd)))
                {
                    reject(Markdown::DocumentUpdateStatus::InvalidInput, NOTEPADS_MARKDOWN_CORE_INVALID_EDITS);
                    return AwaitPreview(update);
                }
                // Nonoverlapping deleted ranges sum to at most beforeBytes;
                // bound cumulative inserts even when rows reuse the same pool.
                if (edit.ReplacementLength > MaximumSourceBytes - inserted)
                {
                    reject(Markdown::DocumentUpdateStatus::InvalidInput, NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT);
                    return AwaitPreview(update);
                }
                removed += edit.DeleteLength;
                inserted += edit.ReplacementLength;
                previousStart = edit.Start;
                previousEnd = edit.Start + edit.DeleteLength;
                rows.push_back({edit.Start, previousEnd, edit.ReplacementOffset, edit.ReplacementLength});
            }
            if (beforeBytes - removed > MaximumSourceBytes - inserted)
            {
                reject(Markdown::DocumentUpdateStatus::InvalidInput, NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT);
                return AwaitPreview(update);
            }
            auto afterBytes = static_cast<size_t>(beforeBytes - removed + inserted);
            auto after = std::make_shared<std::vector<uint8_t>>();
            after->reserve(afterBytes);
            size_t cursor{};
            for (auto const &row : rows)
            {
                after->insert(after->end(), _acceptedSource->begin() + cursor,
                              _acceptedSource->begin() + static_cast<size_t>(row.start));
                auto replacement = ownedReplacements.begin() + static_cast<size_t>(row.replacement_offset);
                after->insert(after->end(), replacement, replacement + static_cast<size_t>(row.replacement_length));
                cursor = static_cast<size_t>(row.end);
            }
            after->insert(after->end(), _acceptedSource->begin() + cursor, _acceptedSource->end());

            notepads_markdown_core_edit_batch descriptor{};
            descriptor.header = {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(descriptor)};
            std::memcpy(descriptor.before.document, &document, 16);
            descriptor.before.revision = baseRevision;
            descriptor.after = descriptor.before;
            descriptor.after.revision = revision;
            descriptor.count = rows.size();
            descriptor.row_size = sizeof(notepads_markdown_core_edit_row);

            // Keep the existing surface until a new frame is ready; retire only
            // reading permissions/owners and work tied to the previous source.
            const bool keepNotes = std::any_of(_noteEmbeds.begin(), _noteEmbeds.end(),
                [](auto const &slot) { return slot.child && slot.sourceSpan.has_value(); });
            StopPreview(Markdown::DocumentUpdateStatus::Superseded, false, true, true, keepNotes, true);
            update->incrementalEdit = true;
            _previewDeviceRetry = false;
            ++_generation;
            ++_previewSourceGeneration;
            update->generation = _generation;
            update->sourceGeneration = _previewSourceGeneration;
            _previewUpdate = update;
            _previewSubmittedSourceGeneration = update->sourceGeneration;
            _deferredRender = false;

            // Every batch enters the core atomically, including batches whose
            // parse will be coalesced away by a later synchronous submission.
            auto result = _previewController->SubmitEdits(descriptor, rows, ownedReplacements, &_previewTicket);
            RecordPreviewSourceState(result);
            bool accepted = result.sourceState.state_valid && result.sourceState.has_source &&
                            result.sourceState.source_accepted && result.sourceState.source.revision == revision &&
                            std::memcmp(result.sourceState.source.document, &document, 16) == 0;
            if (accepted)
            {
                // Paid acceptance is authoritative even when a later part of
                // preparation reports an error. Never roll back to displayed S.
                _acceptedSource = std::move(after);
                _acceptedDocument = document;
                _acceptedRevision = revision;
                if (keepNotes && !PrepareNoteEmbedsForEdit(rows))
                    ClearNoteEmbeds();
            }
            else
            {
                _editChannelNeedsResync = true;
                if (keepNotes)
                    ClearNoteEmbeds();
            }
            update->BindParse(_previewTicket);
            if (result.status != NOTEPADS_MARKDOWN_CORE_OK || !accepted)
            {
                _derivedEditReuse.reset();
                auto error = result.status == NOTEPADS_MARKDOWN_CORE_OK ? NOTEPADS_MARKDOWN_CORE_RESYNC_REQUIRED : result.status;
                update->Complete(EditStatus(error), error);
                _renderingError = L"Markdown edit submit failed (" + to_hstring(error) + L").";
                _lastStatus = Markdown::RenderingStatus::InvalidState;
            }
            else
            {
                ObservePreviewParse(get_weak(), Dispatcher(), _previewTicket, update);
            }
        }
        catch (std::bad_alloc const &)
        {
            reject(Markdown::DocumentUpdateStatus::ResourceLimit, NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY);
        }
        catch (...)
        {
            reject(Markdown::DocumentUpdateStatus::Failed, static_cast<int32_t>(to_hresult()));
        }
        return AwaitPreview(std::move(update));
    }
}
