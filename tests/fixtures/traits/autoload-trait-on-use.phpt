--TEST--
traits: autoload trait on use
--FILE--
<?php
spl_autoload_register(function ($name) {
    echo "load $name
";
    trait LoadedTrait { function answer() { return 7; } }
});
class User { use LoadedTrait; }
echo (new User)->answer();
--EXPECT--
load LoadedTrait
7
