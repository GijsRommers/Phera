--TEST--
Unset static property loads class
--FILE--
<?php
spl_autoload_register(function($name) { throw new Exception($name); });
try { unset(Missing::$p); } catch (Exception $e) { echo $e->getMessage(), "\n"; }
--EXPECT--
Missing
