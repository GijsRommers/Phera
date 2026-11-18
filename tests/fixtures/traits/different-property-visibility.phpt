--TEST--
traits: different property visibility
--FILE--
<?php
trait T {public $p;} class C{use T;protected $p;}
--EXPECTF--
Fatal error: C and T define the same property ($p) in the composition of C. However, the definition differs and is considered incompatible. Class was composed in %s on line 2
