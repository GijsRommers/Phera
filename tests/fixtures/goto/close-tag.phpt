--TEST--
A PHP closing tag can terminate a goto statement
--FILE--
<?php
goto end ?>
skipped
<?php end: echo "end\n";
--EXPECT--
end
