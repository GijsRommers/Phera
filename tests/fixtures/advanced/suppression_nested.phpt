--TEST--
suppression_nested
--FILE--
<?php
echo @(@$missing . $other);echo 'done';
--EXPECT--
done