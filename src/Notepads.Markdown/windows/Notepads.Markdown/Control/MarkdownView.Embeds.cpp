// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include <winrt/Windows.Foundation.Collections.h>
#include "Control/MarkdownView.h"
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
#include "Diagnostics/NativeStartupTrace.h"
#endif
#include "Control/PreviewUpdate.h"
#include "Rendering/NativeDocumentRenderer.h"
#include <cmath>

namespace winrt::Notepads::Markdown::implementation
{
    using ::Notepads::Markdown::Native::NativeDocumentRenderer;
    using ::Notepads::Markdown::Native::ReadingChildKind;
    using ::Notepads::Markdown::Native::ReadingObjectKind;
    using ::Notepads::Markdown::Native::ReadingSpan;
    using Windows::Foundation::Rect;
    using Windows::UI::Xaml::Visibility;
    using Windows::UI::Xaml::Controls::Canvas;
    using namespace Windows::UI::Xaml::Automation::Peers;

    std::optional<Rect> MarkdownView::NoteEmbedBounds(ReadingSpan span) const
    {
        if (!_presented.frame)
            return {};
        auto rects = NativeDocumentRenderer::Rectangles(*_presented.frame, span.start, span.end);
        if (rects.size() != 1)
            return {};
        auto const &rect = rects.front();
        auto const &viewport = _presented.geometry;
        auto x = rect.x + _padding;
        auto y = rect.y + _padding;
        if (rect.width <= 0 || rect.height <= 0 || x + rect.width <= 0 || y + rect.height <= 0 ||
            x >= viewport.width || y >= viewport.height)
            return {};
        return Rect{x + static_cast<float>(viewport.x), y + static_cast<float>(viewport.y), rect.width, rect.height};
    }

    Windows::Foundation::Collections::IVectorView<Markdown::MarkdownNoteEmbedSlot> MarkdownView::GetVisibleNoteEmbeds()
    {
        CheckUiThread();
        auto slots = single_threaded_vector<Markdown::MarkdownNoteEmbedSlot>();
        if (_presented.inspection || !ReadingAvailable() || !_viewportOverlay || !_presented.frame || !_presented.frame->measuredBlocks ||
            _presented.frame->measuredBlocks->empty())
            return slots.GetView();
        auto snapshot = AutomationSnapshot();
        auto const &blocks = *_presented.frame->measuredBlocks;
        auto start = blocks.front().block.logical_start;
        auto end = blocks.back().block.logical_end;
        auto const &objects = snapshot->descriptors->objects;
        auto first = std::lower_bound(objects.begin(), objects.end(), start,
            [](auto const &object, auto offset) { return object.text.end <= offset; });
        for (auto it = first; it != objects.end() && it->text.start < end && slots.Size() < _noteEmbeds.size(); ++it)
        {
            if (it->kind == ReadingObjectKind::NoteEmbed)
                if (auto bounds = NoteEmbedBounds(it->text))
                    slots.Append({it->object, hstring{std::wstring_view(it->input.data(), it->input.size())}, *bounds});
        }
        return slots.GetView();
    }

    void MarkdownView::PositionNoteEmbed(Markdown::MarkdownView const &child, Rect const &bounds)
    {
        auto const &viewport = _presented.geometry;
        Canvas::SetLeft(child, bounds.X + viewport.overlayX - viewport.x);
        Canvas::SetTop(child, bounds.Y + viewport.overlayY - viewport.y);
        child.Width(bounds.Width);
        child.Height(bounds.Height);
        child.Visibility(_presented.inspection ? Visibility::Collapsed : Visibility::Visible);
    }

