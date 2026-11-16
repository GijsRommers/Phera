--TEST--
Application class sweep: function-types
--FILE--
<?php
class Entity {
function __construct(
public int $id)
{
}
}
 class Customer extends Entity {
}
 function identity(
Entity $entity)
: Entity {
return $entity;
}
 function duplicate(
Entity $entity)
: Entity {
return clone $entity;
}
 $customer=new Customer(5);
$same=identity($customer);
$copy=duplicate($customer);
$copy->id=9;
echo $same->id,
'/',
$copy->id,
"\n";
echo get_class($same),
'/',
get_class($copy),
"\n";

--EXPECT--
5/9
Customer/Customer
