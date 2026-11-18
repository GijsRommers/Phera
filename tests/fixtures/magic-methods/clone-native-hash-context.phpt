--TEST--
A cloned HashContext keeps an independent copy of incremental state
--FILE--
<?php
$source = hash_init('sha256');
hash_update($source, 'shared');
$copy = clone $source;
hash_update($source, 'left');
hash_update($copy, 'right');
var_dump($source !== $copy);
var_dump(hash_final($source) === hash('sha256', 'sharedleft'));
var_dump(hash_final($copy) === hash('sha256', 'sharedright'));
--EXPECT--
bool(true)
bool(true)
bool(true)
