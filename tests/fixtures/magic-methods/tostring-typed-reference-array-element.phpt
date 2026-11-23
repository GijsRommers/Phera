--TEST--
An assignment through an array reference coerces to the property type
--FILE--
<?php
class ArrayAliasText {
    public function __toString(): string { echo "cast\n"; return 'new'; }
}
class AliasHolder { public string $label = 'old'; }
$holder = new AliasHolder;
$aliases = ['group' => ['label' => &$holder->label]];
$input = new ArrayAliasText;
var_dump($aliases['group']['label'] = $input);
var_dump($holder->label, $aliases['group']['label'], $input instanceof ArrayAliasText);
--EXPECT--
cast
string(3) "new"
string(3) "new"
string(3) "new"
bool(true)
