<?php
$offset = 7;
$closure = function ($value) use ($offset) {
    return $value + $offset;
};
$total = 0;
for ($index = 0; $index < 1000000; ++$index) {
    $total += $closure($index);
}
$arrow = fn($value) => $value + $offset;
for ($index = 0; $index < 200000; ++$index) {
    $mapped = array_map($arrow, [1, 2, 3, 4]);
    $total += $mapped[3];
}
class ClosureFactory {
    private $offset = 3;
    public function make() {
        return function ($value) {
            return $this->offset + $value;
        };
    }
}
$factory = new ClosureFactory;
for ($index = 0; $index < 200000; ++$index) {
    $created = $factory->make();
}
echo $total, "\n";
