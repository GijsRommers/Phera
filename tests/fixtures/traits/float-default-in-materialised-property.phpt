--TEST--
traits: float default in materialised property
--FILE--
<?php
trait T{public float $a=1;} class C{use T;} var_dump((new C)->a);
--EXPECT--
float(1)
