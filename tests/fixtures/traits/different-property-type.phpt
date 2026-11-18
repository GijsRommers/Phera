--TEST--
traits: different property type
--FILE--
<?php
trait T {public int $p;} class C{use T;public float $p;}
--EXPECTF--
Fatal error: C and T define the same property ($p) in the composition of C. However, the definition differs and is considered incompatible. Class was composed in %s on line 2
