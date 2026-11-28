---
title: Wikilinks
based_on: https://github.com/jgm/commonmark-hs/blob/master/commonmark-extensions/test/wikilinks_title_after_pipe.md
---

# Wikilinks, title after pipe

Wikilinks can have one of the following forms:

    [[https://example.org]]
    [[https://example.org|title]]
    [[name of page]]
    [[name of page|title]]

With this version of wikilinks, the title comes after the pipe.

```````````````````````````````` example
[[https://example.org]]
.
<p><a href="https://example.org" data-wikilink="true">https://example.org</a></p>
````````````````````````````````

```````````````````````````````` example
[[https://example.org|title]]
.
<p><a href="https://example.org" data-wikilink="true">title</a></p>
````````````````````````````````

```````````````````````````````` example
[[Name of page]]
.
<p><a href="Name%20of%20page" data-wikilink="true">Name of page</a></p>
````````````````````````````````

```````````````````````````````` example
[[Name of page|Title]]
.
<p><a href="Name%20of%20page" data-wikilink="true">Title</a></p>
````````````````````````````````

HTML entities are recognized both in the name of page and in the link title.

```````````````````````````````` example
[[Gesch&uuml;tztes Leerzeichen|&#xDC;ber &amp;nbsp;]]
.
<p><a href="Gesch%C3%BCtztes%20Leerzeichen" data-wikilink="true">Über &amp;nbsp;</a></p>
````````````````````````````````

Escaping characters is supported

```````````````````````````````` example
[[https://example.org|foo\[\]bar]]
.
<p><a href="https://example.org" data-wikilink="true">foo[]bar</a></p>
````````````````````````````````

```````````````````````````````` example
[[Name \[of\] page]]
.
<p><a href="Name%20%5Bof%5D%20page" data-wikilink="true">Name [of] page</a></p>
````````````````````````````````

Emphasis or other inline markdown is not supported

```````````````````````````````` example
[[Name _of_ page]]
.
<p><a href="Name%20_of_%20page" data-wikilink="true">Name _of_ page</a></p>
````````````````````````````````
