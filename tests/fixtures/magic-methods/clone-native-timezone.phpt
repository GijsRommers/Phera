--TEST--
A cloned DateTimeZone retains its timezone
--FILE--
<?php
$copy = clone new DateTimeZone('Asia/Kathmandu');
echo $copy->getName();
--EXPECT--
Asia/Kathmandu
