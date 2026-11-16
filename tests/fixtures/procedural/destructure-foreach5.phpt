--TEST--
destructure-foreach5
--FILE--
<?php $rows=[[1,[2,3]],[4,[5,6]]]; foreach($rows as [$a,[$b,&$c]]) { echo $a,$b,$c; $c=9; } var_dump($rows);
--EXPECT--
123456array(2) {
  [0]=>
  array(2) {
    [0]=>
    int(1)
    [1]=>
    array(2) {
      [0]=>
      int(2)
      [1]=>
      int(9)
    }
  }
  [1]=>
  array(2) {
    [0]=>
    int(4)
    [1]=>
    array(2) {
      [0]=>
      int(5)
      [1]=>
      &int(9)
    }
  }
}
