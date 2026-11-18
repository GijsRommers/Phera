--TEST--
traits: typed constant invalid literal default
--FILE--
<?php
trait T{const int X="x";}
--EXPECTF--
Fatal error: Cannot use string as value for class constant T::X of type int in %s on line 2
