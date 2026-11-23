--TEST--
JSON distinguishes older active ancestors from shared siblings
--FILE--
<?php
function wrap($value, $depth) {
    for ($i = 0; $i < $depth; ++$i) {
        $value = [$value];
    }
    return $value;
}

foreach ([31, 32, 33, 70] as $depth) {
    $cycle = [];
    $cycle[] = wrap([&$cycle], $depth);
    $encoded = json_encode($cycle, 0, 100);
    echo ($encoded === false ? 'false' : 'string'), ':', json_last_error(), "\n";
    $partial = json_encode($cycle, JSON_PARTIAL_OUTPUT_ON_ERROR, 100);
    echo strlen($partial), ':', substr_count($partial, 'null'), ':', json_last_error(), "\n";
    $shared = wrap([0], $depth);
    echo strlen(json_encode([$shared, $shared], 0, 100)), ':', json_last_error(), "\n";
}
--EXPECT--
false:6
70:1:6
133:0
false:6
72:1:6
137:0
false:6
74:1:6
141:0
false:6
148:1:6
289:0
