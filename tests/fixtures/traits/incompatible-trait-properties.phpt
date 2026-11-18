--TEST--
traits: incompatible trait properties
--FILE--
<?php
trait T {public $p=1;} trait U{public $p=2;} class C{use T,U;}
--EXPECTF--
Fatal error: T and U define the same property ($p) in the composition of C. However, the definition differs and is considered incompatible. Class was composed in %s on line 2
