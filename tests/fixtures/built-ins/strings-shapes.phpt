--TEST--
Byte search, repetition, spans and grouped numbers preserve PHP output
--FILE--
<?php
$binary = "xx\x80\0Word\xffWord";
var_dump(strpos($binary, "\x80\0"), strpos($binary, 'Word', 6), strrpos($binary, 'Word'));
var_dump(stripos($binary, 'woRD'), strpos($binary, '', -1), str_contains($binary, "\xffW"));
var_dump(explode("\x80\0", $binary));
var_dump(bin2hex(str_repeat("\xff", 7)), bin2hex(str_repeat("\0", 4)), str_repeat('', PHP_INT_MAX));
try { str_repeat('x', -1); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
$binary = "0\x80\xffabcdZ!";
var_dump(strspn($binary, "0123456789\xff\x80abcde"), strcspn($binary, "!Z\0\1\2\3"));
var_dump(strspn($binary, 'abcde', -6, 3), strcspn($binary, "d\xff\x80aZ", 3, -1));
foreach ([
    [1234567.891, 2, ",", "."], [PHP_INT_MAX, 2, '::', ''], [PHP_INT_MIN, -2, '', '_'],
    [1.005, 2, '.', ','], [-0.005, 2, '.', ','], [-0.004, 2, '.', ','],
    [1.23456, 4, '', "\0"], [1999, -2, '.', ','], [0, 3, '.', ','], [INF, 0, '.', ','],
] as [$number, $places, $decimal, $thousands]) {
    echo bin2hex(number_format($number, $places, $decimal, $thousands)), "\n";
}
--EXPECT--
int(2)
int(9)
int(9)
int(4)
int(12)
bool(true)
array(2) {
  [0]=>
  string(2) "xx"
  [1]=>
  string(9) "WordÿWord"
}
string(14) "ffffffffffffff"
string(8) "00000000"
string(0) ""
str_repeat(): Argument #2 ($times) must be greater than or equal to 0
int(7)
int(7)
int(3)
int(0)
312e3233342e3536372c3839
393232333337323033363835343737353830373a3a3030
2d395f3232335f3337325f3033365f3835345f3737355f383030
312e3031
2d302e3031
302e3030
3132333436
322c303030
302e303030
696e66
