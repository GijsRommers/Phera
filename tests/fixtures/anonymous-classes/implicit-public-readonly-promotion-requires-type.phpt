--TEST--
anonymous-classes: implicit public readonly promotion requires type
--FILE--
<?php
new readonly class(2) {function __construct(readonly $x){}};
--EXPECTF--
Fatal error: Readonly property class@anonymous::$x must have type in %s on line 2
