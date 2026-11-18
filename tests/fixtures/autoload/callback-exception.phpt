--TEST--
Callback exception
--FILE--
<?php
spl_autoload_register(function($x) { throw new RuntimeException($x); }); try { class_exists("Wanted"); } catch (RuntimeException $e) { echo $e->getMessage(), "\n"; }
--EXPECT--
Wanted
