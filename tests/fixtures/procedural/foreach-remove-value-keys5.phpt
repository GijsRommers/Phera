--TEST--
foreach-remove-value-keys5
--FILE--
<?php $items=[1,2,3]; $count=0; foreach($items as $key=>$value) { $count++; echo $key,":",$value,"\n"; unset($items[1]); if($count>5) break; }
--EXPECT--
0:1
1:2
2:3
