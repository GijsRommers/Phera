--TEST--
SplObjectStorage serialization bytes and restored state
--FILE--
<?php
$original = new SplObjectStorage;
$key = (object) ['name' => 'key'];
$original[$key] = ['same' => $key];
$original[new stdClass] = 'other';
$wire = serialize($original);
echo $wire, "\n";
$copy = unserialize($wire);
echo get_class($copy), '|', (int) (serialize($copy) === $wire), "\n";
foreach ($copy as $key) {
    $value = $copy[$key];
    if (is_array($value)) {
        echo $key->name, '|', (int) ($key === $value['same']), "\n";
    } else {
        echo $value, "\n";
    }
}
?>
--EXPECT--
O:16:"SplObjectStorage":2:{i:0;a:4:{i:0;O:8:"stdClass":1:{s:4:"name";s:3:"key";}i:1;a:1:{s:4:"same";r:3;}i:2;O:8:"stdClass":0:{}i:3;s:5:"other";}i:1;a:0:{}}
SplObjectStorage|1
key|1
other
