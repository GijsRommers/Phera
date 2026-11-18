<?php
$a = fopen('lock', 'c+');
$b = fopen('lock', 'c+');
$wouldBlock = 100;
var_dump(flock($a, LOCK_EX), flock($b, LOCK_EX | LOCK_NB, $wouldBlock), $wouldBlock);
var_dump(flock($a, LOCK_UN), flock($b, LOCK_EX | LOCK_NB, $wouldBlock), $wouldBlock);
fclose($b);
var_dump(flock($a, LOCK_SH | LOCK_NB, $wouldBlock), $wouldBlock);
