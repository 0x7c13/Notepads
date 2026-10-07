// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

#include "pch.h"
#include "EditorBaseControl.h"
#include "EditorWrapper.h"

namespace winrt::WinUIEditor::implementation
{
	void Editor::SetLexerLanguage(hstring const &name, array_view<hstring const> keywords,
		array_view<hstring const> propertyNames, array_view<hstring const> propertyValues)
	{
		auto view = _editor.get();
		if (!view) winrt::throw_hresult(RO_E_CLOSED);
		if (!view->Dispatcher().HasThreadAccess()) winrt::throw_hresult(RPC_E_WRONG_THREAD);
		if (_textLoadState && !_textLoadState->canceled.load()) winrt::throw_hresult(E_ILLEGAL_METHOD_CALL);
		view->SetSyntaxLexer(name, keywords, propertyNames, propertyValues);
	}

	WinUIEditor::EditorSyntaxPauseReason Editor::SyntaxHighlightingPauseReason()
	{
		auto view = _editor.get();
		if (!view) winrt::throw_hresult(RO_E_CLOSED);
		if (!view->Dispatcher().HasThreadAccess()) winrt::throw_hresult(RPC_E_WRONG_THREAD);
		return view->SyntaxHighlightingPauseReason();
	}
}
