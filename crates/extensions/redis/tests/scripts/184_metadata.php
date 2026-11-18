<?php
require __DIR__ . '/../support.php';
foreach ([Redis::SERIALIZER_NONE, Redis::SERIALIZER_JSON] as $serializer) {
    $r->setOption(Redis::OPT_SERIALIZER, $serializer);
    $r->setOption(Redis::OPT_PREFIX, $p . $serializer . ':');
    check(fn()=>$r->getWithMeta('missing'));
    check(fn()=>$r->hGetWithMeta('missing', 'field'));
    check(fn()=>$r->set('string', "a\0b"));
    check(fn()=>$r->getWithMeta('string'));
    check(fn()=>$r->hGetWithMeta('string', 'field'));
    check(fn()=>$r->hSet('hash', 'field', "c\0d"));
    check(fn()=>$r->hGetWithMeta('hash', 'field'));
    check(fn()=>$r->getWithMeta('hash'));
    check(fn()=>$r->multi());
    check(fn()=>$r->getWithMeta('string'));
    check(fn()=>$r->hGetWithMeta('hash', 'field'));
    check(fn()=>$r->exec());
    check(fn()=>$r->pipeline());
    check(fn()=>$r->getWithMeta('missing'));
    check(fn()=>$r->hGetWithMeta('string', 'field'));
    check(fn()=>$r->exec());
}
$r->setOption(Redis::OPT_PREFIX, '');
finish();
