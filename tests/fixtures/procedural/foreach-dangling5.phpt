--TEST--
foreach-dangling5
--FILE--
<?php $items=[1,2,3]; foreach($items as &$value) { break; } foreach($items as $value) echo $value; var_dump($items);
--EXPECT--
123array(3) {
  [0]=>
  &int(3)
  [1]=>
  int(2)
  [2]=>
  int(3)
}
