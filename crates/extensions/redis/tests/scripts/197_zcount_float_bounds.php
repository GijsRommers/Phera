<?php
require __DIR__ . '/../support.php';
// phpredis declares zCount's bounds int|string but reads them as strings:
// a float bound keeps its fraction instead of being cut to an int.
$r->zAdd('z197', 1, 'a', 2.5, 'b', 3, 'c');
check(fn()=>$r->zCount('z197', 1, 2.5));
check(fn()=>$r->zCount('z197', 2.5, 2.5));
check(fn()=>$r->zCount('z197', '(1', '+inf'));
check(fn()=>$r->zCount('z197', -1.5, 0.5));
check(fn()=>$r->zCount('z197', [], 1));
$r->del('z197');
finish();
