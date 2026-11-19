--TEST--
Constant builtins preserve PHP byte lengths and array counts
--FILE--
<?php
var_dump(ord("\xFF"), strlen("a\x00\xFF"), bin2hex(chr(0)), count([1, 2]), sizeof([]));
--EXPECT--
int(255)
int(3)
string(2) "00"
int(2)
int(0)
