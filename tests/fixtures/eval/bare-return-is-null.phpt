--TEST--
Bare return is null
--FILE--
<?php
var_dump(eval('return;'));
--EXPECT--
NULL
