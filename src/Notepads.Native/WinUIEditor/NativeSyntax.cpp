// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

#include "pch.h"
#include "NativeSyntax.h"
#include "LexerModule.h"
#include <array>

extern const Lexilla::LexerModule lmCPP, lmPython, lmJSON, lmHTML, lmXML, lmCss,
		lmPowerShell, lmSQL, lmYAML, lmMarkdown, lmRust, lmBash, lmLua, lmTOML;

namespace WinUIEditor
{
#ifdef _DEBUG
	static thread_local int syntaxAllocationFailurePoint;
	void FailSyntaxAllocationForTesting(int point) noexcept { syntaxAllocationFailurePoint = point; }
	void CheckSyntaxAllocationForTesting(int point)
	{
		if (syntaxAllocationFailurePoint == point)
		{
			syntaxAllocationFailurePoint = 0;
			throw std::bad_alloc();
		}
	}
#endif
	int SyntaxHighlightingState::SizeReason(Scintilla::Internal::Document const &document) const noexcept
	{
		if (document.Length() > MaxBytes) return 1;
		if (document.LinesTotal() > MaxLines) return 2;
		return 0;
	}

	void SyntaxHighlightingState::CheckLines(Scintilla::Internal::Document const &document, Sci_Position first, Sci_Position last)
	{
		for (auto line = first; line <= last && line < document.LinesTotal(); ++line)
			if (document.LineStart(line + 1) - document.LineStart(line) > MaxLineBytes)
				_longLines.insert(line);
	}

	void SyntaxHighlightingState::Reset(Scintilla::Internal::Document const &document)
	{
		if (_memoryPaused) return;
		_longLines.clear();
		_pauseReason = SizeReason(document);
		_indexed = _pauseReason == 0;
		if (_indexed)
		{
			CheckLines(document, 0, document.LinesTotal() - 1);
			if (!_longLines.empty()) _pauseReason = 3;
		}
	}

	bool SyntaxHighlightingState::Changed(Scintilla::Internal::Document const &document, Scintilla::Internal::DocModification const &change)
	{
		if (_memoryPaused) return false;
		const auto previous = _pauseReason;
		const auto sizeReason = SizeReason(document);
		if (sizeReason != 0)
		{
			_pauseReason = sizeReason;
			_indexed = false;
			_longLines.clear();
		}
		else if (!_indexed)
			Reset(document);
		else
		{
			const auto first = document.SciLineFromPosition(change.position);
			const auto oldLast = first + std::max<Sci_Position>(0, -change.linesAdded);
			_longLines.erase(_longLines.lower_bound(first), _longLines.upper_bound(oldLast));
			if (change.linesAdded != 0)
			{
				std::set<Sci_Position> shifted;
				for (const auto line : _longLines)
					shifted.insert(line > oldLast ? line + change.linesAdded : line);
				_longLines.swap(shifted);
			}
			CheckLines(document, first, first + std::max<Sci_Position>(0, change.linesAdded));
			_pauseReason = _longLines.empty() ? 0 : 3;
		}
		return previous != _pauseReason;
	}

