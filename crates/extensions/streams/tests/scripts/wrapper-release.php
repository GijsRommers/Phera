<?php
class ReleasedWrapper
{
    public $context;
    public function stream_open($path, $mode, $flags, &$opened) { echo "open\n"; return true; }
    public function stream_close() { echo "close\n"; }
    public function __destruct() { echo "destruct\n"; }
}
stream_wrapper_register('release-wrapper', ReleasedWrapper::class);
function release_local() { $handle = fopen('release-wrapper://file', 'r'); }
release_local();
echo "after function\n";
$handle = fopen('release-wrapper://file', 'r');
$another = $handle;
$handle = null;
echo "after first owner\n";
$another = null;
echo "after last owner\n";
