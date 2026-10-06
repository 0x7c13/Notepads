// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

#pragma once
#include "ILexer.h"
#include <memory>
#include <set>
#include <string_view>

namespace Scintilla::Internal { class Document; class DocModification; }

namespace WinUIEditor
{
	// Admission state is shared with the document's lexer, without retaining a view.
	class SyntaxHighlightingState
	{
	public:
		int PauseReason() const noexcept { return _pauseReason; }
		void PauseForMemory() noexcept { _memoryPaused = true; _pauseReason = 4; _longLines.clear(); _indexed = false; }
		void Reset(Scintilla::Internal::Document const &document);
		bool Changed(Scintilla::Internal::Document const &document, Scintilla::Internal::DocModification const &change);
	private:
		static constexpr Sci_Position MaxBytes = 32 * 1024 * 1024;
		static constexpr Sci_Position MaxLines = 200000;
		static constexpr Sci_Position MaxLineBytes = 64 * 1024;
		int _pauseReason{};
		bool _memoryPaused{};
		bool _indexed{};
		std::set<Sci_Position> _longLines;
		int SizeReason(Scintilla::Internal::Document const &document) const noexcept;
		void CheckLines(Scintilla::Internal::Document const &document, Sci_Position first, Sci_Position last);
	};

	struct LexerRelease
	{
		void operator()(Scintilla::ILexer5 *lexer) const noexcept { if (lexer) lexer->Release(); }
	};
	using OwnedLexer = std::unique_ptr<Scintilla::ILexer5, LexerRelease>;
	OwnedLexer CreateSyntaxLexer(std::string_view name, std::shared_ptr<SyntaxHighlightingState> const &state);
#ifdef _DEBUG
	void FailSyntaxAllocationForTesting(int point) noexcept;
	void CheckSyntaxAllocationForTesting(int point);
#endif
}
