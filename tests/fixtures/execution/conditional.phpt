--TEST--
conditional
--FILE--
<?php
if(true){function yes(){echo 42;}}yes();
--EXPECT--
42