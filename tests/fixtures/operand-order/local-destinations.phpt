--TEST--
Direct literal assignment preserves references and releases the old local
--FILE--
<?php
class TypedDestination {
    public int $value = 1;
}
$holder = new TypedDestination();
$alias =& $holder->value;
$alias = '61';
var_dump($holder->value);
try {
    $alias = 'invalid';
} catch (TypeError $exception) {
    echo $exception->getMessage(), "\n";
}
var_dump($holder->value);
class OldValue {
    public function __destruct() {
        echo "old value released\n";
    }
}
$observed = new OldValue();
$observed = 42;
var_dump($observed);
--EXPECT--
int(61)
Cannot assign string to reference held by property TypedDestination::$value of type int
int(61)
old value released
int(42)
