--TEST--
traits: widened defaults are compatible
--FILE--
<?php
trait T{public float $a=1;} class C{use T; public float $a=1.0;} var_dump((new C)->a);
--EXPECT--
float(1)
