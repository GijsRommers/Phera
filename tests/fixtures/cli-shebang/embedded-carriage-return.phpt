--TEST--
A carriage return inside a CLI shebang does not add a line
--FILE--
#!firstpart
<?php echo __LINE__, PHP_EOL;
--EXPECT--
2
