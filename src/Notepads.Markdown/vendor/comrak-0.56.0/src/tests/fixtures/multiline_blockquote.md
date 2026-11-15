---
title: GitLab Flavored Markdown Spec
version: 0.1
date: '2023-12-18'
license: '[CC-BY-SA 4.0](http://creativecommons.org/licenses/by-sa/4.0/)'
---

## Multi-line Blockquotes

Simple container

```````````````````````````````` example
>>>
*content*
>>>
.
<blockquote>
<p><em>content</em></p>
</blockquote>
````````````````````````````````


Can contain block elements

```````````````````````````````` example
>>>
### heading

-----------
>>>
.
<blockquote>
<h3>heading</h3>
<hr />
</blockquote>
````````````````````````````````


Ending marker can be longer

```````````````````````````````` example
>>>>>>
  hello world
>>>>>>>>>>>
normal
.
<blockquote>
<p>hello world</p>
</blockquote>
<p>normal</p>
````````````````````````````````


Nested blockquotes

```````````````````````````````` example
>>>>>
>>>>
foo
>>>>
>>>>>
.
<blockquote>
<blockquote>
<p>foo</p>
</blockquote>
</blockquote>
````````````````````````````````

Incorrectly nested blockquotes

```````````````````````````````` example
>>>>
this block is closed with 5 markers below

>>>>>

auto-closed blocks
>>>>>
>>>>
.
<blockquote>
<p>this block is closed with 5 markers below</p>
</blockquote>
<p>auto-closed blocks</p>
<blockquote>
<blockquote>
</blockquote>
</blockquote>
````````````````````````````````


Marker can be indented up to 3 spaces

```````````````````````````````` example
   >>>>
   first-level blockquote
    >>>
    second-level blockquote
    >>>
   >>>>
   regular paragraph
.
<blockquote>
<p>first-level blockquote</p>
<blockquote>
<p>second-level blockquote</p>
</blockquote>
</blockquote>
<p>regular paragraph</p>
````````````````````````````````


Fours spaces makes it a code block

```````````````````````````````` example
    >>>
    content
    >>>
.
<pre><code>&gt;&gt;&gt;
content
&gt;&gt;&gt;
</code></pre>
````````````````````````````````


Detection of embedded 4 spaces code block starts in the
column the blockquote starts, not from the beginning of
the line.

```````````````````````````````` example
  >>>
      code block
  >>>
.
<blockquote>
<pre><code>code block
</code></pre>
</blockquote>
````````````````````````````````

```````````````````````````````` example
   >>>>
   content
    >>>
        code block
    >>>
   >>>>
.
<blockquote>
<p>content</p>
<blockquote>
<pre><code>code block
</code></pre>
</blockquote>
</blockquote>
````````````````````````````````

Closing marker can't have text on the same line

```````````````````````````````` example
>>>
foo
>>> arg=123
.
<blockquote>
<p>foo</p>
<blockquote>
<blockquote>
<blockquote>
<p>arg=123</p>
</blockquote>
</blockquote>
</blockquote>
</blockquote>
````````````````````````````````


Blockquotes self-close at the end of the document

```````````````````````````````` example
>>>
foo
.
<blockquote>
<p>foo</p>
</blockquote>
````````````````````````````````


They should terminate paragraphs

```````````````````````````````` example
blah blah
>>>
content
>>>
.
<p>blah blah</p>
<blockquote>
<p>content</p>
</blockquote>
````````````````````````````````


They can be nested in lists

```````````````````````````````` example
 -  >>>
    - foo
    >>>
.
<ul>
<li>
<blockquote>
<ul>
<li>foo</li>
</ul>
</blockquote>
</li>
</ul>
````````````````````````````````


Or in blockquotes

```````````````````````````````` example
> >>>
> foo
>> bar
> baz
> >>>
.
<blockquote>
<blockquote>
<p>foo</p>
<blockquote>
<p>bar
baz</p>
</blockquote>
</blockquote>
</blockquote>
````````````````````````````````


List indentation

```````````````````````````````` example
 -  >>>
    foo
    bar
    >>>

 -  >>>
    foo
    bar
    >>>
.
<ul>
<li>
<blockquote>
<p>foo
bar</p>
</blockquote>
</li>
<li>
<blockquote>
<p>foo
bar</p>
</blockquote>
</li>
</ul>
````````````````````````````````


Ignored inside code blocks:

```````````````````````````````` example
```txt
# Code
>>>
# Code
>>>
# Code
```
.
<pre><code class="language-txt"># Code
&gt;&gt;&gt;
# Code
&gt;&gt;&gt;
# Code
</code></pre>
````````````````````````````````


Does not require a leading or trailing blank line

```````````````````````````````` example
Some text
>>>
A quote
>>>
Some other text
.
<p>Some text</p>
<blockquote>
<p>A quote</p>
</blockquote>
<p>Some other text</p>
````````````````````````````````
