--TEST--
foreach-nested5
--FILE--
<?php $items=[1,2,3];foreach($items as &$outer) { foreach($items as &$inner) { echo $outer,$inner; } }
--EXPECT--
111213212223313233