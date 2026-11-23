--TEST--
Retained native callables emit the PHP 8.5 notices before argument validation
--FILE--
<?php
class Example { private $property; private function method(array $x) {} }
$method = new ReflectionMethod(Example::class, 'method');
$property = new ReflectionProperty(Example::class, 'property');
$param = $method->getParameters()[0];
$s = new SplObjectStorage();
$o = new stdClass();
set_error_handler(function($severity, $message) { echo "$severity $message\n"; return true; });
foreach ([
    'attach' => fn() => $s->attach($o),
    'attach()' => fn() => $s->attach(),
    'attach invalid' => fn() => $s->attach([]),
    'contains' => fn() => $s->contains($o),
    'contains()' => fn() => $s->contains(),
    'detach' => fn() => $s->detach($o),
    'detach()' => fn() => $s->detach(),
    'method access' => fn() => $method->setAccessible(true),
    'property access' => fn() => $property->setAccessible(true),
    'param class' => fn() => $param->getClass(),
    'param array' => fn() => $param->isArray(),
    'param callable' => fn() => $param->isCallable(),
    'param array invalid' => fn() => $param->isArray(1),
    'libxml' => fn() => libxml_disable_entity_loader(true),
    'libxml invalid' => fn() => libxml_disable_entity_loader([]),
    'pkey' => fn() => openssl_pkey_free(null),
    'utf8' => fn() => print bin2hex(utf8_encode("x\xE9")) . "\n",
    'utf8 invalid' => fn() => utf8_encode([]),
] as $label => $call) {
    echo "$label\n";
    try { $call(); } catch (Throwable $e) { echo $e::class, ' ', $e->getMessage(), "\n"; }
}
foreach (['utf8_encode', 'openssl_pkey_free', 'libxml_disable_entity_loader'] as $function) {
    var_dump((new ReflectionFunction($function))->isDeprecated());
}
foreach (['attach', 'contains', 'detach', 'offsetSet'] as $method) {
    var_dump((new ReflectionMethod(SplObjectStorage::class, $method))->isDeprecated());
}
--EXPECT--
attach
8192 Method SplObjectStorage::attach() is deprecated since 8.5, use method SplObjectStorage::offsetSet() instead
attach()
8192 Method SplObjectStorage::attach() is deprecated since 8.5, use method SplObjectStorage::offsetSet() instead
ArgumentCountError SplObjectStorage::attach() expects at least 1 argument, 0 given
attach invalid
8192 Method SplObjectStorage::attach() is deprecated since 8.5, use method SplObjectStorage::offsetSet() instead
TypeError SplObjectStorage::attach(): Argument #1 ($object) must be of type object, array given
contains
8192 Method SplObjectStorage::contains() is deprecated since 8.5, use method SplObjectStorage::offsetExists() instead
contains()
8192 Method SplObjectStorage::contains() is deprecated since 8.5, use method SplObjectStorage::offsetExists() instead
ArgumentCountError SplObjectStorage::contains() expects exactly 1 argument, 0 given
detach
8192 Method SplObjectStorage::detach() is deprecated since 8.5, use method SplObjectStorage::offsetUnset() instead
detach()
8192 Method SplObjectStorage::detach() is deprecated since 8.5, use method SplObjectStorage::offsetUnset() instead
ArgumentCountError SplObjectStorage::detach() expects exactly 1 argument, 0 given
method access
8192 Method ReflectionMethod::setAccessible() is deprecated since 8.5, as it has no effect since PHP 8.1
property access
8192 Method ReflectionProperty::setAccessible() is deprecated since 8.5, as it has no effect since PHP 8.1
param class
8192 Method ReflectionParameter::getClass() is deprecated since 8.0, use ReflectionParameter::getType() instead
param array
8192 Method ReflectionParameter::isArray() is deprecated since 8.0, use ReflectionParameter::getType() instead
param callable
8192 Method ReflectionParameter::isCallable() is deprecated since 8.0, use ReflectionParameter::getType() instead
param array invalid
8192 Method ReflectionParameter::isArray() is deprecated since 8.0, use ReflectionParameter::getType() instead
ArgumentCountError ReflectionParameter::isArray() expects exactly 0 arguments, 1 given
libxml
8192 Function libxml_disable_entity_loader() is deprecated since 8.0, as external entity loading is disabled by default
libxml invalid
8192 Function libxml_disable_entity_loader() is deprecated since 8.0, as external entity loading is disabled by default
TypeError libxml_disable_entity_loader(): Argument #1 ($disable) must be of type bool, array given
pkey
8192 Function openssl_pkey_free() is deprecated since 8.0, as OpenSSLAsymmetricKey objects are freed automatically
TypeError openssl_pkey_free(): Argument #1 ($key) must be of type OpenSSLAsymmetricKey, null given
utf8
8192 Function utf8_encode() is deprecated since 8.2, visit the php.net documentation for various alternatives
78c3a9
utf8 invalid
8192 Function utf8_encode() is deprecated since 8.2, visit the php.net documentation for various alternatives
TypeError utf8_encode(): Argument #1 ($string) must be of type string, array given
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(false)
