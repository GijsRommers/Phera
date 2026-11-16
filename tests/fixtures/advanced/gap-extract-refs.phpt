--TEST--
gap-extract-refs
--FILE--
<?php $items=['a'=>1];extract($items,EXTR_REFS);$a=9;echo $items['a'];
--EXPECT--
9