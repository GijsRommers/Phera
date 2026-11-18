--TEST--
traits: incompatible trait constant
--FILE--
<?php
trait T{const X=2;} class C{use T; const X=3;}
--EXPECTF--
Fatal error: C and T define the same constant (X) in the composition of C. However, the definition differs and is considered incompatible. Class was composed in %s on line 2
