--TEST--
callable_property_error
--FILE--
<?php
class A{public callable $callback;}
--EXPECTF--
Fatal error: Property A::$callback cannot have type callable in %s on line 2
