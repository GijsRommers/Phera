--TEST--
Default include once
--FILE--
<?php
set_include_path(__DIR__); spl_autoload("SPLRepeatedProbe"); spl_autoload("SPLRepeatedProbe");
--EXPECT--
loaded
