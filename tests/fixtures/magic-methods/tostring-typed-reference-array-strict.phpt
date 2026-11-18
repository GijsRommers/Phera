--TEST--
Strict indexed alias assignment does not coerce the object
--FILE--
<?php
declare(strict_types=1);
class StrictAliasText {
    public function __toString(): string { echo "cast\n"; return 'new'; }
}
class StrictAliasHolder { public string $label = 'old'; }
$holder = new StrictAliasHolder;
$aliases = [&$holder->label];
try {
    $aliases[0] = new StrictAliasText;
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
var_dump($holder->label);
--EXPECT--
Cannot assign StrictAliasText to reference held by property StrictAliasHolder::$label of type string
string(3) "old"
