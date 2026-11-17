--TEST--
callback exception handler
--FILE--
<?php
set_exception_handler(function($e){echo $e->getMessage(),"
";});throw new RuntimeException("handled closure");
--EXPECT--
handled closure
