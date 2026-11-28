---
title: GitLab Flavored Markdown Spec
version: 0.1
date: '2023-12-18'
license: '[CC-BY-SA 4.0](http://creativecommons.org/licenses/by-sa/4.0/)'
---

## Multi-line Alerts

Simple container

```````````````````````````````` example
>>> [!NOTE]
*content*
>>>
.
<div class="markdown-alert markdown-alert-note">
<p class="markdown-alert-title">Note</p>
<p><em>content</em></p>
</div>
````````````````````````````````

Other kinds of alerts:

```````````````````````````````` example
>>> [!TIP]
Optional information to help a user be more successful.
>>>
.
<div class="markdown-alert markdown-alert-tip">
<p class="markdown-alert-title">Tip</p>
<p>Optional information to help a user be more successful.</p>
</div>
````````````````````````````````

```````````````````````````````` example
>>> [!IMPORTANT]
Crucial information necessary for users to succeed.
>>>
.
<div class="markdown-alert markdown-alert-important">
<p class="markdown-alert-title">Important</p>
<p>Crucial information necessary for users to succeed.</p>
</div>
````````````````````````````````

```````````````````````````````` example
>>> [!WARNING]
Critical content demanding immediate user attention due to potential risks.
>>>
.
<div class="markdown-alert markdown-alert-warning">
<p class="markdown-alert-title">Warning</p>
<p>Critical content demanding immediate user attention due to potential risks.</p>
</div>
````````````````````````````````

```````````````````````````````` example
>>> [!CAUTION]
Negative potential consequences of an action.
>>>
.
<div class="markdown-alert markdown-alert-caution">
<p class="markdown-alert-title">Caution</p>
<p>Negative potential consequences of an action.</p>
</div>
````````````````````````````````

A title can be specified to override the default title:

```````````````````````````````` example
>>> [!NOTE] Pay attention
Highlights information that users should take into account, even when skimming.
>>>
.
<div class="markdown-alert markdown-alert-note">
<p class="markdown-alert-title">Pay attention</p>
<p>Highlights information that users should take into account, even when skimming.</p>
</div>
````````````````````````````````

Can contain block elements

```````````````````````````````` example
>>> [!NOTE]
### heading

-----------
>>>
.
<div class="markdown-alert markdown-alert-note">
<p class="markdown-alert-title">Note</p>
<h3>heading</h3>
<hr />
</div>
````````````````````````````````


Ending marker can be longer

```````````````````````````````` example
>>>>>> [!NOTE]
  hello world
>>>>>>>>>>>
normal
.
<div class="markdown-alert markdown-alert-note">
<p class="markdown-alert-title">Note</p>
<p>hello world</p>
</div>
<p>normal</p>
````````````````````````````````


Nested alerts

```````````````````````````````` example
>>>>> [!NOTE]
>>>> [!CAUTION]
foo
>>>>
>>>>>
.
<div class="markdown-alert markdown-alert-note">
<p class="markdown-alert-title">Note</p>
<div class="markdown-alert markdown-alert-caution">
<p class="markdown-alert-title">Caution</p>
<p>foo</p>
</div>
</div>
````````````````````````````````

Incorrectly nested alerts

```````````````````````````````` example
>>>> [!NOTE]
this block is closed with 5 markers below

>>>>>

auto-closed blocks
>>>>>
>>>>
.
<div class="markdown-alert markdown-alert-note">
<p class="markdown-alert-title">Note</p>
<p>this block is closed with 5 markers below</p>
</div>
<p>auto-closed blocks</p>
<blockquote>
<blockquote>
</blockquote>
</blockquote>
````````````````````````````````


Marker can be indented up to 3 spaces

```````````````````````````````` example
   >>>> [!NOTE]
   first-level blockquote
    >>> [!CAUTION]
    second-level blockquote
    >>>
   >>>>
   regular paragraph
