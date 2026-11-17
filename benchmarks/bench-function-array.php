<?php
function build(int $n): array { $a = []; for ($i = 0; $i < $n; $i++) { $a[] = $i * 2; } return $a; }
function total(array $a): int { $s = 0; $n = count($a); for ($i = 0; $i < $n; $i++) { $s += $a[$i]; } foreach ($a as $v) { $s += $v; } return $s; }
echo total(build(1000000)), "\n";
