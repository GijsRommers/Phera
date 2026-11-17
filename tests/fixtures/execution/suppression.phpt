--TEST--
suppression
--FILE--
<?php
echo @$missing; echo "done";
--EXPECT--
done