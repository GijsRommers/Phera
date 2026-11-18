--TEST--
anonymous-classes: readonly static property
--FILE--
<?php
new readonly class { public static int $value; };
--EXPECTF--
Fatal error: Static property class@anonymous::$value cannot be readonly in %s on line 2
