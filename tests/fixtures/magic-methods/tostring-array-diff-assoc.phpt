--TEST--
Associative difference casts only entries with an exact key match
--FILE--
<?php
class DifferenceLabel {
    public function __construct(private string $name) {}
    public function __toString(): string {
        echo $this->name, "\n";
        return 'same';
    }
}
$first = ['match' => new DifferenceLabel('left'), 'unmatched' => new DifferenceLabel('kept')];
$second = ['match' => new DifferenceLabel('right'), 'other' => new DifferenceLabel('ignored')];
$result = array_diff_assoc($first, $second);
var_dump(array_keys($result), $result['unmatched'] === $first['unmatched']);
--EXPECT--
left
right
array(1) {
  [0]=>
  string(9) "unmatched"
}
bool(true)
