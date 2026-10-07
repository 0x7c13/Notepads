// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

#include "NativeDiff.h"
#include <algorithm>
#include <cstring>
#include <limits>
#include <numeric>
#include <string>
#include "UniConversion.h"

namespace WinUIEditor
{
	namespace
	{
		using Scintilla::Internal::UTF8IsTrailByte;
		struct WorkExhausted { DiffReason reason{DiffReason::WorkBudget}; };
		struct Line { uint32_t start{}, length{}; uint64_t hash{}; };
		struct Match { uint32_t oldLine{}, newLine{}; };
		class Comparison
		{
			DiffText _old, _new;
			DiffJob const &_job;
			DiffOptions const &_options;
			uint64_t _work{};
			std::vector<Line> _oldLines, _newLines;
			DiffResult _result;
			void CheckCancellation() const
			{
				if (_job.canceled.load(std::memory_order_relaxed)) throw DiffCanceled{};
			}
			void Charge(uint64_t amount = 1)
			{
				CheckCancellation();
				_work += amount;
				if (std::chrono::steady_clock::now() >= _job.deadline) throw WorkExhausted{DiffReason::TimeBudget};
				if (_work > _options.maximumWork) throw WorkExhausted{};
			}
			void CheckLinearScan() const
			{
				CheckCancellation();
				if (std::chrono::steady_clock::now() >= _job.deadline) throw DiffUnavailable{DiffReason::TimeBudget};
			}
			bool WholeTextEqual() const
			{
				if (_old.Length() != _new.Length()) return false;
				// This bounded linear proof must not become a coarse change merely
				// because the separate line-refinement work budget is exhausted.
				for (size_t position = 0; position < _old.Length();)
				{
					CheckLinearScan();
					const auto oldSpan = _old.SpanFrom(position), newSpan = _new.SpanFrom(position);
					const auto count = std::min({size_t{65536}, oldSpan.size(), newSpan.size()});
					if (std::memcmp(oldSpan.data(), newSpan.data(), count) != 0) return false;
					position += count;
				}
				CheckLinearScan();
				return true;
			}
			std::vector<Line> Index(DiffText text, uint32_t allowance)
			{
				if (text.Length() > INT32_MAX || allowance == 0) throw DiffUnavailable{};
				std::vector<Line> lines;
				uint32_t start = 0;
				uint64_t hash = 14695981039346656037ull;
				for (uint32_t position = 0; position < text.Length(); ++position)
				{
					if ((position & 65535) == 0)
						CheckLinearScan();
					const auto value = text.At(position);
					hash = (hash ^ value) * 1099511628211ull;
					if (value == '\r')
					{
						if (lines.size() + 1 >= allowance) throw DiffUnavailable{};
						lines.push_back({start, position + 1 - start, hash & _options.hashMask});
						start = position + 1;
						hash = 14695981039346656037ull;
					}
				}
				lines.push_back({start, static_cast<uint32_t>(text.Length()) - start, hash & _options.hashMask});
				return lines;
			}
			bool Equal(uint32_t oldLine, uint32_t newLine)
			{
				Charge();
				const auto &left = _oldLines[oldLine];
				const auto &right = _newLines[newLine];
				if (left.length != right.length || left.hash != right.hash) return false;
				for (uint32_t offset = 0; offset < left.length;)
				{
					const auto oldPosition = left.start + offset;
					const auto newPosition = right.start + offset;
					const auto oldSpan = _old.SpanFrom(oldPosition);
					const auto newSpan = _new.SpanFrom(newPosition);
					const auto count = std::min({size_t{65536}, size_t{left.length - offset}, oldSpan.size(), newSpan.size()});
					Charge(count);
					if (std::memcmp(oldSpan.data(), newSpan.data(), count) != 0) return false;
					offset += static_cast<uint32_t>(count);
				}
				return true;
			}
			void Add(uint32_t oldStart, uint32_t oldEnd, uint32_t newStart, uint32_t newEnd)
			{
				if (oldStart == oldEnd && newStart == newEnd) return;
				if (!_result.hunks.empty())
				{
					auto &last = _result.hunks.back();
					if (last.oldStart + last.oldCount == oldStart && last.newStart + last.newCount == newStart)
					{
						last.oldCount += oldEnd - oldStart;
						last.newCount += newEnd - newStart;
						return;
					}
				}
				if (_result.hunks.size() >= _options.maximumHunks) throw WorkExhausted{};
				_result.hunks.push_back({oldStart, oldEnd - oldStart, newStart, newEnd - newStart});
			}
			std::vector<Match> Anchors(uint32_t oldBegin, uint32_t oldEnd, uint32_t newBegin, uint32_t newEnd)
			{
				std::vector<uint32_t> left(oldEnd - oldBegin), right(newEnd - newBegin);
				std::iota(left.begin(), left.end(), oldBegin);
				std::iota(right.begin(), right.end(), newBegin);
				auto sort = [&](auto &indices, auto const &lines)
				{
					std::sort(indices.begin(), indices.end(), [&](auto a, auto b)
					{
						Charge();
						return lines[a].hash < lines[b].hash;
					});
				};
				sort(left, _oldLines);
				sort(right, _newLines);
				std::vector<Match> matches;
				for (size_t a = 0, b = 0; a < left.size() && b < right.size();)
				{
					Charge();
					const auto ah = _oldLines[left[a]].hash, bh = _newLines[right[b]].hash;
					if (ah < bh) { ++a; continue; }
					if (bh < ah) { ++b; continue; }
					const auto aStart = a++, bStart = b++;
					while (a < left.size() && _oldLines[left[a]].hash == ah) { Charge(); ++a; }
					while (b < right.size() && _newLines[right[b]].hash == bh) { Charge(); ++b; }
					if (a == aStart + 1 && b == bStart + 1 && Equal(left[aStart], right[bStart]))
						matches.push_back({left[aStart], right[bStart]});
				}
				std::sort(matches.begin(), matches.end(), [&](auto a, auto b) { Charge(); return a.oldLine < b.oldLine; });
				std::vector<size_t> tails, previous(matches.size(), SIZE_MAX);
				for (size_t i = 0; i < matches.size(); ++i)
				{
					Charge();
					auto found = std::lower_bound(tails.begin(), tails.end(), matches[i].newLine,
						[&](size_t index, uint32_t line) { Charge(); return matches[index].newLine < line; });
					if (found != tails.begin()) previous[i] = *(found - 1);
					if (found == tails.end()) tails.push_back(i); else *found = i;
				}
				std::vector<Match> ordered;
				for (auto index = tails.empty() ? SIZE_MAX : tails.back(); index != SIZE_MAX; index = previous[index])
				{
					Charge();
					ordered.push_back(matches[index]);
				}
				std::reverse(ordered.begin(), ordered.end());
				return ordered;
			}
			void Refine(uint32_t oldStart, uint32_t oldEnd, uint32_t newStart, uint32_t newEnd)
			{
				Charge();
				if (oldStart == oldEnd || newStart == newEnd) { Add(oldStart, oldEnd, newStart, newEnd); return; }
				const auto n = static_cast<int>(oldEnd - oldStart), m = static_cast<int>(newEnd - newStart);
				// Bounds the O(D^2) trace even when options allow more edits.
				constexpr uint32_t MaximumTraceEdits = 256;
				const auto limit = static_cast<int>(std::min(_options.maximumEdits, MaximumTraceEdits));
				if (std::abs(n - m) > limit)
				{
					_result.coarse = true;
					_result.reason = DiffReason::RefinementBudget;
					Add(oldStart, oldEnd, newStart, newEnd);
					return;
				}
				const int origin = limit + 1;
				std::vector<int> frontier(static_cast<size_t>(2 * limit + 3), 0);
				std::vector<std::vector<int>> trace;
				for (int distance = 0; distance <= limit; ++distance)
				{
					Charge();
					trace.push_back(frontier);
					for (int diagonal = -distance; diagonal <= distance; diagonal += 2)
					{
						Charge();
						int x = diagonal == -distance || (diagonal != distance && frontier[origin + diagonal - 1] < frontier[origin + diagonal + 1])
							? frontier[origin + diagonal + 1] : frontier[origin + diagonal - 1] + 1;
						int y = x - diagonal;
						while (x < n && y < m && Equal(oldStart + x, newStart + y)) { ++x; ++y; }
						frontier[origin + diagonal] = x;
						if (x < n || y < m) continue;
						std::vector<Match> matches;
						for (int d = distance; d > 0; --d)
						{
							Charge();
							const auto &v = trace[d];
							const int k = x - y;
							const int previousK = k == -d || (k != d && v[origin + k - 1] < v[origin + k + 1]) ? k + 1 : k - 1;
							const int previousX = v[origin + previousK], previousY = previousX - previousK;
							while (x > previousX && y > previousY) { Charge(); matches.push_back({oldStart + --x, newStart + --y}); }
							x = previousX;
							y = previousY;
						}
						while (x > 0 && y > 0) { Charge(); matches.push_back({oldStart + --x, newStart + --y}); }
						std::reverse(matches.begin(), matches.end());
						for (auto const match : matches)
						{
							Add(oldStart, match.oldLine, newStart, match.newLine);
							oldStart = match.oldLine + 1;
							newStart = match.newLine + 1;
						}
						Add(oldStart, oldEnd, newStart, newEnd);
						return;
					}
				}
				_result.coarse = true;
				_result.reason = DiffReason::RefinementBudget;
				Add(oldStart, oldEnd, newStart, newEnd);
			}
			void Inline()
			{
				for (auto const &hunk : _result.hunks)
				{
					for (uint32_t i = 0; i < std::min(hunk.oldCount, hunk.newCount); ++i)
					{
						if (_result.inlineRanges.size() >= 2048) return;
						const auto &a = _oldLines[hunk.oldStart + i], &b = _newLines[hunk.newStart + i];
						if (a.length > 16384 || b.length > 16384) continue;
						uint32_t prefix = 0, suffix = 0;
						while (prefix < std::min(a.length, b.length) && _old.At(a.start + prefix) == _new.At(b.start + prefix)) { Charge(); ++prefix; }
						while (prefix && ((prefix < a.length && UTF8IsTrailByte(_old.At(a.start + prefix))) ||
							(prefix < b.length && UTF8IsTrailByte(_new.At(b.start + prefix))))) --prefix;
						while (suffix < std::min(a.length, b.length) - prefix && _old.At(a.start + a.length - 1 - suffix) == _new.At(b.start + b.length - 1 - suffix)) { Charge(); ++suffix; }
						while (suffix && (UTF8IsTrailByte(_old.At(a.start + a.length - suffix)) || UTF8IsTrailByte(_new.At(b.start + b.length - suffix)))) --suffix;
						if (prefix + suffix == a.length && prefix + suffix == b.length) continue;
						_result.inlineRanges.push_back({a.start + prefix, a.length - prefix - suffix, b.start + prefix, b.length - prefix - suffix});
					}
				}
			}
			void CountChangedLines()
			{
				for (auto const &hunk : _result.hunks)
				{
					CheckCancellation();
					const bool oldTerminal = hunk.oldCount && hunk.oldStart + hunk.oldCount == _oldLines.size() && !_oldLines.back().length;
					const bool newTerminal = hunk.newCount && hunk.newStart + hunk.newCount == _newLines.size() && !_newLines.back().length;
					_result.deletedLines += hunk.oldCount - static_cast<uint32_t>(oldTerminal);
					_result.addedLines += hunk.newCount - static_cast<uint32_t>(newTerminal);
				}
			}
		public:
			Comparison(DiffText oldText, DiffText newText, DiffJob const &job, DiffOptions const &options)
				: _old(oldText), _new(newText), _job(job), _options(options) {}
			DiffResult Run()
			{
				if (_options.maximumHunks == 0 || _options.maximumLines < 2) throw DiffUnavailable{};
				_oldLines = Index(_old, _options.maximumLines - 1);
				_newLines = Index(_new, _options.maximumLines - static_cast<uint32_t>(_oldLines.size()));
				_result.oldLines = static_cast<uint32_t>(_oldLines.size());
				_result.newLines = static_cast<uint32_t>(_newLines.size());
				if (WholeTextEqual()) return std::move(_result);
				uint32_t prefix = 0, oldEnd = _result.oldLines, newEnd = _result.newLines;
				try
				{
					while (prefix < std::min(oldEnd, newEnd) && Equal(prefix, prefix)) ++prefix;
					while (oldEnd > prefix && newEnd > prefix && Equal(oldEnd - 1, newEnd - 1)) { --oldEnd; --newEnd; }
					uint32_t oldStart = prefix, newStart = prefix;
					for (auto const &anchor : Anchors(prefix, oldEnd, prefix, newEnd))
					{
						Refine(oldStart, anchor.oldLine, newStart, anchor.newLine);
						oldStart = anchor.oldLine + 1;
						newStart = anchor.newLine + 1;
					}
					Refine(oldStart, oldEnd, newStart, newEnd);
				}
				catch (WorkExhausted const &failure)
				{
					_result.hunks.clear();
					_result.coarse = true;
					_result.reason = failure.reason;
					Add(prefix, oldEnd, prefix, newEnd);
				}
				try { Inline(); }
				catch (WorkExhausted const &) { /* Optional inline refinement never changes line coverage. */ }
				CountChangedLines();
				CheckCancellation();
				return std::move(_result);
			}
		};
	}
	DiffResult CompareText(DiffText oldText, DiffText newText, DiffJob const &job, DiffOptions const &options)
	{
		return Comparison(oldText, newText, job, options).Run();
	}
}
