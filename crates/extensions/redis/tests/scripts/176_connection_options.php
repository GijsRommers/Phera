<?php
require __DIR__ . '/../support.php';
foreach (['connect','open','pconnect','popen'] as $method) {
    $c = new Redis();
    check(fn()=>$c->$method('127.0.0.1',16399,1,'phera-id',0,2));
    check(fn()=>$c->getHost());check(fn()=>$c->getPort());
    check(fn()=>$c->getTimeout());check(fn()=>$c->getPersistentID());
    check(fn()=>$c->getReadTimeout());check(fn()=>$c->select(14));
    check(fn()=>$c->getDBNum());check(fn()=>$c->ping());
    check(fn()=>$c->close());check(fn()=>$c->isConnected());
}
$c = new Redis(['host'=>'127.0.0.1','port'=>16399,'database'=>14,
    'connectTimeout'=>1,'readTimeout'=>2]);
check(fn()=>$c->getHost());check(fn()=>$c->getDBNum());check(fn()=>$c->getTimeout());
check(fn()=>$c->serverName());check(fn()=>$c->serverVersion());
finish();