	namespace
	{
		// The wrapper preserves lexer configuration while admission pauses coloring.
		// It owns no view and cannot outlive the retained document's native resources.
		class AdmittedLexer final : public Scintilla::ILexer5
		{
			OwnedLexer _inner;
			std::shared_ptr<SyntaxHighlightingState> _state;
		public:
			AdmittedLexer(OwnedLexer inner, std::shared_ptr<SyntaxHighlightingState> state) :
				_inner(std::move(inner)), _state(std::move(state)) {}
			int SCI_METHOD Version() const override { return _inner->Version(); }
			void SCI_METHOD Release() override { delete this; }
			const char *SCI_METHOD PropertyNames() override { return _inner->PropertyNames(); }
			int SCI_METHOD PropertyType(const char *name) override { return _inner->PropertyType(name); }
			const char *SCI_METHOD DescribeProperty(const char *name) override { return _inner->DescribeProperty(name); }
			Sci_Position SCI_METHOD PropertySet(const char *key, const char *value) override
			{
				if (_state->PauseReason() == 4) return -1;
				try
				{
#ifdef _DEBUG
					CheckSyntaxAllocationForTesting(5);
#endif
					return _inner->PropertySet(key, value);
				}
				catch (const std::bad_alloc &) { _state->PauseForMemory(); return -1; }
			}
			const char *SCI_METHOD DescribeWordListSets() override { return _inner->DescribeWordListSets(); }
			Sci_Position SCI_METHOD WordListSet(int index, const char *words) override
			{
				if (_state->PauseReason() == 4) return -1;
				try
				{
#ifdef _DEBUG
					CheckSyntaxAllocationForTesting(4);
#endif
					return _inner->WordListSet(index, words);
				}
				catch (const std::bad_alloc &) { _state->PauseForMemory(); return -1; }
			}
			void SCI_METHOD Lex(Sci_PositionU start, Sci_Position length, int style, Scintilla::IDocument *document) override
			{
				if (_state->PauseReason() == 0)
				{
					try
					{
#ifdef _DEBUG
						CheckSyntaxAllocationForTesting(6);
#endif
						_inner->Lex(start, length, style, document);
						return;
					}
					catch (const std::bad_alloc &) { _state->PauseForMemory(); }
				}
				// Complete the requested range even after a partially styled OOM.
				// The view releases optional storage after Colourise has returned.
				document->StartStyling(static_cast<Sci_Position>(start));
				document->SetStyleFor(length, 0);
			}
			void SCI_METHOD Fold(Sci_PositionU start, Sci_Position length, int style, Scintilla::IDocument *document) override
			{
				if (_state->PauseReason() == 0)
				{
					try
					{
#ifdef _DEBUG
						CheckSyntaxAllocationForTesting(7);
#endif
						_inner->Fold(start, length, style, document);
					}
					catch (const std::bad_alloc &) { _state->PauseForMemory(); }
				}
			}
			void *SCI_METHOD PrivateCall(int operation, void *pointer) override { return _inner->PrivateCall(operation, pointer); }
			int SCI_METHOD LineEndTypesSupported() override { return _inner->LineEndTypesSupported(); }
			int SCI_METHOD AllocateSubStyles(int style, int count) override { return _inner->AllocateSubStyles(style, count); }
			int SCI_METHOD SubStylesStart(int style) override { return _inner->SubStylesStart(style); }
			int SCI_METHOD SubStylesLength(int style) override { return _inner->SubStylesLength(style); }
			int SCI_METHOD StyleFromSubStyle(int style) override { return _inner->StyleFromSubStyle(style); }
			int SCI_METHOD PrimaryStyleFromStyle(int style) override { return _inner->PrimaryStyleFromStyle(style); }
			void SCI_METHOD FreeSubStyles() override { _inner->FreeSubStyles(); }
			void SCI_METHOD SetIdentifiers(int style, const char *identifiers) override { _inner->SetIdentifiers(style, identifiers); }
			int SCI_METHOD DistanceToSecondaryStyles() override { return _inner->DistanceToSecondaryStyles(); }
			const char *SCI_METHOD GetSubStyleBases() override { return _inner->GetSubStyleBases(); }
			int SCI_METHOD NamedStyles() override { return _inner->NamedStyles(); }
			const char *SCI_METHOD NameOfStyle(int style) override { return _inner->NameOfStyle(style); }
			const char *SCI_METHOD TagsOfStyle(int style) override { return _inner->TagsOfStyle(style); }
			const char *SCI_METHOD DescriptionOfStyle(int style) override { return _inner->DescriptionOfStyle(style); }
			const char *SCI_METHOD GetName() override { return _inner->GetName(); }
			int SCI_METHOD GetIdentifier() override { return _inner->GetIdentifier(); }
			const char *SCI_METHOD PropertyGet(const char *key) override { return _inner->PropertyGet(key); }
		};
	}

	OwnedLexer CreateSyntaxLexer(std::string_view name, std::shared_ptr<SyntaxHighlightingState> const &state)
	{
		if (name.empty()) return {};
		static const std::array modules{ &::lmCPP, &::lmPython, &::lmJSON,
			&::lmHTML, &::lmXML, &::lmCss, &::lmPowerShell, &::lmSQL,
			&::lmYAML, &::lmMarkdown, &::lmRust, &::lmBash, &::lmLua, &::lmTOML };
		for (const auto module : modules)
			if (name == module->languageName)
			{
#ifdef _DEBUG
				CheckSyntaxAllocationForTesting(2);
#endif
				OwnedLexer inner{ module->Create() };
				if (!inner) throw std::bad_alloc();
				return OwnedLexer{ new AdmittedLexer(std::move(inner), state) };
			}
		throw winrt::hresult_invalid_argument(L"The requested syntax lexer is not compiled into this editor.");
	}
}
