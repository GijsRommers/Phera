--TEST--
A failed coercion leaves the referenced value and chains the original exception
--FILE--
<?php
class ThrowingAliasText {
    public function __toString(): string { throw new Exception('cannot cast'); }
}
class ThrowingAliasHolder { public string $label = 'old'; }
$holder = new ThrowingAliasHolder;
$aliases = [&$holder->label];
try {
    $aliases[0] = new ThrowingAliasText;
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
    echo $e->getPrevious()->getMessage(), "\n";
}
var_dump($holder->label);
--EXPECT--
Cannot assign ThrowingAliasText to reference held by property ThrowingAliasHolder::$label of type string
cannot cast
string(3) "old"