    void MarkdownView::UpdateNoteEmbedExtent(Markdown::MarkdownView const &child) noexcept
    {
        try
        {
            auto slot = std::find_if(_noteEmbeds.begin(), _noteEmbeds.end(), [&](auto const &entry)
                { return entry.child == child; });
            if (slot == _noteEmbeds.end())
                return;
            auto implementation = winrt::get_self<MarkdownView>(child);
            auto frame = implementation->_presented.frame;
            if (!frame || !frame->navigation ||
                !implementation->CurrentViewportFrame(*frame, implementation->_presented.geometry))
                return;
            auto contentHeight = frame->totalHeight + 2 * implementation->_padding;
            if (!std::isfinite(contentHeight) || contentHeight < 0)
                return;
            const auto height = static_cast<float>(std::clamp(std::ceil(contentHeight), 1.0,
                static_cast<double>(::Notepads::Markdown::Native::NoteEmbedStyle::maximumHeight)));
            // A child edit can finish before its parent's replacement frame.
            // Keep the fitted height with the occurrence until it is rebound.
            slot->fittedHeight = height;
            auto main = _readingSource ? _readingSource->Main() : nullptr;
            if (!_loaded || !_documentAccepted || !main || main->SourceGeneration() != _previewSourceGeneration ||
                slot->textGeneration != main->TextGeneration())
                return;
            using Extents = ::Notepads::Markdown::Native::PreviewNoteEmbedExtents;
            using Extent = ::Notepads::Markdown::Native::PreviewNoteEmbedExtent;
            auto find = [&](auto &entries)
            {
                return std::lower_bound(entries.begin(), entries.end(), slot->object,
                    [](auto const &entry, uint32_t object) { return entry.object < object; });
            };
            if (_noteEmbedExtents)
            {
                auto prior = find(*_noteEmbedExtents);
                if (prior != _noteEmbedExtents->end() && prior->object == slot->object && prior->height == height)
                    return;
            }
            // The owning reading budget funds retained geometry. Keep it when
            // a child leaves the viewport so scrolling does not resize the source.
            auto memory = main->Memory();
            auto extents = std::allocate_shared<Extents>(
                ::Notepads::Markdown::Native::ReadingAllocator<Extents>{memory},
                ::Notepads::Markdown::Native::ReadingAllocator<Extent>{memory});
            if (_noteEmbedExtents)
                *extents = *_noteEmbedExtents;
            auto at = find(*extents);
            if (at != extents->end() && at->object == slot->object)
                at->height = height;
            else
                extents->insert(at, {slot->object, height});
            CaptureImageAnchorBeforeChange();
            _noteEmbedExtents = std::move(extents);
            StartPreviewLayout();
        }
        catch (...)
        {
            // Optional fitting must leave the already published child usable
            // if the shared reading budget cannot fund another extent snapshot.
        }
    }

