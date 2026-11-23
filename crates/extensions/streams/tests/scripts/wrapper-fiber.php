<?php
class SuspendingWrapper
{
    public $context;
    private $first = true;
    public function stream_open($path, $mode, $options, &$opened_path) { return true; }
    public function stream_read($amount)
    {
        if ($this->first) {
            $this->first = false;
            Fiber::suspend('paused');
            throw new RuntimeException('read interrupted');
        }
        return 'data';
    }
    public function stream_eof() { return false; }
    public function stream_close() { echo "closed\n"; }
}

stream_wrapper_register('fiber-stream', SuspendingWrapper::class);
$stream = fopen('fiber-stream://data', 'r');
$fiber = new Fiber(function () use ($stream) {
    try {
        fread($stream, 1);
    } catch (RuntimeException $error) {
        echo $error->getMessage(), "\n";
    }
});
var_dump($fiber->start());
$fiber->resume();
var_dump(fread($stream, 2));
fclose($stream);
