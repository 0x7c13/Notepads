// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "MarkdownTaskToggleRequestedEventArgs.g.h"
#include <utility>

namespace winrt::Notepads::Markdown::implementation
{
    struct MarkdownTaskToggleRequestedEventArgs : MarkdownTaskToggleRequestedEventArgsT<MarkdownTaskToggleRequestedEventArgs>
    {
        MarkdownTaskToggleRequestedEventArgs(guid document, uint64_t revision, uint64_t textGeneration,
                                            uint64_t requestId, uint32_t taskId, uint64_t start, uint64_t length,
                                            uint64_t sourceOffset, hstring expected, bool checked)
            : _document(document), _revision(revision), _textGeneration(textGeneration), _requestId(requestId),
              _taskId(taskId), _start(start), _length(length), _sourceOffset(sourceOffset),
              _expected(std::move(expected)), _checked(checked)
        {
        }
        guid DocumentId() const noexcept
        {
            return _document;
        }
        uint64_t Revision() const noexcept
        {
            return _revision;
        }
        uint64_t TextGeneration() const noexcept
        {
            return _textGeneration;
        }
        uint64_t RequestId() const noexcept
        {
            return _requestId;
        }
        uint32_t TaskId() const noexcept
        {
            return _taskId;
        }
        uint64_t Start() const noexcept
        {
            return _start;
        }
        uint64_t Length() const noexcept
        {
            return _length;
        }
        uint64_t SourceByteOffset() const noexcept
        {
            return _sourceOffset;
        }
        uint64_t SourceByteLength() const noexcept
        {
            return 3;
        }
        hstring ExpectedSource() const
        {
            return _expected;
        }
        bool IsChecked() const noexcept
        {
            return _checked;
        }
        bool RequestedChecked() const noexcept
        {
            return !_checked;
        }

      private:
        guid _document;
        uint64_t _revision;
        uint64_t _textGeneration;
        uint64_t _requestId;
        uint32_t _taskId;
        uint64_t _start;
        uint64_t _length;
        uint64_t _sourceOffset;
        hstring _expected;
        bool _checked;
    };
}
