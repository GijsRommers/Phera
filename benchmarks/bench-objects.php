<?php
class Counter {
    public int $count = 0;
    public function first(): int { return $this->count; }
    public function second(): int { return $this->count + 1; }
}
$total = 0;
for ($index = 0; $index < 1000000; $index++) {
    $counter = new Counter;
    $total += $counter->first();
    $total += $counter->second();
    $counter->count++;
}
echo $total, "\n";
