--TEST--
nested_break
--FILE--
<?php
for($i=0;$i<5;$i++){for($j=0;$j<5;$j++){echo $i,$j; if($j==2)break 2;}}
--EXPECT--
000102