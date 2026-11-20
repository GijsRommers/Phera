<?php
class LifecycleFilter extends php_user_filter
{
    public function filter($in, $out, &$consumed, $closing): int
    {
        $data = '';
        while ($bucket = stream_bucket_make_writeable($in)) {
            $data .= $bucket->data;
            if ($this->filtername === 'lifecycle.first') {
                $reference =& $bucket->data;
                $reference = strtoupper($reference);
                stream_bucket_append($out, $bucket);
                stream_bucket_prepend($out, $bucket);
            } else {
                stream_bucket_append($out, $bucket);
            }
        }
        printf("%s %s %s %s\n", $this->filtername, json_encode($consumed),
            json_encode($closing), json_encode($data));
        return $data === 'feed' ? PSFS_FEED_ME : PSFS_PASS_ON;
    }

    public function onClose(): void
    {
        printf("%s close\n", $this->filtername);
    }
}

stream_filter_register('lifecycle.*', LifecycleFilter::class);
$stream = fopen('php://memory', 'w+');
$first = stream_filter_append($stream, 'lifecycle.first', STREAM_FILTER_WRITE);
stream_filter_append($stream, 'lifecycle.second', STREAM_FILTER_WRITE);
fflush($stream);
fwrite($stream, 'feed');
fwrite($stream, 'ab');
fflush($stream);
var_dump(stream_filter_remove($first));
fclose($stream);

$stream = fopen('data:,read', 'r');
stream_filter_append($stream, 'lifecycle.read', STREAM_FILTER_READ);
var_dump(stream_get_contents($stream));
fclose($stream);

$bucket = new StreamBucket();
try {
    var_dump($bucket->data);
} catch (Error $error) {
    echo $error->getMessage(), "\n";
}
