--TEST--
Reflection identifies shared references and reads global constants
--FILE--
<?php
namespace Example;

#[\Deprecated(message: 'Use OTHER')]
const VERSION = 7;

$local = 'secret';
$values = ['a' => &$local, 'b' => &$local, 'c' => 'ordinary'];
$first = \ReflectionReference::fromArrayElement($values, 'a');
$second = \ReflectionReference::fromArrayElement($values, 'b');
var_dump($first->getId() === $second->getId(), strlen($first->getId()));
var_dump(\ReflectionReference::fromArrayElement($values, 'c'));
try {
    \ReflectionReference::fromArrayElement($values, 'unavailable');
} catch (\ReflectionException $e) {
    echo $e->getMessage(), "\n";
}
$constant = new \ReflectionConstant(__NAMESPACE__ . '\\VERSION');
var_dump($constant->getNamespaceName(), $constant->getShortName(), $constant->getValue(), $constant->isDeprecated());
echo $constant->getAttributes()[0]->newInstance()->message, "\n";
--EXPECT--
bool(true)
int(20)
NULL
Array key not found
string(7) "Example"
string(7) "VERSION"
int(7)
bool(true)
Use OTHER
