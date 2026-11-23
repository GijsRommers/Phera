--TEST--
Exists probes again after an autoloader is registered
--FILE--
<?php
var_dump(class_exists('Later'));
spl_autoload_register(function ($name) { echo "autoload $name\n"; if ($name === 'Defined') { class Defined {} } });
var_dump(class_exists('Later'));
var_dump(class_exists('Later', false));
var_dump(interface_exists('Later'));
var_dump(enum_exists('Later'));
var_dump(trait_exists('Later'));
var_dump(class_exists('Defined'));
--EXPECT--
bool(false)
autoload Later
bool(false)
bool(false)
autoload Later
bool(false)
autoload Later
bool(false)
autoload Later
bool(false)
autoload Defined
bool(true)