.
<div class="markdown-alert markdown-alert-note">
<p class="markdown-alert-title">Note</p>
<p>first-level blockquote</p>
<div class="markdown-alert markdown-alert-caution">
<p class="markdown-alert-title">Caution</p>
<p>second-level blockquote</p>
</div>
</div>
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
column the alert starts, not from the beginning of
the line.

```````````````````````````````` example
  >>> [!NOTE]
      code block
  >>>
.
<div class="markdown-alert markdown-alert-note">
<p class="markdown-alert-title">Note</p>
<pre><code>code block
</code></pre>
</div>
````````````````````````````````

```````````````````````````````` example
   >>>> [!NOTE]
   content
    >>> [!CAUTION]
        code block
    >>>
   >>>>
.
<div class="markdown-alert markdown-alert-note">
<p class="markdown-alert-title">Note</p>
<p>content</p>
<div class="markdown-alert markdown-alert-caution">
<p class="markdown-alert-title">Caution</p>
<pre><code>code block
</code></pre>
</div>
</div>
````````````````````````````````

Closing marker can't have text on the same line

```````````````````````````````` example
>>> [!NOTE]
foo
>>> arg=123
.
<div class="markdown-alert markdown-alert-note">
<p class="markdown-alert-title">Note</p>
<p>foo</p>
<blockquote>
<blockquote>
<blockquote>
<p>arg=123</p>
</blockquote>
</blockquote>
</blockquote>
</div>
````````````````````````````````


Alerts self-close at the end of the document

```````````````````````````````` example
>>> [!NOTE]
foo
.
<div class="markdown-alert markdown-alert-note">
<p class="markdown-alert-title">Note</p>
<p>foo</p>
</div>
````````````````````````````````


They should terminate paragraphs

```````````````````````````````` example
blah blah
>>> [!NOTE]
content
>>>
.
<p>blah blah</p>
<div class="markdown-alert markdown-alert-note">
<p class="markdown-alert-title">Note</p>
<p>content</p>
</div>
````````````````````````````````


They can be nested in lists

```````````````````````````````` example
 -  >>> [!NOTE]
    - foo
    >>>
.
<ul>
<li>
<div class="markdown-alert markdown-alert-note">
<p class="markdown-alert-title">Note</p>
<ul>
<li>foo</li>
</ul>
</div>
</li>
</ul>
````````````````````````````````


Or in blockquotes

```````````````````````````````` example
> >>> [!NOTE]
> foo
>> bar
> baz
> >>>
.
<blockquote>
<div class="markdown-alert markdown-alert-note">
<p class="markdown-alert-title">Note</p>
<p>foo</p>
<blockquote>
<p>bar
baz</p>
</blockquote>
</div>
</blockquote>
````````````````````````````````


List indentation

```````````````````````````````` example
 -  >>> [!NOTE]
    foo
    bar
    >>>

 -  >>> [!NOTE]
    foo
    bar
    >>>
.
<ul>
<li>
<div class="markdown-alert markdown-alert-note">
<p class="markdown-alert-title">Note</p>
<p>foo
bar</p>
</div>
</li>
<li>
<div class="markdown-alert markdown-alert-note">
<p class="markdown-alert-title">Note</p>
<p>foo
bar</p>
</div>
</li>
</ul>
````````````````````````````````


Ignored inside code blocks:

```````````````````````````````` example
```txt
# Code
>>> [!NOTE]
# Code
>>>
# Code
```
.
<pre><code class="language-txt"># Code
&gt;&gt;&gt; [!NOTE]
# Code
&gt;&gt;&gt;
# Code
</code></pre>
````````````````````````````````


Does not require a leading or trailing blank line

```````````````````````````````` example
Some text
>>> [!NOTE]
A quote
>>>
Some other text
.
<p>Some text</p>
<div class="markdown-alert markdown-alert-note">
<p class="markdown-alert-title">Note</p>
<p>A quote</p>
</div>
<p>Some other text</p>
````````````````````````````````
