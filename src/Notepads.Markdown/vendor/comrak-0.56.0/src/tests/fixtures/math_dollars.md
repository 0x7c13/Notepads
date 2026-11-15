---
title: Dollar Math
based_on: https://github.com/jgm/commonmark-hs/blob/master/commonmark-extensions/test/math.md
---

# TeX Math

Inline math goes between `$` characters, and display math
goes between `$$`:

```````````````````````````````` example
Let $x$ and $y$ be integers such that
$$x=y + 2$$
.
<p>Let <span data-math-style="inline">x</span> and <span data-math-style="inline">y</span> be integers such that
<span data-math-style="display">x=y + 2</span></p>
````````````````````````````````

In inline math, the opening `$` must not be followed by
a whitespace, and the closing `$` must not be
preceded by whitespace.

```````````````````````````````` example
This is not math: 2000$.
And neither is this $ 4 $.
Or this $4
$.
.
<p>This is not math: 2000$.
And neither is this $ 4 $.
Or this $4
$.</p>
````````````````````````````````

Display math delimiters can be surrounded by whitespace:

```````````````````````````````` example
This is display math:
$$
e=mc^2
$$
.
<p>This is display math:
<span data-math-style="display">
e=mc^2
</span></p>
````````````````````````````````

Note that math can contain embedded math.  In scanning
for a closing delimiter, we skip material in balanced
curly braces:

```````````````````````````````` example disabled
This is display math:
$$
\text{Hello $x^2$}
$$
And this is inline math:
$\text{Hello $x$ there!}$
.
<p>This is display math:
<span class="math display">\[
\text{Hello $x^2$}
\]</span>
And this is inline math:
<span class="math inline">\(\text{Hello $x$ there!}\)</span></p>
````````````````````````````````

To avoid treating currency signs as math delimiters,
one may occasionally have to backslash-escape them:

```````````````````````````````` example
The cost is between \$10 and 30$.
.
<p>The cost is between $10 and 30$.</p>
````````````````````````````````

Dollar signs must also be backslash-escaped if they
occur within math:

```````````````````````````````` example
$\text{\$}$
.
<p><span data-math-style="inline">\text{\$}</span></p>
````````````````````````````````

Everything inside the math construction is treated
as math, and not given its normal commonmark meaning.

```````````````````````````````` example
$b<a>c$
.
<p><span data-math-style="inline">b&lt;a&gt;c</span></p>
````````````````````````````````

Block math can directly follow a paragraph.

```````````````````````````````` example
This is inline display math
$$1+2$$

This is block math
$$
1+2
$$
.
<p>This is inline display math
<span data-math-style="display">1+2</span></p>
<p>This is block math
<span data-math-style="display">
1+2
</span></p>
````````````````````````````````
