<?php
$list = []; for ($i = 0; $i < 10000; $i++) { $list["k$i"] = [$i, "value $i", $i * 1.5, true, null, ['x' => $i]]; }
$n = 0; for ($r = 0; $r < 30; $r++) { $s = serialize($list); $n += strlen($s); $u = unserialize($s); $n += count($u); }
echo $n, "\n";