    bool MarkdownView::SetNoteEmbedChild(guid document, uint64_t revision, uint64_t textGeneration,
                                        uint32_t objectId, Markdown::MarkdownView const &child)
    {
        CheckUiThread();
        if (!objectId || !textGeneration)
            return false;
        auto find = std::find_if(_noteEmbeds.begin(), _noteEmbeds.end(), [&](auto const &slot)
            {
                return slot.child && ((slot.object == objectId && slot.textGeneration == textGeneration) ||
                    (!child && slot.previousObject == objectId && slot.previousTextGeneration == textGeneration));
            });
        if (!child)
        {
            if (find == _noteEmbeds.end())
                return false;
            auto detached = std::exchange(*find, MountedNoteEmbed{});
            auto implementation = winrt::get_self<MarkdownView>(detached.child);
            implementation->_noteParentView = {};
            // Existing child sessions/frames own the shared budget root. This
            // mount-only bootstrap pin must not retain the old parent model.
            implementation->_embedBudgetParent.Reset();
            if (!implementation->_loaded)
            {
                if (implementation->_previewUpdate)
                    implementation->_previewUpdate->Complete(Markdown::DocumentUpdateStatus::NotLoaded);
                implementation->StopPreview(Markdown::DocumentUpdateStatus::NotLoaded, true, false);
            }
            if (_viewportOverlay)
            {
                uint32_t index{};
                auto children = _viewportOverlay.Children();
                if (children.IndexOf(detached.child, index))
                    children.RemoveAt(index);
            }
            RaiseNoteEmbedStructureChanged();
            return true;
        }
        if (!ReadingAvailable() || !_viewportOverlay)
            return false;
        auto snapshot = AutomationSnapshot();
        if (snapshot->document != document || snapshot->revision != revision || snapshot->token != textGeneration ||
            winrt::get_self<MarkdownView>(child) == this)
            return false;
        if (find != _noteEmbeds.end())
        {
            if (find->child != child)
                return false;
            find->previousObject = 0;
            find->previousTextGeneration = 0;
            return true;
        }
        auto const &objects = snapshot->descriptors->objects;
        auto object = std::find_if(objects.begin(), objects.end(), [&](auto const &row)
            { return row.kind == ReadingObjectKind::NoteEmbed && row.object == objectId; });
        if (object == objects.end())
            return false;
        auto bounds = NoteEmbedBounds(object->text);
        auto free = std::find_if(_noteEmbeds.begin(), _noteEmbeds.end(), [](auto const &slot) { return !slot.child; });
        auto implementation = winrt::get_self<MarkdownView>(child);
        if (!bounds || bounds->Width <= 2 * implementation->_padding || free == _noteEmbeds.end() ||
            implementation->_loaded || implementation->_previewController ||
            implementation->_isEmbeddedChild || implementation->_embedBudgetParent)
            return false;
        implementation->_embedBudgetParent = snapshot->main->ModelOwner().Clone();
        implementation->_noteParentView = get_weak();
        implementation->_isEmbeddedChild = true;
        try
        {
            child.TaskInteractionEnabled(false);
            PositionNoteEmbed(child, *bounds);
            _viewportOverlay.Children().Append(child);
        }
        catch (...)
        {
            implementation->_noteParentView = {};
            implementation->_embedBudgetParent.Reset();
            implementation->_isEmbeddedChild = false;
            throw;
        }
        *free = {objectId, textGeneration, child};
        // A unique, exact authored export can be followed through the byte
        // edit stream without treating transient parse IDs as stable identity.
        try
        {
            auto authored = to_string(hstring{std::wstring_view{object->source.data(), object->source.size()}});
            if (_acceptedSource && _acceptedDocument == document && _acceptedRevision == revision && !authored.empty())
            {
                auto bytes = std::string_view{reinterpret_cast<char const *>(_acceptedSource->data()), _acceptedSource->size()};
                auto at = bytes.find(authored);
                if (at != bytes.npos && bytes.find(authored, at + 1) == bytes.npos)
                    free->sourceSpan = std::pair<uint64_t, uint64_t>{at, at + authored.size()};
            }
        }
        catch (...) {}
        return true;
    }

    AutomationPeer MarkdownView::ReadingNotePeer(TextSnapshot const &snapshot, ReadingChildKind kind, size_t index) const
    {
        if (kind != ReadingChildKind::Object || snapshot.descriptors->objects[index].kind != ReadingObjectKind::NoteEmbed ||
            !ReadingChildVisible(snapshot, snapshot.descriptors->objects[index].text))
            return nullptr;
        auto const id = snapshot.descriptors->objects[index].object;
        for (auto const &slot : _noteEmbeds)
        {
            if (!slot.child || slot.object != id || slot.textGeneration != snapshot.token ||
                slot.child.Visibility() != Visibility::Visible || !winrt::get_self<MarkdownView>(slot.child)->ReadingAvailable())
                continue;
            auto peer = FrameworkElementAutomationPeer::FromElement(slot.child);
            return peer ? peer : FrameworkElementAutomationPeer::CreatePeerForElement(slot.child);
        }
        return nullptr;
    }

    std::optional<ReadingSpan> MarkdownView::ReadingNoteChildSpan(TextSnapshot const &snapshot,
                                                                 Markdown::MarkdownView const &child) const
    {
        CheckLiveRange(snapshot);
        for (auto const &slot : _noteEmbeds)
        {
            if (!slot.child || slot.child != child || slot.textGeneration != snapshot.token ||
                child.Visibility() != Visibility::Visible || !winrt::get_self<MarkdownView>(child)->ReadingAvailable())
                continue;
            auto const &objects = snapshot.descriptors->objects;
            auto at = std::find_if(objects.begin(), objects.end(), [&](auto const &entry)
                { return entry.kind == ReadingObjectKind::NoteEmbed && entry.object == slot.object; });
            if (at != objects.end())
                return at->text;
        }
        return {};
    }

