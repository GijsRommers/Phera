--TEST--
JSON key and non-finite errors allow later values to determine the result
--FILE--
<?php
class JsonWarningOrder implements JsonSerializable {
    public function jsonSerialize(): mixed {
        echo "serialize\n";
        return 1;
    }
}
foreach ([0, JSON_PARTIAL_OUTPUT_ON_ERROR, JSON_THROW_ON_ERROR] as $flags) {
    foreach ([
        ["\xff" => NAN, "next" => new JsonWarningOrder()],
        ["number" => NAN, "\xff" => new JsonWarningOrder()],
        ["\xff" => STDOUT, "next" => new JsonWarningOrder()],
        ["\xff" => "\xff", "next" => new JsonWarningOrder()],
    ] as $data) {
        try {
            var_dump(json_encode($data, $flags), json_last_error());
        } catch (JsonException $error) {
            echo "exception ", $error->getCode(), "\n";
        }
    }
}
--EXPECT--
serialize
bool(false)
int(7)
serialize
bool(false)
int(5)
bool(false)
int(8)
bool(false)
int(5)
serialize
string(15) "{"":0,"next":1}"
int(7)
serialize
string(17) "{"number":0,"":1}"
int(5)
serialize
string(18) "{"":null,"next":1}"
int(8)
serialize
string(18) "{"":null,"next":1}"
int(5)
serialize
exception 7
serialize
exception 5
exception 8
exception 5
