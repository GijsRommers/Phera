# Tests copied from php-src

These files come from the PHP source tree (origin/PHP-8.5.12) and are covered by the
PHP License in `LICENSE`. `tests/phpt/selected.txt` lists the ones
`make phpt` runs. Do not edit them; `tests/phpt/sweep.sh` copies them
again and adds every test that newly passes.

Last sweep: real PHP and Phera on every test of each directory.
"Usable" is what real PHP passes under our runner; the rest needs a
section the runner does not implement or an extension that is not loaded.

| Directory | Tests | Usable | Phera passes | Share of usable |
| --- | ---: | ---: | ---: | ---: |
| `Zend/tests` | 5396 | 4981 | 1699 | 34% |
| `tests` | 895 | 648 | 353 | 54% |
| `ext/standard/tests` | 3880 | 2847 | 1129 | 39% |
| `ext/json/tests` | 94 | 78 | 59 | 75% |
| `ext/pcre/tests` | 174 | 143 | 125 | 87% |
| `ext/ctype/tests` | 49 | 46 | 1 | 2% |
