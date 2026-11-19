--TEST--
An imported method with one missing signature type fails without an autoloader
--FILE--
<?php
class ParentService {
    function create($parentArg): DateTimeInterface {}
}
trait CreatesService {
    function create($traitArg): MissingTraitResult {}
}
class Service extends ParentService {
    use CreatesService;
}
echo "class linked\n";
--EXPECTF--
Fatal error: Could not check compatibility between Service::create($traitArg): MissingTraitResult and ParentService::create($parentArg): DateTimeInterface, because class MissingTraitResult is not available in %s on line 6
