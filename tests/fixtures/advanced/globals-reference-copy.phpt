--TEST--
globals-reference-copy
--FILE--
<?php $x=1; $alias =& $x; $copy=$GLOBALS; $copy['x']=2; echo $x, ':', $alias; $other=3; $GLOBALS['x'] =& $other; $other=4; echo ':', $x, ':', $GLOBALS['x'];

--EXPECT--
2:2:4:4