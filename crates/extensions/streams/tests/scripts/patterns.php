<?php
mkdir('sub');
file_put_contents('b.txt', '');
file_put_contents('a.txt', '');
file_put_contents('.hidden', '');
file_put_contents('sub/c.txt', '');
var_dump(glob('*.txt'));
var_dump(glob('{b,a}.txt', GLOB_BRACE));
var_dump(glob('s*', GLOB_ONLYDIR | GLOB_MARK));
var_dump(glob('nomatch*', GLOB_NOCHECK));
var_dump(glob('sub/*.txt', GLOB_ERR));
var_dump(fnmatch('sub/*.TXT', 'sub/c.txt', FNM_CASEFOLD | FNM_PATHNAME));
var_dump(fnmatch('*', '.hidden', FNM_PERIOD));
var_dump(fnmatch('[[:digit:]].txt', '5.txt'));
