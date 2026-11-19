<?php
class P { public $a = 1; public $b = 'two'; public $c = 3.5; }
$list = []; for ($i = 0; $i < 10000; $i++) { $list[] = new P; }
$n = 0; for ($r = 0; $r < 30; $r++) { $s = serialize($list); $n += strlen($s); $u = unserialize($s); $n += count($u); }
echo $n, "\n";
