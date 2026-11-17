--TEST--
destructure-scalars5
--FILE--
<?php [$a] = null; [$b] = 1; [$c] = 'abc'; var_dump($a,$b,$c); [, $a,, $b]=[1,2,3,4];var_dump($a,$b);
--EXPECTF--

Warning: Cannot use int as array in %s on line 1

Warning: Cannot use string as array in %s on line 1
NULL
NULL
NULL
int(2)
int(4)
