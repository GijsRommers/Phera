--TEST--
Generator calls bind arguments without entering the body
--FILE--
<?php
class DefaultValue {
    public function __construct() { echo "default\n"; }
}
function values(int $number, $argument = new DefaultValue): Generator {
    echo "body\n";
    yield $number;
    return $argument instanceof DefaultValue;
}
try {
    values([]);
} catch (TypeError $error) {
    echo "bad argument\n";
}
$values = values(6);
echo "created\n";
var_dump($values->current());
$values->next();
var_dump($values->getReturn());
--EXPECT--
bad argument
default
created
body
int(6)
bool(true)
