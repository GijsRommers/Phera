--TEST--
anonymous-classes: readonly declared default error
--FILE--
<?php
new readonly class { public int $x = 1; };
--EXPECTF--
Fatal error: Readonly property class@anonymous::$x cannot have default value in /private%s on line 2
