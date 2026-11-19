--TEST--
Custom storage hashes cannot mutate another storage in PHP 8.6
--FILE--
<?php
class ObservedStorage extends SplObjectStorage {
    public ?Closure $probe = null;
    public function getHash(object $object): string {
        if ($this->probe) { ($this->probe)(); }
        return parent::getHash($object);
    }
}
$object = new stdClass;
$source = new SplObjectStorage;
$source[$object] = 'source';
$other = new SplObjectStorage;
$other[$object] = 'original';
$observed = new ObservedStorage;
$observed[$object] = 'observed';
foreach ([
    function () use ($other, $object) { $other[$object] = 'dimension'; },
    function () use ($other, $object) { unset($other[$object]); },
    fn() => $other->offsetSet($object, 'method'),
    fn() => $other->setInfo('info'),
    fn() => $other->addAll($source),
    fn() => $other->__unserialize([[$object, 'decoded'], []]),
] as $probe) {
    $observed->probe = $probe;
    try { var_dump($observed[$object]); } catch (Error $error) { echo $error->getMessage(), PHP_EOL; }
    var_dump(isset($other[$object]) ? $other[$object] : null);
    $other[$object] = 'original';
    var_dump($other[$object]);
}
--EXPECT--
Modification of SplObjectStorage during getHash() is prohibited
string(8) "original"
string(8) "original"
Modification of SplObjectStorage during getHash() is prohibited
string(8) "original"
string(8) "original"
Modification of SplObjectStorage during getHash() is prohibited
string(8) "original"
string(8) "original"
Modification of SplObjectStorage during getHash() is prohibited
string(8) "original"
string(8) "original"
Modification of SplObjectStorage during getHash() is prohibited
string(8) "original"
string(8) "original"
Modification of SplObjectStorage during getHash() is prohibited
string(8) "original"
string(8) "original"
