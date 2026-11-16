--TEST--
offset_write
--FILE--
<?php
$s='abc';$s[1]='X';echo $s;
--EXPECT--
aXc