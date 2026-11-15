// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Control/MarkdownView.h"
#include "Control/MarkdownTaskToggleRequestedEventArgs.h"
#include <limits>

namespace winrt::Notepads::Markdown::implementation
{
    bool MarkdownView::TaskInteractionEnabled() const
    {
        CheckUiThread();
        return _taskInteractionEnabled;
    }

    void MarkdownView::TaskInteractionEnabled(bool enabled)
    {
        auto lifetime = get_strong();
        CheckUiThread();
        if (_taskInteractionEnabled == enabled)
            return;
        if (_taskInteractionGeneration == std::numeric_limits<uint64_t>::max())
            throw hresult_error(E_BOUNDS);
        ++_taskInteractionGeneration;
        _taskInteractionEnabled = enabled;
        OnTaskInteractionChanged();
    }

    event_token MarkdownView::TaskToggleRequested(
        Windows::Foundation::TypedEventHandler<Markdown::MarkdownView, Markdown::MarkdownTaskToggleRequestedEventArgs> const &handler)
    {
        CheckUiThread();
        return _taskToggleRequested.add(handler);
    }

    void MarkdownView::TaskToggleRequested(event_token const &token) noexcept
    {
        _taskToggleRequested.remove(token);
    }

    bool MarkdownView::ReadingTaskEnabled(TextSnapshot const &snapshot, size_t index) const
    {
        CheckUiThread();
        CheckLiveRange(snapshot);
        if (!_taskInteractionEnabled || !snapshot.descriptors || index >= snapshot.descriptors->objects.size())
            return false;
        auto const &task = snapshot.descriptors->objects[index];
        return task.kind == ::Notepads::Markdown::Native::ReadingObjectKind::Task && !task.staticHtmlCheckbox &&
               task.hasTaskAction && ReadingChildVisible(snapshot, task.text);
    }

    void MarkdownView::InvokeReadingTask(TextSnapshot const &snapshot, size_t index)
    {
        auto lifetime = get_strong();
        CheckUiThread();
        CheckLiveRange(snapshot);
        if (!ReadingTaskEnabled(snapshot, index))
            throw hresult_error(UIA_E_ELEMENTNOTENABLED);
        auto current = _automationSnapshot;
        auto const &task = current->descriptors->objects[index];
        if (index >= std::numeric_limits<uint32_t>::max() ||
            _taskRequestId == std::numeric_limits<uint64_t>::max())
            throw hresult_error(E_BOUNDS);
        wchar_t expected[3]{static_cast<wchar_t>(task.taskExpected[0]),
                            static_cast<wchar_t>(task.taskExpected[1]),
                            static_cast<wchar_t>(task.taskExpected[2])};
        auto args = make<implementation::MarkdownTaskToggleRequestedEventArgs>(
            current->document, current->revision, current->token, ++_taskRequestId,
            static_cast<uint32_t>(index + 1), task.text.start, task.text.end - task.text.start,
            task.taskActionStart, hstring{expected, 3}, task.checked);
        // Event parameters own only scalars and the original three characters.
        // Source publication remains authoritative; no old state is used after callbacks.
        try
        {
            _taskToggleRequested(*this, args);
        }
        catch (...)
        {
            // Host failure is not a committed task change.
        }
    }
}
