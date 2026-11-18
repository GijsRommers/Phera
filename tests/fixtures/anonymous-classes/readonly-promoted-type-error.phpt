--TEST--
anonymous-classes: readonly promoted type error
--FILE--
<?php
new readonly class(1) { function __construct(public $x) {} };
--EXPECTF--
Fatal error: Readonly property class@anonymous::$x must have type in /private%s on line 2
