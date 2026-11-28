# Optimization/Design notes

This is my scratch pad for optimization ideas. Some of this I will implement, some I have implemented, some are just speculative.

# Scopes

## Representation ideas:

- Normal arrays of strings
- array of 32-bit or 64-bit atoms (maybe using Servo's atom library)
- Atoms packed into one or two u64s
  - fast equality checking
  - potentially fast prefix checking
  - needs unsafe code

## Potential packings:

- variable width atoms, either 7 bits and a tag bit for top 128 or 13 bits and 3 tagging bits for rest
  - can fit all but 33 of the scopes present
- tagged pointer (taking advantage of alignment), either a pointer to a slow path, or the first 4 bits set then a packed representation, one of others mentioned
- 6 10-bit atoms referencing unique things by position (see by-position stats below)
- 5 11-bit atoms and one 8-bit one for the first atom (2^11 = 2048, 2^8 = 256), one remaining bit for tag marker

## Stats:

- 7000 scopes referenced in sublime, 3537 unique ones, all stats after this are based on non-unique data
- all but 33 scopes in default packages could fit in 64 with combination 8bit or 16bit atom encoding
- there are only 1219 unique atoms in the default package set
- the top 128 atoms make up ~90% of all unique atoms referenced in syntax files
- there are 26 unique first atoms, 145 unique last atoms
- every position (1st atom, 2nd atom, ...) has under 878 possibilities, only 2nd,3rd and 4th have >256
- 99.8% of scopes have 6 or fewer atoms, 97% have 5 or fewer, 70% have 4 or fewer
  - for unique scopes: {2=>81, 4=>1752, 3=>621, 5=>935, 7=>8, 6=>140} ----> 95% of uniques <= 6
  - for non-unique scopes: {2=>125, 4=>3383, 3=>1505, 5=>1891, 7=>9, 6=>202}

# Checking prefix

operation: `fn extent_matched(potential_prefix: Scope, s: Scope) -> u8`
idea: any differences are beyond the length of the prefix.
figure this out by xor and then ctz/clz then a compare to the length (however that works).

```bash
XXXXYYYY00000000 # prefix
XXXXYYYYZZZZ0000 # testee
00000000ZZZZ0000 # = xored

XXXXYYYYQQQQ0000 # non-prefix
XXXXYYYYZZZZ0000 # testee
00000000GGGG0000 # = xored

XXXXQQQQ00000000 # non-prefix
XXXXYYYYZZZZ0000 # testee
0000BBBBZZZZ0000 # = xored
```

# Parsing

* Problem: need to reduce number of regex search calls
* Solution: cache better

## Stats

```bash
# On stats branch
$cargo run --release --example syncat testdata/jquery.js | grep cmiss | wc -l
     Running `target/release/examples/syncat testdata/jquery.js`
   61266
$cargo run --release --example syncat testdata/jquery.js | grep ptoken | wc -l
   Compiling syntect v0.1.0 (file:///Users/tristan/Box/Dev/Projects/syntect)
     Running `target/release/examples/syncat testdata/jquery.js`
   98714
$wc -l testdata/jquery.js
    9210 testdata/jquery.js
$cargo run --release --example syncat testdata/jquery.js | grep cclear | wc -l
   Compiling syntect v0.1.0 (file:///Users/tristan/Box/Dev/Projects/syntect)
     Running `target/release/examples/syncat testdata/jquery.js`
   71302
$cargo run --release --example syncat testdata/jquery.js | grep freshcachetoken | wc -l
    Compiling syntect v0.1.0 (file:///Users/tristan/Box/Dev/Projects/syntect)
      Running `target/release/examples/syncat testdata/jquery.js`
   80512
# On stats-2 branch
$cargo run --example syncat testdata/jquery.js | grep cachehit | wc -l
     Running `target/debug/examples/syncat testdata/jquery.js`
  527774
$cargo run --example syncat testdata/jquery.js | grep regsearch | wc -l
     Running `target/debug/examples/syncat testdata/jquery.js`
 2862948
$cargo run --example syncat testdata/jquery.js | grep regmatch | wc -l
   Compiling syntect v0.6.0 (file:///Users/tristan/Box/Dev/Projects/syntect)
     Running `target/debug/examples/syncat testdata/jquery.js`
  296127
$cargo run --example syncat testdata/jquery.js | grep leastmatch | wc -l
   Compiling syntect v0.6.0 (file:///Users/tristan/Box/Dev/Projects/syntect)
     Running `target/debug/examples/syncat testdata/jquery.js`
  137842
# With search caching
$cargo run --example syncat testdata/jquery.js | grep searchcached | wc -l
   Compiling syntect v0.6.0 (file:///Users/tristan/Box/Dev/Projects/syntect)
     Running `target/debug/examples/syncat testdata/jquery.js`
 2440527
$cargo run --example syncat testdata/jquery.js | grep regsearch | wc -l
     Running `target/debug/examples/syncat testdata/jquery.js`
  950195
```

Average unique regexes per line is 87.58, average non-unique is regsearch/lines = 317

Ideally we should have only a couple fresh cache searches per line, not `~10` like the stats show (freshcachetoken/linecount).

In a fantabulous world these stats mean a possible 10x speed improvement, but since caching does have a cost and we can't always cache it likely will be nice but not that high.

## Issues

- Stack transitions always bust cache, even when for example JS just pushes another group
- Doesn't cache actual matches, only if it matched or not

## Attacks

- cache based on actual context, only search if it is a prototype we haven't searched before
  - hash maps based on casting RC ref to pointer and hashing? (there is a Hash impl for pointers)
- for new searches, store matched regexes for context in BTreeMap like textmate
  - for subsequent tokens in same context, just pop off btreemap and re-search if before curpos
- cache per Regex
