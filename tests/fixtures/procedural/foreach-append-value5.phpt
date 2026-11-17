--TEST--
foreach-append-value5
--FILE--
<?php $items=[1,2,3]; $count=0; foreach($items as $value) { $count++; echo $value,"\n"; if($count==1) $items[]=4; if($count>5) break; }
--EXPECT--
1
2
3
