--TEST--
isset_empty_coalesce_in_conditions
--FILE--
<?php
$arr = ['k' => 0, 'z' => null];
if (isset($arr['k'])) echo "isset-k\n";
if (isset($arr['z'])) echo "isset-z\n"; else echo "notset-z\n";
if (!isset($arr['no'])) echo "notset-no\n";
var_dump(isset($arr['k'], $arr['z']), isset($arr['k'], $arr['no']), isset($arr['k'], $arr['k']));
if (isset($arr['k'], $arr['k'])) echo "both-k\n";
$v = null;
if (isset($v)) echo "isset-v\n"; else echo "notset-v\n";
if (empty($v)) echo "empty-v\n";
if (empty($arr['k'])) echo "empty-k\n"; else echo "notempty-k\n";
if (!empty($arr['z'])) echo "notempty-z\n"; else echo "empty-z\n";
if (empty($arr['no'])) echo "empty-no\n";
var_dump(empty(0), empty(''), empty('0'), empty([]), empty(0.0), empty('x'), empty([0]));
var_dump($arr['no'] ?? 'dflt', $arr['k'] ?? 'dflt', $arr['z'] ?? 'dflt');
if ($arr['no'] ?? null) echo "co-yes\n"; else echo "co-no\n";
if ($arr['k'] ?? null) echo "co-k-yes\n"; else echo "co-k-no\n";
if ($arr['z'] ?? 'x') echo "co-z-yes\n"; else echo "co-z-no\n";
while (($arr['z'] ?? 'x') === 'x') { echo "co-loop\n"; break; }
$w = $arr['no'] ?? ($arr['k'] ?? 7);
var_dump($w);
if (isset($arr[$arr['k']])) echo "nested\n"; else echo "notnested\n";
--EXPECTF--
isset-k
notset-z
notset-no
bool(false)
bool(false)
bool(true)
both-k
notset-v
empty-v
empty-k
empty-z
empty-no
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(false)
bool(false)
string(4) "dflt"
int(0)
string(4) "dflt"
co-no
co-k-no
co-z-yes
co-loop
int(0)
notnested
