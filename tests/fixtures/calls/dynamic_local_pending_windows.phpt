--TEST--
Dynamic locals grow before reserved nested argument windows
--FILE--
<?php
function capture($first, $second, ...$rest) { return [$first, $second, $rest]; }
function inner() { return 5; }
$name = 'added';
var_dump(capture($$name = 3, inner(), 9), $added);
function run() {
    $name = 'late';
    $result = capture(2, $$name = 7, inner());
    var_dump($result, $late);
}
run();
--EXPECT--
array(3) {
  [0]=>
  int(3)
  [1]=>
  int(5)
  [2]=>
  array(1) {
    [0]=>
    int(9)
  }
}
int(3)
array(3) {
  [0]=>
  int(2)
  [1]=>
  int(7)
  [2]=>
  array(1) {
    [0]=>
    int(5)
  }
}
int(7)
