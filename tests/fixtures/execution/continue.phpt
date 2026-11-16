--TEST--
continue
--FILE--
<?php
for($i=0;$i<5;$i++){if($i==3)continue;echo $i;}
--EXPECT--
0124