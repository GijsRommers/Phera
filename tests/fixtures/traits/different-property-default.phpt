--TEST--
traits: different property default
--FILE--
<?php
trait T {public $p=1;} class C{use T;public $p=2;}
--EXPECTF--
Fatal error: C and T define the same property ($p) in the composition of C. However, the definition differs and is considered incompatible. Class was composed in %s on line 2
