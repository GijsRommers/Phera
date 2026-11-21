--TEST--
A promoted property needs a nullable type for a null default
--FILE--
<?php
class Container {
    public function __construct(public int $count = null) {}
}
--EXPECTF--
Fatal error: Cannot use null as default value for parameter $count of type int in %s on line 3
