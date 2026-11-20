--TEST--
Indexed writes evaluate expressions early and fetch compiled key locals after the right-hand side
--FILE--
<?php
function offset() { echo "offset\n"; return 0; }
function contents() { echo "right\n"; return 'C'; }
$string = 'ab';
$string[offset()] = contents();
echo $string, "\n";
$index=0;
$string[$index] = ($index=1);
echo $string, " ", $index, "\n";
$array=[];
$index=0;
$array[$index] = ($index=1);
var_dump($array);
$array=[];
$array[$index=0][$index] = ($index=1);
var_dump($array);
$array=[];
$array[$later] = ($later=0);
var_dump($array);
--EXPECT--
offset
right
Cb
C1 1
array(1) {
  [1]=>
  int(1)
}
array(1) {
  [0]=>
  array(1) {
    [1]=>
    int(1)
  }
}
array(1) {
  [0]=>
  int(0)
}
