--TEST--
An array reference held in an object property keeps its type constraint
--FILE--
<?php
class PropertyAliasText {
    public function __toString(): string { return 'written'; }
}
class PropertyAliasHolder {
    public string $label = 'old';
    public array $aliases = [];
}
$holder = new PropertyAliasHolder;
$holder->aliases = ['label' => &$holder->label];
$input = new PropertyAliasText;
var_dump($holder->aliases['label'] = $input, $holder->label);
--EXPECT--
string(7) "written"
string(7) "written"
