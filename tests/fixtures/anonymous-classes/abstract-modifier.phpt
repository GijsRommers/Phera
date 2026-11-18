--TEST--
anonymous-classes: abstract modifier
--FILE--
<?php
new abstract class {};
--EXPECTF--
Fatal error: Cannot use the abstract modifier on an anonymous class in %s on line 2
