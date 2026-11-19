--TEST--
An imported method with two missing signature types fails without an autoloader
--FILE--
<?php
class ParentService {
    function create($parentArg): MissingParentResult {}
}
trait CreatesService {
    function create($traitArg): MissingTraitResult {}
}
class Service extends ParentService {
    use CreatesService;
}
echo "class linked\n";
--EXPECTF--
Fatal error: Could not check compatibility between Service::create($traitArg): MissingTraitResult and ParentService::create($parentArg): MissingParentResult, because class MissingTraitResult is not available in %s on line 6
