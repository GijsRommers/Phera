--TEST--
traits: interface trait error
--FILE--
<?php
trait T{} interface I{use T;}
--EXPECTF--
Fatal error: Cannot use traits inside of interfaces. T is used in I in /private%s on line 2
