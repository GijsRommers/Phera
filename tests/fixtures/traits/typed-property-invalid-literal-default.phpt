--TEST--
traits: typed property invalid literal default
--FILE--
<?php
echo "first"; trait T{public int $x="bad";}
--EXPECTF--
Fatal error: Cannot use string as default value for property T::$x of type int in %s on line 2
