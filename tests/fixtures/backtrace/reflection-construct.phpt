--TEST--
Reflection constructors show their native methods
--FILE--
<?php
class Example {
    function __construct($x) {
        debug_print_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS);
        var_dump(debug_backtrace()[1]['args']);
    }
}
$r = new ReflectionClass(Example::class);
$r->newInstance(21);
$r->newInstanceArgs([42]);
--EXPECTF--
#0 [internal function]: Example->__construct()
#1 %s/reflection-construct.php(9): ReflectionClass->newInstance()
array(1) {
  [0]=>
  int(21)
}
#0 [internal function]: Example->__construct()
#1 %s/reflection-construct.php(10): ReflectionClass->newInstanceArgs()
array(1) {
  [0]=>
  array(1) {
    [0]=>
    int(42)
  }
}
