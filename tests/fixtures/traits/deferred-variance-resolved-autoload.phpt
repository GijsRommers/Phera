--TEST--
An imported method is checked again after both types load
--FILE--
<?php
class ParentService {
    function create(): ParentResult {}
}
trait CreatesService {
    function create(): TraitResult { return new TraitResult; }
}
spl_autoload_register(function ($name) {
    echo "load $name\n";
    if ($name === 'TraitResult') {
        class TraitResult extends ParentResult {}
    } elseif ($name === 'ParentResult') {
        class ParentResult {}
    }
});
class Service extends ParentService { use CreatesService; }
echo get_class((new Service)->create()), "\n";
--EXPECT--
load TraitResult
load ParentResult
TraitResult
