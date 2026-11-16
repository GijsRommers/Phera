--TEST--
Class members: error-readonly-modify
--FILE--
<?php class Foo {public readonly int $x;function run(){$this->x=1;$this->x=2;}} (new Foo)->run();
--EXPECTF--

Fatal error: Uncaught Error: Cannot modify readonly property Foo::$x in %s:1
Stack trace:
#0 %s(1): Foo->run()
#1 {main}
  thrown in %s on line 1
