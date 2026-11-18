--TEST--
anonymous-classes: readonly untyped property
--FILE--
<?php
new readonly class { public $value; };
--EXPECTF--
Fatal error: Readonly property class@anonymous::$value must have type in %s on line 2
