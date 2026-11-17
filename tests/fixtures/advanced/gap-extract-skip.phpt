--TEST--
gap-extract-skip
--FILE--
<?php $a=1;extract(['a'=>2],EXTR_SKIP);echo $a;
--EXPECT--
1