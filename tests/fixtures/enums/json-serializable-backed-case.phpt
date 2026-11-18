--TEST--
JsonSerializable overrides backed enum JSON even inside arrays
--FILE--
<?php
enum Flag: string implements JsonSerializable {
    case On = 'on';
    public function jsonSerialize(): mixed { return ['flag' => $this->name]; }
}
echo json_encode(['item' => Flag::On]), "\n";
--EXPECT--
{"item":{"flag":"On"}}
