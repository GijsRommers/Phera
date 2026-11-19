--TEST--
An imported method with two missing signature types fails after autoload
--FILE--
<?php
class ParentService {
    function create($parentArg): MissingParentResult {}
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
load MissingParentResult

Fatal error: Could not check compatibility between Service::create($traitArg): MissingTraitResult and ParentService::create($parentArg): MissingParentResult, because class MissingTraitResult is not available in %s on line 6
