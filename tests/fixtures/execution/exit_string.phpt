--TEST--
exit_string
--FILE--
<?php
echo "a";exit("b");echo "c";
--EXPECT--
ab