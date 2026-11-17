--TEST--
foreach-unset-ref5
--FILE--
<?php $items=[1,2,3]; $count=0; foreach($items as &$value) { $count++; echo $value,"\n"; unset($items); if($count>5) break; }
--EXPECT--
1
2
3
