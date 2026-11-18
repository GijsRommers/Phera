--TEST--
SPL array sorting uses object string conversions and protects storage during callbacks
--FILE--
<?php
class SortLabel {
    public function __construct(public string $label) {}
    public function __toString(): string {
        global $entries;
        try {
            $entries[] = new self('forbidden');
        } catch (Error $error) {
            echo $error->getMessage(), "\n";
        }
        return $this->label;
    }
}
$entries = new ArrayObject(['large' => new SortLabel('12'), 'small' => new SortLabel('2')]);
$entries->natsort();
echo implode(',', array_keys($entries->getArrayCopy())), "\n";
$entries[] = new SortLabel('3');
echo count($entries), "\n";
--EXPECT--
Modification of ArrayObject during sorting is prohibited
Modification of ArrayObject during sorting is prohibited
small,large
3
