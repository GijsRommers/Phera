--TEST--
PHP converts against each typed property holding the array reference
--FILE--
<?php
class MultipleAliasText {
    private int $calls = 0;
    public function __toString(): string {
        echo 'cast ', ++$this->calls, "\n";
        return "value $this->calls";
    }
}
class MultipleAliasHolder {
    public string $a = 'old';
    public string $b = '';
}
$holder = new MultipleAliasHolder;
$holder->b =& $holder->a;
$aliases = [&$holder->a];
try {
    $aliases[0] = new MultipleAliasText;
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
var_dump($holder->a, $holder->b, $aliases[0]);
--EXPECT--
cast 1
cast 2
Cannot assign MultipleAliasText to reference held by property MultipleAliasHolder::$a of type string and property MultipleAliasHolder::$b of type string, as this would result in an inconsistent type conversion
string(3) "old"
string(3) "old"
string(3) "old"
