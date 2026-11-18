--TEST--
Binary HTML before and after PHP tags is preserved
--FILE--
[ÿ]<?php echo "[php]"; ?>[€]
--EXPECT--
[ÿ][php][€]
