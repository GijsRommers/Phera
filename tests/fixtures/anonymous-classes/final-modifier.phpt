--TEST--
anonymous-classes: final modifier
--FILE--
<?php
new final class {};
--EXPECTF--
Fatal error: Cannot use the final modifier on an anonymous class in /private%s on line 2
