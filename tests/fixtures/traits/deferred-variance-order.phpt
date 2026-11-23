--TEST--
Trait imports join the parent-method variance order before trait and interface requirements
--FILE--
<?php
class ParentService {
    function first(): A {}
    function second(): B {}
    function own(): C {}
    function required(): D {}
}
trait ServiceMethods {
    abstract function required(): E;
    function second(): F {}
    function first(): G {}
}
interface ServiceFactory {
    function create(): H;
}
spl_autoload_register(fn($name) => print "load $name\n");
class Service extends ParentService implements ServiceFactory {
    use ServiceMethods;
    function own(): I {}
    function create(): J {}
}
--EXPECTF--
load G
load A
load F
load B
load I
load C
load D
load E
load J
load H

Fatal error: Could not check compatibility between Service::first(): G and ParentService::first(): A, because class G is not available in %s on line 11
