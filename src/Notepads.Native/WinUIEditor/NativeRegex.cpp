// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

#include "pch.h"
#include "NativeRegex.h"
#include <list>

namespace WinUIEditor
{
	using Status = winrt::WinUIEditor::EditorSearchStatus;
	namespace
	{
		using Matcher = std::unique_ptr<URegularExpression, decltype(&uregex_close)>;
		constexpr int32_t MaximumGroups = 1024;
		struct CacheEntry
		{
			std::wstring pattern;
			bool matchCase;
			Matcher expression;
		};
		std::mutex CacheMutex;
		std::list<CacheEntry> PatternCache;

		void CheckIcu(UErrorCode error, RegexJob const &job, int32_t offset = -1)
		{
			job.Check();
			if (U_SUCCESS(error)) return;
			if (error == U_REGEX_TIME_OUT) throw RegexFailure{Status::TimedOut};
			if (error == U_REGEX_STACK_OVERFLOW || error == U_MEMORY_ALLOCATION_ERROR || error == U_BUFFER_OVERFLOW_ERROR)
				throw RegexFailure{Status::ResourceLimit};
			throw RegexFailure{Status::InvalidPattern, offset};
		}
		Matcher Compile(RegexRequest const &request, RegexJob const &job)
		{
			if (request.pattern.size() > MaximumRegexInput || request.replacement.size() > MaximumRegexInput)
				throw RegexFailure{Status::ResourceLimit};
			UErrorCode error = U_ZERO_ERROR;
			{
				std::lock_guard guard(CacheMutex);
				for (auto entry = PatternCache.begin(); entry != PatternCache.end(); ++entry)
				{
					if (entry->matchCase == request.matchCase && entry->pattern == request.pattern)
					{
						auto result = Matcher(uregex_clone(entry->expression.get(), &error), uregex_close);
						PatternCache.splice(PatternCache.begin(), PatternCache, entry);
						CheckIcu(error, job);
						return result;
					}
				}
			}
			UParseError parse{};
			auto flags = UREGEX_MULTILINE | (request.matchCase ? 0 : UREGEX_CASE_INSENSITIVE);
			Matcher compiled(uregex_open(reinterpret_cast<UChar const *>(request.pattern.data()),
				static_cast<int32_t>(request.pattern.size()), flags, &parse, &error), uregex_close);
			CheckIcu(error, job, parse.offset);
			if (uregex_groupCount(compiled.get(), &error) > MaximumGroups) throw RegexFailure{Status::ResourceLimit};
			Matcher result(uregex_clone(compiled.get(), &error), uregex_close);
			CheckIcu(error, job);
			{
				std::lock_guard guard(CacheMutex);
				PatternCache.push_front({request.pattern, request.matchCase, std::move(compiled)});
				if (PatternCache.size() > 8) PatternCache.pop_back();
			}
			return result;
		}
		UBool U_CALLCONV MatchCallback(const void *context, int32_t) noexcept
		{
			return static_cast<RegexJob const *>(context)->Interruption() == Status::Found;
		}
		struct ScanProgress
		{
			RegexJob const &job;
			mutable uint32_t calls{};
		};
		UBool U_CALLCONV FindCallback(const void *context, int64_t) noexcept
		{
			// ICU calls this per scanned start position, so the clock is read every 1024 calls.
			auto const &progress = *static_cast<ScanProgress const *>(context);
			if (progress.job.canceled.load()) return false;
			return ++progress.calls % 1024 != 0 || progress.job.Interruption() == Status::Found;
		}
		struct Token
		{
			enum class Kind { Literal, Group, Prefix, Suffix, Entire, Last } kind;
			std::string literal;
			int32_t group{};
		};
		std::string CanonicalLiteral(std::wstring_view input)
		{
			std::wstring canonical;
			canonical.reserve(input.size());
			for (size_t i = 0; i < input.size(); ++i)
			{
				auto unit = input[i];
				if (unit == L'\\' && i + 1 < input.size())
				{
					const auto escaped = input[i + 1];
					if (escaped == L'r' || escaped == L'n' || escaped == L't')
					{
						unit = escaped == L't' ? L'\t' : escaped == L'r' ? L'\r' : L'\n';
						++i;
					}
				}
				canonical.push_back(unit);
			}
			if (canonical.empty()) return {};
			auto length = WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, canonical.data(), static_cast<int>(canonical.size()), nullptr, 0, nullptr, nullptr);
			if (!length) throw RegexFailure{Status::InvalidReplacement};
			std::string result(static_cast<size_t>(length), '\0');
			if (!WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, canonical.data(), static_cast<int>(canonical.size()), result.data(), length, nullptr, nullptr))
				throw RegexFailure{Status::InvalidReplacement};
			return result;
		}
		std::vector<Token> ParseTemplate(URegularExpression *matcher, std::wstring_view replacement, bool expand, RegexJob const &job)
		{
			std::vector<Token> tokens;
			if (!expand) return {{Token::Kind::Literal, CanonicalLiteral(replacement)}};
			UErrorCode error = U_ZERO_ERROR;
			auto groups = uregex_groupCount(matcher, &error);
			size_t literalStart = 0;
			for (size_t i = 0; i < replacement.size(); ++i)
			{
				job.Check();
				if (replacement[i] != L'$' || i + 1 == replacement.size()) continue;
				Token token{Token::Kind::Literal, {}};
				auto end = i + 2;
				auto next = replacement[i + 1];
				bool recognized = true;
				if (next == L'$') token.literal = "$";
				else if (next == L'&') token.kind = Token::Kind::Group;
				else if (next == L'`') token.kind = Token::Kind::Prefix;
				else if (next == L'\'') token.kind = Token::Kind::Suffix;
				else if (next == L'_') token.kind = Token::Kind::Entire;
				else if (next == L'+') token.kind = Token::Kind::Last;
				else
				{
					std::wstring_view name;
					if (next == L'{')
					{
						auto close = replacement.find(L'}', i + 2);
						if (close != std::wstring_view::npos) { name = replacement.substr(i + 2, close - i - 2); end = close + 1; }
					}
					else if (next >= L'0' && next <= L'9')
					{
						while (end < replacement.size() && replacement[end] >= L'0' && replacement[end] <= L'9') ++end;
						name = replacement.substr(i + 1, end - i - 1);
					}
					int32_t group = -1;
					if (!name.empty())
					{
						bool digits = true;
						int64_t number = 0;
						for (auto digit : name)
						{
							if (digit < L'0' || digit > L'9') { digits = false; break; }
							number = std::min(int64_t{INT32_MAX} + 1, number * 10 + digit - L'0');
						}
						if (digits) group = number <= groups ? static_cast<int32_t>(number) : -1;
						else if (next == L'{')
						{
							error = U_ZERO_ERROR;
							group = uregex_groupNumberFromName(matcher, reinterpret_cast<UChar const *>(name.data()), static_cast<int32_t>(name.size()), &error);
							if (U_FAILURE(error)) group = -1;
						}
					}
					recognized = group >= 0;
					token.kind = Token::Kind::Group;
					token.group = group;
				}
				if (!recognized) continue;
				if (i > literalStart) tokens.push_back({Token::Kind::Literal, CanonicalLiteral(replacement.substr(literalStart, i - literalStart))});
				tokens.push_back(std::move(token));
				i = end - 1;
				literalStart = end;
			}
			if (literalStart < replacement.size()) tokens.push_back({Token::Kind::Literal, CanonicalLiteral(replacement.substr(literalStart))});
			return tokens;
		}
		void EmitRange(SplitText source, int64_t start, int64_t end, RegexJob const &job, std::function<void(std::string_view)> const &emit)
		{
			while (start < end)
			{
				job.Check();
				const bool first = start < static_cast<int64_t>(source.first.size());
				auto span = first ? source.first : source.second;
				const auto offset = static_cast<size_t>(first ? start : start - source.first.size());
				const auto count = std::min({size_t{65536}, span.size() - offset, static_cast<size_t>(end - start)});
				emit(span.substr(offset, count));
				start += count;
			}
		}
		void Expand(URegularExpression *matcher, SplitText source, std::vector<Token> const &tokens, RegexJob const &job,
			std::function<void(std::string_view)> const &emit)
		{
			bool previousCr = false;
			std::string buffer;
			buffer.reserve(65536);
			auto canonicalEmit = [&](std::string_view bytes, bool logicalSource)
			{
				buffer.clear();
				for (char byte : bytes)
				{
					if (logicalSource && byte == '\r') byte = '\n';
					if (byte == '\n' && previousCr) { previousCr = false; continue; }
					previousCr = byte == '\r';
					buffer.push_back(byte == '\n' ? '\r' : byte);
				}
				emit(buffer);
			};
			UErrorCode error = U_ZERO_ERROR;
			for (auto const &token : tokens)
			{
				job.Check();
				if (token.kind == Token::Kind::Literal) { canonicalEmit(token.literal, false); continue; }
				int64_t start = 0, end = source.Length();
				if (token.kind == Token::Kind::Prefix) end = uregex_start64(matcher, 0, &error);
				else if (token.kind == Token::Kind::Suffix) start = uregex_end64(matcher, 0, &error);
				else if (token.kind == Token::Kind::Group || token.kind == Token::Kind::Last)
				{
					auto group = token.group;
					if (token.kind == Token::Kind::Last)
					{
						group = uregex_groupCount(matcher, &error);
						while (group > 0 && uregex_start64(matcher, group, &error) < 0) --group;
						if (group == 0 && uregex_groupCount(matcher, &error) != 0) continue;
					}
					start = uregex_start64(matcher, group, &error);
					end = uregex_end64(matcher, group, &error);
				}
				CheckIcu(error, job);
				if (start >= 0) EmitRange(source, start, end, job, [&](auto bytes) { canonicalEmit(bytes, true); });
			}
		}
	}

	Status RegexJob::Interruption() const noexcept
	{
		if (canceled.load()) return Status::Canceled;
		if (std::chrono::steady_clock::now() >= deadline) return Status::TimedOut;
		return Status::Found;
	}
	void RegexJob::Check() const
	{
		auto status = Interruption();
		if (status != Status::Found) throw RegexFailure{status};
	}
	winrt::WinUIEditor::EditorSearchResult RunRegex(SplitText source, RegexRequest const &request,
		RegexJob const &job, std::function<void(std::string_view)> const &emit)
	{
		job.Check();
		auto matcher = Compile(request, job);
		UErrorCode error = U_ZERO_ERROR;
		UText text = UTEXT_INITIALIZER;
		OpenSplitText(&text, source, &error);
		auto closeText = std::unique_ptr<UText, decltype(&utext_close)>(&text, utext_close);
		uregex_setUText(matcher.get(), &text, &error);
		uregex_setMatchCallback(matcher.get(), MatchCallback, &job, &error);
		ScanProgress progress{job};
		uregex_setFindProgressCallback(matcher.get(), FindCallback, &progress, &error);
		uregex_setTimeLimit(matcher.get(), 2000, &error);
		uregex_setStackLimit(matcher.get(), 8 * 1024 * 1024, &error);
		CheckIcu(error, job);
		auto result = StatusResult(Status::NotFound);
		std::vector<Token> tokens;
		if (request.replace) tokens = ParseTemplate(matcher.get(), request.replacement, request.replaceAll, job);
		int64_t tail = 0;
		auto enumerate = [&](int64_t origin, bool previous, bool all)
		{
			auto found = uregex_find64(matcher.get(), previous || all ? 0 : origin, &error);
			while (found && U_SUCCESS(error))
			{
				job.Check();
				auto start = uregex_start64(matcher.get(), 0, &error);
				auto end = uregex_end64(matcher.get(), 0, &error);
				CheckIcu(error, job);
				if (previous && start > origin) break;
				bool eligible = !previous || end <= origin;
				if (start == end && start == request.excludedEmpty) eligible = false;
				if (eligible)
				{
					result = {Status::Found, start, end, result.MatchCount + 1, 0, -1};
					if (all)
					{
						EmitRange(source, tail, start, job, emit);
						Expand(matcher.get(), source, tokens, job, emit);
						tail = end;
					}
					else if (!previous) break;
				}
				found = uregex_findNext(matcher.get(), &error);
			}
			CheckIcu(error, job);
		};
		enumerate(request.origin, request.previous, request.replaceAll);
		if (result.Status == Status::NotFound && request.wrap && !request.replace)
			enumerate(request.previous ? source.Length() : 0, request.previous, false);
		if (request.replace && result.Status == Status::Found)
		{
			if (request.replaceAll)
				EmitRange(source, tail, source.Length(), job, emit);
			else
			{
				// A single replacement is one literal token and emits only its text;
				// the caller edits [Start, End).
				Expand(matcher.get(), source, tokens, job, emit);
				result.MatchCount = 1;
			}
		}
		job.Check();
		if (!request.replace && result.Status == Status::Found) result.MatchCount = 1;
		return result;
	}
}
