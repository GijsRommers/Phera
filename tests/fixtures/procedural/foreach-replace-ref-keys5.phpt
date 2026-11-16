--TEST--
foreach-replace-ref-keys5
--FILE--
<?php $items=[1,2,3]; $count=0; foreach($items as $key=>&$value) { $count++; echo $key,":",$value,"\n"; if($count==1) $items=[7,8]; if($count>5) break; }
--EXPECT--
0:1
0:7
1:8
