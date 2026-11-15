// Copyright 2026 by Breece Walker
// See src/Notepads.Editor/LICENSE for the original WinUIEdit license.
//
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
// See LICENSE.txt in the project root for the Notepads modifications.

#include "pch.h"
#include "EditorBaseControl.h"

using namespace winrt;

namespace winrt::WinUIEditor::implementation
{
    int64_t EditorBaseControl::ViewportSourceByte()
    {
        if (IsFinalized()) throw_hresult(RO_E_CLOSED);
        if (!Dispatcher().HasThreadAccess()) throw_hresult(RPC_E_WRONG_THREAD);
        return _scintilla->ViewportSourceByte();
    }

    WinUIEditor::SourceViewportSpan EditorBaseControl::GetSourceViewport()
    {
        if (IsFinalized()) throw_hresult(RO_E_CLOSED);
        if (!Dispatcher().HasThreadAccess()) throw_hresult(RPC_E_WRONG_THREAD);
        return _scintilla->GetSourceViewport();
    }

    void EditorBaseControl::RevealSourceByte(int64_t position)
    {
        if (IsFinalized()) throw_hresult(RO_E_CLOSED);
        if (!Dispatcher().HasThreadAccess()) throw_hresult(RPC_E_WRONG_THREAD);
        _scintilla->RevealSourceByte(position);
    }

    bool EditorBaseControl::SetViewportBetweenSourcePositions(int64_t start, int64_t end, double progress)
    {
        if (IsFinalized()) throw_hresult(RO_E_CLOSED);
        if (!Dispatcher().HasThreadAccess()) throw_hresult(RPC_E_WRONG_THREAD);
        return _scintilla->SetViewportBetweenSourcePositions(start, end, progress);
    }

    double EditorBaseControl::VerticalScrollProgress()
    {
        if (IsFinalized()) return -1;
        if (!Dispatcher().HasThreadAccess()) throw_hresult(RPC_E_WRONG_THREAD);
        return _scintilla->VerticalScrollProgress();
    }

    double EditorBaseControl::VerticalScrollExtent()
    {
        if (IsFinalized()) return -1;
        if (!Dispatcher().HasThreadAccess()) throw_hresult(RPC_E_WRONG_THREAD);
        return _scintilla->VerticalScrollExtent();
    }

    bool EditorBaseControl::TrySetVerticalScrollProgress(double progress)
    {
        if (IsFinalized()) return false;
        if (!Dispatcher().HasThreadAccess()) throw_hresult(RPC_E_WRONG_THREAD);
        return _scintilla->TrySetVerticalScrollProgress(progress);
    }

}
