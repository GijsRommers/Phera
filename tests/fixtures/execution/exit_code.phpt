--TEST--
exit_code
--FILE--
<?php
echo "a";exit(3);echo "c";
--EXPECT--
a