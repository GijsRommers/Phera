--TEST--
isset, empty and ?? see a string offset outside the string as missing
--FILE--
<?php
$s = "abc"; $o = 3; $n = -4; $k = "1"; $f = 1.0; $bad = "x"; $big = PHP_INT_MAX;
var_dump(isset($s[$o]), isset($s[2]), isset($s[3]), isset($s[-1]), isset($s[-3]), isset($s[-4]), isset($s[$n]), isset($s[$k]), isset($s[$f]), isset($s[$bad]), isset($s["1"]), isset($s["1x"]), isset($s[" 1"]), isset($s[$big]), isset($s[true]), isset($s[1][0]), isset($s[1][1]), isset($s[0][0][0]));
var_dump(empty($s[$o]), empty($s[0]), empty("0"[0]), isset($s[$o + 0]), $s[$o] ?? 'default', $s[-4] ?? 'neg', $s[1] ?? 'no');
function g(string $c) { $i = 0; $out = ''; while (isset($c[$i])) { $out .= $c[$i]; $i++; } return $out . '|' . $i; }
echo g("hello"), ' ', g(""), "\n";
class W { public string $p = "xy"; } $w = new W; $i = 2; var_dump(isset($w->p[$i]), isset($w->p[1]), isset($w->q[0]));
$arr = ['s' => 'ab']; var_dump(isset($arr['s'][$i]), isset($arr['s'][1]), isset($arr['t'][0]));
--EXPECT--
bool(false)
bool(true)
bool(false)
bool(true)
bool(true)
bool(false)
bool(false)
bool(true)
bool(true)
bool(false)
bool(true)
bool(false)
bool(true)
bool(false)
bool(true)
bool(true)
bool(false)
bool(true)
bool(true)
bool(false)
bool(true)
bool(false)
string(7) "default"
string(3) "neg"
string(1) "b"
hello|5 |0
bool(false)
bool(true)
bool(false)
bool(false)
bool(true)
bool(false)
