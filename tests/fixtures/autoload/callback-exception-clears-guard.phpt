--TEST--
Callback exception clears guard
--FILE--
<?php
$once = false; spl_autoload_register(function($x) use (&$once) { if (!$once) { $once = true; throw new Exception("first"); } class Wanted {} }); try { class_exists("Wanted"); } catch (Exception $e) { echo $e->getMessage(), "\n"; } var_dump(class_exists("Wanted"));
--EXPECT--
first
bool(true)
