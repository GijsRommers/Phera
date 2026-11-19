--TEST--
A default clause outside switch cannot declare a label
--FILE--
<?php
echo "skipped\n";
default:
--EXPECTF--
Parse error: syntax error, unexpected token "default", expecting end of file in %s on line 3
