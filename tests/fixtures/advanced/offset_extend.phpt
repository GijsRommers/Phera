--TEST--
offset_extend
--FILE--
<?php
$s='ab';$s[4]='X';echo $s;
--EXPECT--
ab  X