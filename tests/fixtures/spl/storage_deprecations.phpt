--TEST--
SPL retains the deprecated SplObjectStorage aliases
--FILE--
<?php
set_error_handler(function ($severity, $message) {
    echo $severity, ': ', $message, "\n";
    return true;
});
$s = new SplObjectStorage;
$o = new stdClass;
$s->attach($o, 2);
var_dump($s->contains($o));
$s->detach($o);
var_dump($s->offsetExists($o));
--EXPECT--
8192: Method SplObjectStorage::attach() is deprecated since 8.5, use method SplObjectStorage::offsetSet() instead
8192: Method SplObjectStorage::contains() is deprecated since 8.5, use method SplObjectStorage::offsetExists() instead
bool(true)
8192: Method SplObjectStorage::detach() is deprecated since 8.5, use method SplObjectStorage::offsetUnset() instead
bool(false)
