--TEST--
offset_negative
--FILE--
<?php
$s='abc';$s[-1]='X';echo $s;
--EXPECT--
abX