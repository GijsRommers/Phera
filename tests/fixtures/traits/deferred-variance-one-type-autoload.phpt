--TEST--
An imported method with one missing signature type fails after autoload
--FILE--
<?php
class ParentService {
    function create($parentArg): DateTimeInterface {}
}
trait CreatesService {
    function create($traitArg): MissingTraitResult {}
}
spl_autoload_register(fn($name) => print "load $name\n");
class Service extends ParentService {
    use CreatesService;
}
echo "class linked\n";
--EXPECTF--
load MissingTraitResult

Fatal error: Could not check compatibility between Service::create($traitArg): MissingTraitResult and ParentService::create($parentArg): DateTimeInterface, because class MissingTraitResult is not available in %s on line 6
