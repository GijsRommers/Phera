--TEST--
Magic line
--FILE--
<?php
var_dump(eval("\necho __LINE__, PHP_EOL;\nreturn __LINE__;"));
--EXPECT--
2
int(3)