    void MarkdownView::RaiseNoteEmbedStructureChanged() noexcept
    {
        try
        {
            auto peer = FrameworkElementAutomationPeer::FromElement(*this);
            if (peer && ReadingAvailable())
            {
                peer.InvalidatePeer();
                peer.RaiseAutomationEvent(AutomationEvents::StructureChanged);
            }
        }
        catch (...) {}
    }

    void MarkdownView::RestrictNoteEmbedToHeading(hstring const &title)
    {
        CheckUiThread();
        if (!_embedBudgetParent || _documentAccepted || title.empty())
            throw hresult_invalid_argument();
        _noteSectionTitle = title;
    }

    void MarkdownView::UpdateNoteEmbeds()
    {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        namespace UiTrace = ::Notepads::Markdown::Native::StartupTrace;
        UiTrace::UiScope uiTrace{UiTrace::Phase::UiUpdateNotes, UiTrace::Phase::UiUpdateNotesEnd, _previewLayoutGeneration};
#endif
        if (std::none_of(_noteEmbeds.begin(), _noteEmbeds.end(), [](auto const &slot) { return !!slot.child; }))
            return;
        auto visible = GetVisibleNoteEmbeds();
        bool changed{};
        for (auto const &slot : _noteEmbeds)
        {
            if (!slot.child)
                continue;
            const auto prior = slot.child.Visibility();
            bool found{};
            if (_automationSnapshot && _automationSnapshot->token == slot.textGeneration)
                for (auto const &bounds : visible)
                    if (bounds.ObjectId == slot.object)
                    {
                        PositionNoteEmbed(slot.child, bounds.Bounds);
                        found = true;
                        break;
                    }
            if (!found)
                slot.child.Visibility(Visibility::Collapsed);
            changed |= prior != slot.child.Visibility();
        }
        if (changed)
            RaiseNoteEmbedStructureChanged();
    }

    bool MarkdownView::PrepareNoteEmbedsForEdit(std::span<notepads_markdown_core_edit_row const> edits) noexcept
    {
        bool retain{};
        for (auto &slot : _noteEmbeds)
        {
            if (!slot.child || !slot.sourceSpan)
                continue;
            // An already fitted child may temporarily lack a current frame
            // while forwarding the preceding source edit or changing width.
            if (!slot.fittedHeight && !winrt::get_self<MarkdownView>(slot.child)->ReadingAvailable())
            {
                slot.sourceSpan.reset();
                continue;
            }
            auto [start, end] = *slot.sourceSpan;
            int64_t delta{};
            for (auto const &edit : edits)
            {
                if (edit.end <= start)
                {
                    delta += static_cast<int64_t>(edit.replacement_length) -
                             static_cast<int64_t>(edit.end - edit.start);
                    continue;
                }
                if (edit.start < end)
                    slot.sourceSpan.reset();
                break;
            }
            if (slot.sourceSpan)
            {
                slot.sourceSpan = std::pair{static_cast<uint64_t>(static_cast<int64_t>(start) + delta),
                                            static_cast<uint64_t>(static_cast<int64_t>(end) + delta)};
                retain = true;
            }
        }
        return _noteEmbedsPending = retain;
    }

