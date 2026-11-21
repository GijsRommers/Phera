--TEST--
A literal null default also accepts an explicit null argument
--FILE--
<?php
function updateValue(int $value = null): void {
    var_dump($value);
}
updateValue();
updateValue(null);
var_dump((string) (new ReflectionFunction('updateValue'))->getParameters()[0]->getType());
--EXPECTF--
Deprecated: updateValue(): Implicitly marking parameter $value as nullable is deprecated, the explicit nullable type must be used instead in %s on line 2
NULL
NULL
string(4) "?int"
