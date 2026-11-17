<?php
class Vec {
    public function __construct(public float $x, public float $y) {}
    public function scale(float $by): static { return new static($this->x * $by, $this->y * $by); }
    public function length2(): float { return $this->x * $this->x + $this->y * $this->y; }
}
function total(int $count): float {
    $sum = 0.0;
    for ($i = 0; $i < $count; $i++) {
        $vector = new Vec($i % 10, $i % 7);
        $sum += $vector->scale(2)->length2();
    }
    return $sum;
}
echo total(400000), "\n";
