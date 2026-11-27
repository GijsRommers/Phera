<?php

use Phera\Protobuf\Schema;

// The service declares Say's message types, so routes and calls for it
// need none; the other methods name theirs.
return Schema::define([
    'echo.Req' => [
        1 => ['name' => 'message', 'type' => 'string'],
        2 => ['name' => 'count', 'type' => 'int32'],
    ],
    'echo.Resp' => [
        1 => ['name' => 'message', 'type' => 'string'],
        2 => ['name' => 'index', 'type' => 'int32'],
        3 => ['name' => 'user', 'type' => 'string'],
    ],
    'echo.Sum' => [
        1 => ['name' => 'total', 'type' => 'int64'],
    ],
], [
    'echo.Echo' => ['Say' => ['echo.Req', 'echo.Resp']],
]);
