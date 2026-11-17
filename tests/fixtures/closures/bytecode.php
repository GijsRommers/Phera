<?php
$a = 1;
$b = 2;
$f = function ($x) use ($a, &$b) { return $x + $a + $b; };
$g = fn($x) => $x + $a;
$h = strlen(...);
