--TEST--
C collation orders unsigned bytes and stops at NUL
--FILE--
<?php
setlocale(LC_COLLATE, 'C');
var_dump(strcoll('a', 'b') < 0, strcoll('b', 'a') > 0, strcoll("a\0z", 'a'));
--EXPECT--
bool(true)
bool(true)
int(0)
