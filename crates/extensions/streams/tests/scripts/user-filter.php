<?php
class SampleFilter extends php_user_filter
{
    public function onCreate(): bool
    {
        echo 'create ', $this->filtername, ' ', $this->params, "\n";
        return true;
    }
    public function filter($in, $out, &$consumed, bool $closing): int
    {
        echo 'filter ', (int) $closing, "\n";
        while ($bucket = stream_bucket_make_writeable($in)) {
            $consumed += $bucket->datalen;
            $bucket->data = strtoupper($bucket->data);
            stream_bucket_append($out, $bucket);
        }
        return PSFS_PASS_ON;
    }
    public function onClose(): void { echo "closed\n"; }
}

var_dump(stream_filter_register('example.*', SampleFilter::class));
var_dump(in_array('example.*', stream_get_filters(), true));
$stream = fopen('php://memory', 'w+');
$filter = stream_filter_prepend($stream, 'example.upper', STREAM_FILTER_WRITE, 'argument');
var_dump(fwrite($stream, 'abc'), fwrite($stream, '!'));
var_dump(stream_filter_remove($filter));
rewind($stream);
var_dump(stream_get_contents($stream));
fclose($stream);

class LastOwnerFilter extends php_user_filter
{
    private $last;

    public function filter($in, $out, &$consumed, bool $closing): int
    {
        echo 'last-owner filter ', (int) $closing, ' ', (int) is_resource($this->stream), "\n";
        while ($bucket = stream_bucket_make_writeable($in)) {
            $consumed += $bucket->datalen;
            stream_bucket_append($out, $bucket);
        }
        if (!$closing) {
            $this->last = stream_bucket_new($this->stream, '!');
        } elseif ($this->last !== null) {
            stream_bucket_append($out, $this->last);
        }
        return PSFS_PASS_ON;
    }

    public function onClose(): void
    {
        echo 'last-owner close ', (int) is_resource($this->stream), "\n";
    }
}

stream_filter_register('example.last-owner', LastOwnerFilter::class);
$path = tempnam(sys_get_temp_dir(), 'closing-filter');
$stream = fopen($path, 'wb');
$filter = stream_filter_append($stream, 'example.last-owner', STREAM_FILTER_WRITE);
$alias = $stream;
fwrite($stream, 'body');
unset($stream);
echo "releasing alias\n";
unset($alias);
var_dump(file_get_contents($path), is_resource($filter));

$stream = fopen($path, 'wb');
$filter = stream_filter_append($stream, 'example.last-owner', STREAM_FILTER_WRITE);
fwrite($stream, 'explicit');
fclose($stream);
var_dump(file_get_contents($path), is_resource($filter));
unlink($path);
