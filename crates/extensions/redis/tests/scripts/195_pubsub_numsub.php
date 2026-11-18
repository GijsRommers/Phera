<?php
require __DIR__ . '/../support.php';
// PUBSUB NUMSUB and SHARDNUMSUB answer channel => subscribers.
check(fn()=>$r->pubsub('numsub', [$p.'one', $p.'two']));
check(fn()=>$r->pubsub('NUMSUB', [$p.'one']));
check(fn()=>$r->pubsub('shardnumsub', [$p.'one']));
check(fn()=>$r->pubsub('numpat'));
check(fn()=>$r->pubsub('channels', $p.'*'));
finish();
