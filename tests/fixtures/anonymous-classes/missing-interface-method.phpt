--TEST--
anonymous-classes: missing interface method
--FILE--
<?php
interface I { function x(); } new class implements I {};
--EXPECTF--
Fatal error: Class I@anonymous must implement 1 abstract method (I::x) in %s on line 2
