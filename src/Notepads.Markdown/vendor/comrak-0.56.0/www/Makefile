all:
	(cat header.html; comrak index.md --header-ids "" --smart -e footnotes; cat footer.html) > index.html

deploy: all
	mc cp * kala/comrak.ee/
	mc cp --attr 'content-type=text/plain' Makefile kala/comrak.ee/
