--TEST--
A resumed comparison applies native comparison hooks to the remaining entries
--FILE--
<?php
class ComparisonLabel {
    public function __toString(): string { echo "label hook\n"; return 'label'; }
}
$left = [new ComparisonLabel, new DateTime('2000-01-02', new DateTimeZone('UTC'))];
$right = ['label', new DateTimeImmutable('2000-01-01', new DateTimeZone('UTC'))];
var_dump($left > $right);
--EXPECT--
label hook
bool(true)
