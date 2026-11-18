--TEST--
Shutdown runs after a runtime fatal error
--FILE--
<?php
register_shutdown_function(fn() => print "shutdown\n");
class Base { public function foo(int $number): void {} }
if (true) {
    class Child extends Base { public function foo(string $number): void {} }
}
--EXPECTF--

Fatal error: Declaration of Child::foo(string $number): void must be compatible with Base::foo(int $number): void in %s on line 5
shutdown
