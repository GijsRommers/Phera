--TEST--
Application class sweep: nested-clone
--FILE--
<?php
class Address {
function __construct(
public string $city)
{
}
}
 class Person {
function __construct(
public string $name,
public Address $address)
{
}
}
$first=new Person(
'first',
new Address(
'Brussels'));
$second=clone $first;
$second->name='second';
$second->address->city='Ghent';
echo $first->name,
'/',
$first->address->city,
"\n";
$second->address=clone $second->address;
$second->address->city='Antwerp';
echo $first->address->city,
'/',
$second->address->city,
"\n";

--EXPECT--
first/Ghent
Ghent/Antwerp
