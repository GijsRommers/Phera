--TEST--
Reserved class alias
--INI--
fatal_error_backtraces=1
--FILE--
<?php
class Example {}
class_alias(Example::class, 'int');
--EXPECTF--
Fatal error: Cannot use "int" as a class alias as it is reserved in %sreserved-class-alias.php on line 3
Stack trace:
#0 %sreserved-class-alias.php(3): class_alias('Example', 'int')
#1 {main}
