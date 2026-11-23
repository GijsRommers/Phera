--TEST--
One reference can be shared by many object fields and released
--FILE--
<?php
$count = 12000;
$fields = ['s:1:"x";i:1;'];
for ($i = 0; $i < $count - 1; $i++) {
    $key = 'alias' . $i;
    $fields[] = 's:' . strlen($key) . ':"' . $key . '";R:2;';
}
$bytes = 'O:8:"stdClass":' . $count . ':{' . implode('', $fields) . '}';
$record = unserialize($bytes);
$record->alias11998 = 4;
var_dump($record->x, count((array) $record));
unset($record);
echo "released\n";
--EXPECT--
int(4)
int(12000)
released
