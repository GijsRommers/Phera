--TEST--
Reads, isset, ?? and empty of local array elements: missing keys, nested levels, string offsets, references, key kinds, ArrayAccess, extract
--FILE--
<?php
function reads(array $row, array $nested, $key) {
    $out = [];
    $out[] = $row['id'];
    $out[] = $row[$key] ?? 'dflt';
    $out[] = isset($row['name']) ? 'set' : 'unset';
    $out[] = isset($row['nul']) ? 'set' : 'unset';
    $out[] = isset($row['missing']) ? 'set' : 'unset';
    $out[] = empty($row['zero']) ? 'empty' : 'full';
    $out[] = empty($row['missing']) ? 'empty' : 'full';
    $out[] = $nested['a']['b'] ?? 'no';
    $out[] = $nested['x']['b'] ?? 'no-level';
    $out[] = isset($nested['a']['b']) ? 'deep' : 'shallow';
    $out[] = isset($nested['s'][0]) ? 'str-offset' : 'no-offset';
    $out[] = $nested['s'][0] ?? 'q';
    $out[] = $row[1] ?? 'int';
    $out[] = $row['1'] ?? 'numeric-string';
    [$first, $second] = $nested['list'];
    $out[] = "$first/$second";
    return implode(',', $out);
}
$row = ['id' => 7, 'name' => 'n', 'nul' => null, 'zero' => 0, 1 => 'one', 'k' => 'K'];
$nested = ['a' => ['b' => 'B'], 's' => 'xyz', 'list' => [10, 20]];
for ($i = 0; $i < 2; $i++) echo reads($row, $nested, 'k'), "\n";
echo reads($row, $nested, 'none'), "\n";
// A reference element and a reference root.
$ref = 5; $withRef = ['r' => &$ref];
function viaRef(array $a) { return [$a['r'], isset($a['r']), $a['r'] ?? 0]; }
var_dump(viaRef($withRef));
function rootRef(array &$a) { return $a['r'] ?? 'none'; }
var_dump(rootRef($withRef));
// Missing element on a plain read warns.
function warn(array $a) { return $a['nope']; }
var_dump(warn([]));
// Float, bool and null keys.
function keys(array $a) { return [$a[true] ?? 't', $a[null] ?? 'n', $a[1.0] ?? 'f']; }
var_dump(keys([1 => 'one', '' => 'empty']));
// ArrayAccess and a dynamic scope.
$ao = new ArrayObject(['k' => 'v']);
function ao($o) { return [$o['k'] ?? 'no', isset($o['k'])]; }
var_dump(ao($ao));
function dyn(array $a) { extract(['b' => $a]); return $b['k'] ?? 'no'; }
var_dump(dyn(['k' => 'dynamic']));

--EXPECTF--
7,K,set,unset,unset,empty,empty,B,no-level,deep,str-offset,x,one,one,10/20
7,K,set,unset,unset,empty,empty,B,no-level,deep,str-offset,x,one,one,10/20
7,dflt,set,unset,unset,empty,empty,B,no-level,deep,str-offset,x,one,one,10/20
array(3) {
  [0]=>
  int(5)
  [1]=>
  bool(true)
  [2]=>
  int(5)
}
int(5)

Warning: Undefined array key "nope" in %s on line 33
NULL

Deprecated: Using null as an array offset is deprecated, use an empty string instead in %s on line 36
array(3) {
  [0]=>
  string(3) "one"
  [1]=>
  string(5) "empty"
  [2]=>
  string(3) "one"
}
array(2) {
  [0]=>
  string(1) "v"
  [1]=>
  bool(true)
}
string(7) "dynamic"
