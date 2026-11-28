// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "DocumentUpdateResult.g.h"

namespace winrt::Notepads::Markdown::implementation
{
    struct DocumentUpdateResult : DocumentUpdateResultT<DocumentUpdateResult>
    {
        DocumentUpdateResult(guid document, uint64_t revision, uint64_t generation,
            Markdown::DocumentUpdateStatus status, int32_t errorCode, bool published, uint64_t blockCount) noexcept
            : _document(document), _revision(revision), _generation(generation), _status(status),
              _errorCode(errorCode), _published(published), _blockCount(blockCount)
        {
        }

        guid DocumentId() const noexcept { return _document; }
        uint64_t Revision() const noexcept { return _revision; }
        uint64_t Generation() const noexcept { return _generation; }
        Markdown::DocumentUpdateStatus Status() const noexcept { return _status; }
        int32_t ErrorCode() const noexcept { return _errorCode; }
        bool Published() const noexcept { return _published; }
        uint64_t BlockCount() const noexcept { return _blockCount; }

    private:
        guid _document;
        uint64_t _revision;
        uint64_t _generation;
        Markdown::DocumentUpdateStatus _status;
        int32_t _errorCode;
        bool _published;
        uint64_t _blockCount;
    };
}
