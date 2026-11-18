--TEST--
Associative intersection casts values only after an exact key match
--FILE--
<?php
class IntersectionLabel {
    public function __construct(private string $name, private string $value) {}
    public function __toString(): string {
        echo $this->name, "\n";
        return $this->value;
    }
}
$first = [
    'missing' => new IntersectionLabel('unmatched', 'same'),
    'keep' => new IntersectionLabel('left', 'same'),
    'drop' => new IntersectionLabel('different', 'old'),
];
$second = [
    'other' => new IntersectionLabel('ignored', 'same'),
    'keep' => new IntersectionLabel('right', 'same'),
    'drop' => 'new',
];
$result = array_intersect_assoc($first, $second);
var_dump(array_keys($result), $result['keep'] === $first['keep']);
--EXPECT--
left
right
different
array(1) {
  [0]=>
  string(4) "keep"
}
bool(true)
