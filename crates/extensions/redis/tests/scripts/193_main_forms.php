<?php
require __DIR__ . '/../support.php';
// The main calling form of methods that the other scripts only call with
// wrong arguments (175) or not at all. State-changing server commands that
// would touch the shared server (save, bgSave, bgrewriteaof, flushAll) are
// left out on purpose.
foreach ([Redis::SERIALIZER_NONE, Redis::SERIALIZER_JSON] as $serializer) {
    $r->setOption(Redis::OPT_SERIALIZER, $serializer);
    foreach (['text', 42, 1.5, [1, 'two' => 2]] as $value) {
        check(fn()=>$r->_serialize($value));
        check(fn()=>$r->_unserialize($r->_serialize($value)));
        check(fn()=>$r->_pack($value));
        check(fn()=>$r->_unpack($r->_pack($value)));
    }
}
$r->setOption(Redis::OPT_SERIALIZER, Redis::SERIALIZER_NONE);
check(fn()=>$r->_compress("bytes\0here"));
check(fn()=>$r->_uncompress("bytes\0here"));

check(fn()=>$r->acl('WHOAMI'));
check(fn()=>is_array($r->acl('CAT')));
check(fn()=>$r->waitaof(0, 0, 0));
check(fn()=>$r->delifeq($p.'missing', 'value'));
check(fn()=>$r->getLastError()); $r->clearLastError();
check(fn()=>$r->expiremember($p.'hash', 'field', 10));
check(fn()=>$r->getLastError()); $r->clearLastError();
check(fn()=>$r->expirememberat($p.'hash', 'field', 4102444800));
check(fn()=>$r->getLastError()); $r->clearLastError();

$r->set($p.'a', 'value');
check(fn()=>$r->delete($p.'a'));
check(fn()=>$r->exists($p.'a'));
$r->mset([$p.'one' => 'ohmytext', $p.'two' => 'mynewtext']);
check(fn()=>$r->lcs($p.'one', $p.'two'));
$keys = $r->keys($p.'*');
sort($keys);
check(fn()=>$keys);
check(fn()=>$r->move($p.'one', 11));
check(fn()=>$r->exists($p.'one'));
$r->select(11);
check(fn()=>$r->del($p.'one'));
$r->select(14);
check(fn()=>$r->swapdb(10, 11));
check(fn()=>$r->swapdb(10, 11));

$r->del($p.'only');
$r->select(10);
$r->set($p.'only', 'value');
check(fn()=>$r->randomKey());
$r->del($p.'only');
$r->select(14);

check(fn()=>$r->eval_ro("return redis.call('get', KEYS[1])", [$p.'two'], 1));
$sha = $r->script('load', "return redis.call('strlen', KEYS[1])");
check(fn()=>$r->evalsha_ro($sha, [$p.'two'], 1));

$r->geoadd($p.'geo', 13.361389, 38.115556, 'Palermo', 15.087269, 37.502669, 'Catania');
foreach ([[], ['WITHDIST'], ['WITHCOORD', 'ASC'], ['count' => 1, 'DESC']] as $options) {
    check(fn()=>$r->georadius_ro($p.'geo', 15, 37, 200, 'km', $options));
    check(fn()=>$r->georadiusbymember($p.'geo', 'Palermo', 200, 'km', $options));
    check(fn()=>$r->georadiusbymember_ro($p.'geo', 'Palermo', 200, 'km', $options));
}

$r->rPush($p.'list', '3', '1', '2', 'b', 'a');
$r->ltrim($p.'list', 0, 2);
check(fn()=>$r->sortAsc($p.'list'));
check(fn()=>$r->sortDesc($p.'list'));
$r->rPush($p.'words', 'b', 'c', 'a');
check(fn()=>$r->sortAscAlpha($p.'words'));
check(fn()=>$r->sortDescAlpha($p.'words'));
check(fn()=>$r->sortAsc($p.'list', null, null, 0, 2));

$r->hSet($p.'h', 'f1', 'v1');
$r->sAdd($p.'s', 'm1');
$r->zAdd($p.'z', 1, 'm1');
foreach (['hscan' => 'h', 'sscan' => 's', 'zscan' => 'z'] as $method => $key) {
    $iterator = null;
    check(function () use ($r, $p, $method, $key, &$iterator) { return $r->$method($p.$key, $iterator); });
    check(fn()=>$iterator);
}

check(fn()=>$r->pubsub('channels', $p.'nothing*'));
check(fn()=>$r->replicaof());
check(fn()=>$r->slaveof());
check(fn()=>$r->role()[0]);
check(fn()=>is_int($r->lastSave()));
check(fn()=>$r->debug($p.'two') === false || is_string($r->debug($p.'two')));

$constructed = new Redis(['host' => '127.0.0.1', 'port' => 16399, 'database' => 14]);
check(fn()=>$constructed->ping());
check(fn()=>$constructed->getDBNum());
finish();
