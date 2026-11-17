<?php
class Vec { public function __construct(public float $x, public float $y) {} public function len2(): float { return $this->x * $this->x + $this->y * $this->y; } }
function work(int $n): float { $acc = 0.0; for ($i = 0; $i < $n; $i++) { $v = new Vec($i % 10, $i % 7); $acc += $v->len2(); if ($acc > 1e9) { $acc -= 1e9; } } return $acc; }
echo work(600000), "\n";
