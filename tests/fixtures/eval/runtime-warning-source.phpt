--TEST--
Runtime warning source
--FILE--
<?php
eval('trigger_error("inside eval");');
--EXPECTF--

Notice: inside eval in %s/runtime-warning-source.php(2) : eval()'d code on line 1
