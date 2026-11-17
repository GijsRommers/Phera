<?php
// Manual upstream-crash reproducer, not a passing comparison fixture.
$r = new Redis();$r->connect('127.0.0.1',16399);$r->select(14);
$key='phera:oracle-crash:'.getmypid();
$r->setOption(Redis::OPT_SERIALIZER,Redis::SERIALIZER_JSON);
$r->zAdd($key,1.5,['nested'=>[1,true,null]]);
$result=$r->zRange($key,0,-1,true);
$r->del($key);
var_dump($result);
