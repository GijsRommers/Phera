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
string(8) "observed"
string(9) "dimension"
string(8) "original"
string(8) "observed"
NULL
string(8) "original"
string(8) "observed"
string(6) "method"
string(8) "original"
string(8) "observed"
string(4) "info"
string(8) "original"
string(8) "observed"
string(6) "source"
string(8) "original"
string(8) "observed"
string(7) "decoded"
string(8) "original"
