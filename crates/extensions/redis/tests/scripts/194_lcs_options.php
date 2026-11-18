<?php
require __DIR__ . '/../support.php';
// LCS's options are flags and MINMATCHLEN's number, never `LEN 1`.
$r->mset([$p.'one' => 'ohmytext', $p.'two' => 'mynewtext']);
foreach ([[], ['len' => true], ['LEN' => false], ['idx' => true], ['idx' => true, 'minmatchlen' => 2],
          ['idx' => true, 'withmatchlen' => true], ['idx' => true, 'minmatchlen' => 4, 'withmatchlen' => true],
          ['len' => true, 'idx' => true]] as $options) {
    check(fn()=>$r->lcs($p.'one', $p.'two', $options));
    check(fn()=>$r->getLastError()); $r->clearLastError();
}
finish();
