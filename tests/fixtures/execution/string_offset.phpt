--TEST--
string_offset
--FILE--
<?php
$s='abcd';echo $s[0],$s[-1];
--EXPECT--
ad