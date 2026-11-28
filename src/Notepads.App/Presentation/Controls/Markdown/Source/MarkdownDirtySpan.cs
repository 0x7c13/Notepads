// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;

namespace Notepads.Presentation.Controls.Markdown;

/// <summary>One conservative replacement from an accepted source to sequentially edited text.</summary>
internal sealed class MarkdownDirtySpan
{
    private long _start;
    private long _oldEnd;
    private long _newEnd;

    internal bool HasChanges { get; private set; }
    internal long Start => _start;
    internal long DeleteLength => _oldEnd - _start;
    internal long ReplacementLength => _newEnd - _start;

    internal void Add(long position, long deleteLength, long insertLength)
    {
        ArgumentOutOfRangeException.ThrowIfNegative(position);
        ArgumentOutOfRangeException.ThrowIfNegative(deleteLength);
        ArgumentOutOfRangeException.ThrowIfNegative(insertLength);
        checked
        {
            var end = position + deleteLength;
            var insertedEnd = position + insertLength;
            if (!HasChanges)
            {
                _start = position;
                _oldEnd = end;
                _newEnd = insertedEnd;
                HasChanges = true;
                return;
            }

            // Calculate from the same prior endpoints before publishing any state.
            var start = Math.Min(_start, position);
            var oldEnd = Math.Max(_oldEnd, end + (_oldEnd - _newEnd));
            var newEnd = Math.Max(_newEnd, end) - deleteLength + insertLength;
            if (oldEnd < start || newEnd < start)
                throw new ArgumentOutOfRangeException(nameof(position));
            _start = start;
            _oldEnd = oldEnd;
            _newEnd = newEnd;
        }
    }

    internal void Clear()
    {
        HasChanges = false;
        _start = _oldEnd = _newEnd = 0;
    }
}
