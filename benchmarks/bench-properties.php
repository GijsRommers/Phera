<?php
class Properties {
    public int $number = 0;
    public array $items = [];
    public function change($other) {
        $this->number++;
        $other->number = $this->number;
        $this->items[] = $other->number;
        return $other->number;
    }
}
$a = new Properties;
$b = new Properties;
$sum = 0;
for ($i = 0; $i < 500000; $i++) { $sum += $a->change($b); }
echo $sum, ':', count($a->items), ':', $b->number, "\n";
