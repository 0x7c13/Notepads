---
title: Description / definition lists
based_on: https://github.com/jgm/commonmark-hs/blob/master/commonmark-extensions/test/definition_lists.md
---

## Definition lists

The term is given on a line by itself, followed by
one or more definitions. Each definition must begin
with `:` (after 0-2 spaces); subsequent lines must
be indented unless they are lazy paragraph
continuations.

The list is tight if there is no blank line between
the term and the first definition, otherwise loose.

```````````````````````````````` example
apple
:   red fruit

orange
:   orange fruit
.
<dl>
<dt>apple</dt>
<dd>red fruit</dd>
<dt>orange</dt>
<dd>orange fruit</dd>
</dl>
````````````````````````````````

Loose:

```````````````````````````````` example
apple

:   red fruit

orange

:   orange fruit
.
<dl>
<dt>apple</dt>
<dd>
<p>red fruit</p>
</dd>
<dt>orange</dt>
<dd>
<p>orange fruit</p>
</dd>
</dl>
````````````````````````````````

Indented marker:

```````````````````````````````` example
apple
  : red fruit

orange
  : orange fruit
.
<dl>
<dt>apple</dt>
<dd>red fruit</dd>
<dt>orange</dt>
<dd>orange fruit</dd>
</dl>
````````````````````````````````

```````````````````````````````` example
apple

 : red fruit

orange

 : orange fruit
.
<dl>
<dt>apple</dt>
<dd>
<p>red fruit</p>
</dd>
<dt>orange</dt>
<dd>
<p>orange fruit</p>
</dd>
</dl>
````````````````````````````````

Multiple blocks in a definition:

```````````````````````````````` example
*apple*

:   red fruit

    contains seeds,
    crisp, pleasant to taste

*orange*

:   orange fruit

        { orange code block }

    > orange block quote
.
<dl>
<dt><em>apple</em></dt>
<dd>
<p>red fruit</p>
<p>contains seeds,
crisp, pleasant to taste</p>
</dd>
<dt><em>orange</em></dt>
<dd>
<p>orange fruit</p>
<pre><code>{ orange code block }
</code></pre>
<blockquote>
<p>orange block quote</p>
</blockquote>
</dd>
</dl>
````````````````````````````````

Nested lists:

```````````````````````````````` example
term

:   1. Para one

       Para two
.
<dl>
<dt>term</dt>
<dd>
<ol>
<li>
<p>Para one</p>
<p>Para two</p>
</li>
</ol>
</dd>
</dl>
````````````````````````````````

Multiple definitions, tight:

```````````````````````````````` example
apple
:   red fruit
:   computer company

orange
:   orange fruit
:   telecom company
.
<dl>
<dt>apple</dt>
<dd>red fruit</dd>
<dd>computer company</dd>
<dt>orange</dt>
<dd>orange fruit</dd>
<dd>telecom company</dd>
</dl>
````````````````````````````````

Multiple definitions, loose:

```````````````````````````````` example
apple

:   red fruit

:   computer company

orange

:   orange fruit
:   telecom company
.
<dl>
<dt>apple</dt>
<dd>
<p>red fruit</p>
</dd>
<dd>
<p>computer company</p>
</dd>
<dt>orange</dt>
<dd>
<p>orange fruit</p>
</dd>
<dd>
<p>telecom company</p>
</dd>
</dl>
````````````````````````````````

Lazy line continuations:

```````````````````````````````` example
apple

:   red fruit

:   computer
company

orange

:   orange
fruit
:   telecom company
.
<dl>
<dt>apple</dt>
<dd>
<p>red fruit</p>
</dd>
<dd>
<p>computer
company</p>
</dd>
<dt>orange</dt>
<dd>
<p>orange
fruit</p>
</dd>
<dd>
<p>telecom company</p>
</dd>
</dl>
````````````````````````````````



`~` may be used as a marker instead of `:`:

```````````````````````````````` example
apple
  ~ red fruit

orange
  ~ orange fruit
.
<dl>
<dt>apple</dt>
<dd>red fruit</dd>
<dt>orange</dt>
<dd>orange fruit</dd>
</dl>
````````````````````````````````

Definition terms may span multiple lines:

```````````````````````````````` example
a
b\
c

:   foo
.
<dl>
<dt>a
b<br />
c</dt>
<dd>
<p>foo</p>
</dd>
</dl>
````````````````````````````````

Definition list with preceding paragraph
(<https://github.com/jgm/commonmark-hs/issues/35>):

```````````````````````````````` example
Foo

bar
:   baz

bim
:   bor
.
<p>Foo</p>
<dl>
<dt>bar</dt>
<dd>baz</dd>
<dt>bim</dt>
<dd>bor</dd>
</dl>
````````````````````````````````
