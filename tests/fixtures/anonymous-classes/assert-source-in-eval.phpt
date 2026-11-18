--TEST--
Assertions quote the source of an anonymous class declared in eval
--FILE--
<?php
$object = eval('return new class {
    public function verify(int $value): void {
        assert($value === 4);
    }
};');
try {
    $object->verify(5);
} catch (AssertionError $error) {
    echo $error->getMessage(), "\n";
}
--EXPECT--
assert($value === 4)
