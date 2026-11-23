--TEST--
JsonSerializable calls its declared method and ordinary missing calls reach __call
--FILE--
<?php
class MagicJson implements JsonSerializable {
    public function jsonSerialize(): mixed { echo "json\n"; return ['ok' => true]; }
    public function __call(string $name, array $args): mixed { echo "magic $name\n"; return 1; }
}
$value = new MagicJson;
echo json_encode($value), "\n";
$value->missing();
--EXPECT--
json
{"ok":true}
magic missing