    bool MarkdownView::RestoreNoteEmbeds(::Notepads::Markdown::Native::PreviewFrame const &frame)
    {
        if (!_noteEmbedsPending)
            return false;
        _noteEmbedsPending = false;
        auto const source = _acceptedSource;
        if (!source)
        {
            ClearNoteEmbeds();
            return false;
        }
        auto bytes = std::string_view{reinterpret_cast<char const *>(source->data()), source->size()};
        std::array<uint32_t, 8> rebound{};
        std::array<::Notepads::Markdown::Native::CoreModelPin, 8> parents;
        using Extents = ::Notepads::Markdown::Native::PreviewNoteEmbedExtents;
        using Extent = ::Notepads::Markdown::Native::PreviewNoteEmbedExtent;
        std::shared_ptr<Extents> extents;
        try
        {
            for (size_t index = 0; index < _noteEmbeds.size(); ++index)
            {
                auto const &slot = _noteEmbeds[index];
                if (!slot.child || !slot.sourceSpan)
                    continue;
                auto [start, end] = *slot.sourceSpan;
                if (start >= end || end > bytes.size())
                    continue;
                auto authored = bytes.substr(static_cast<size_t>(start), static_cast<size_t>(end - start));
                // The old parsed export located exactly these bytes. A retained
                // occurrence must still be unique and parsed as a note here.
                if (bytes.find(authored) != start || bytes.find(authored, static_cast<size_t>(start + 1)) != bytes.npos)
                    continue;
                auto markup = to_hstring(authored);
                auto const &objects = frame.descriptors->objects;
                auto matches = [&](auto const &object)
                {
                    return object.kind == ReadingObjectKind::NoteEmbed &&
                           std::wstring_view{object.source.data(), object.source.size()} == std::wstring_view{markup};
                };
                auto object = std::find_if(objects.begin(), objects.end(), matches);
                if (object == objects.end() || std::any_of(std::next(object), objects.end(), matches))
                    continue;
                parents[index] = frame.main->ModelOwner().Clone();
                rebound[index] = object->object;
                if (slot.fittedHeight > 0)
                {
                    if (!extents)
                    {
                        auto memory = frame.main->Memory();
                        extents = std::allocate_shared<Extents>(
                            ::Notepads::Markdown::Native::ReadingAllocator<Extents>{memory},
                            ::Notepads::Markdown::Native::ReadingAllocator<Extent>{memory});
                    }
                    extents->push_back({object->object, slot.fittedHeight});
                }
            }
            if (extents)
                std::sort(extents->begin(), extents->end(),
                          [](auto const &left, auto const &right) { return left.object < right.object; });
        }
        catch (...)
        {
            // Reuse is optional. The ordinary mount path remains available.
            ClearNoteEmbeds();
            return false;
        }
        _noteEmbedExtents = std::move(extents);
        const bool changedGeometry = static_cast<bool>(_noteEmbedExtents);
        std::array<MountedNoteEmbed, 8> retired;
        for (size_t index = 0; index < _noteEmbeds.size(); ++index)
        {
            auto &slot = _noteEmbeds[index];
            if (!rebound[index])
            {
                retired[index] = std::exchange(slot, MountedNoteEmbed{});
                continue;
            }
            // The host may retire an offscreen child before seeing the new
            // placement. Its last acknowledged stamp still owns that detach.
            if (!slot.previousTextGeneration)
            {
                slot.previousObject = slot.object;
                slot.previousTextGeneration = slot.textGeneration;
            }
            slot.object = rebound[index];
            slot.textGeneration = frame.descriptors->textGeneration;
            winrt::get_self<MarkdownView>(slot.child)->_embedBudgetParent = std::move(parents[index]);
        }
        for (auto &slot : retired)
            ClearNoteEmbed(slot);
        return changedGeometry;
    }

    void MarkdownView::ClearNoteEmbed(MountedNoteEmbed &slot) noexcept
    {
        auto detached = std::exchange(slot, MountedNoteEmbed{});
        if (!detached.child)
            return;
        auto implementation = winrt::get_self<MarkdownView>(detached.child);
        implementation->_noteParentView = {};
        implementation->_embedBudgetParent.Reset();
        if (!implementation->_loaded)
        {
            // A child removed before Loaded has no Unloaded event to
            // settle its initial viewport wait.
            if (implementation->_previewUpdate)
                implementation->_previewUpdate->Complete(Markdown::DocumentUpdateStatus::NotLoaded);
            try { implementation->StopPreview(Markdown::DocumentUpdateStatus::NotLoaded, true, false); }
            catch (...) {}
        }
        if (_viewportOverlay)
        {
            try
            {
                uint32_t index{};
                auto children = _viewportOverlay.Children();
                if (children.IndexOf(detached.child, index))
                    children.RemoveAt(index);
            }
            catch (...) {}
        }
    }

    void MarkdownView::ClearNoteEmbeds() noexcept
    {
        _noteEmbedsPending = false;
        auto retired = std::exchange(_noteEmbeds, {});
        for (auto &slot : retired)
            ClearNoteEmbed(slot);
    }
}
