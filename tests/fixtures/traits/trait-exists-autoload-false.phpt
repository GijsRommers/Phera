--TEST--
traits: trait exists autoload false
--FILE--
<?php
spl_autoload_register(function ($name) {
    echo "loaded $name";
});
var_dump(trait_exists("Missing", autoload: false));
--EXPECT--
bool(false)
