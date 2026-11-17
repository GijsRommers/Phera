--TEST--
coalesce
--FILE--
<?php
echo $missing ?? "fallback";$a=0;echo $a??42;
--EXPECT--
fallback0