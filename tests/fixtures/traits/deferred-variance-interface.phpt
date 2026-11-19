--TEST--
An imported method defers signature checks against an interface
--FILE--
<?php
interface ServiceFactory {
    public function create($contractArg): InterfaceResult;
}
trait CreatesService {
    public function create($traitArg): TraitResult {}
}
spl_autoload_register(fn($name) => print "load $name\n");
class Service implements ServiceFactory { use CreatesService; }
echo "class linked\n";
--EXPECTF--
load TraitResult
load InterfaceResult

Fatal error: Could not check compatibility between Service::create($traitArg): TraitResult and ServiceFactory::create($contractArg): InterfaceResult, because class TraitResult is not available in %s on line 6
