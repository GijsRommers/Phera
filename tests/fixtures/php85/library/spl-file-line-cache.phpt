--TEST--
SplFileObject line reads, iteration and EOF share the stream cursor
--FILE--
<?php
$path = tempnam(sys_get_temp_dir(), 'spl');
file_put_contents($path, "one\ntwo\nthree\n");
$file = new SplFileObject($path);
$temp = new SplTempFileObject;
$temp->fwrite("one\ntwo\nthree\n");
foreach ([$file, $temp] as $stream) {
    $stream->setFlags(SplFileObject::DROP_NEW_LINE);
    $stream->rewind();
    $stream->next();
    var_dump($stream->current(), $stream->key());
    $stream->rewind();
    var_dump($stream->fgets(), $stream->current(), $stream->key());
    $stream->rewind();
    var_dump($stream->current(), $stream->fgets(), $stream->current(), $stream->key());
    $stream->seek(50);
    var_dump($stream->current(), $stream->key());
    $stream->next();
    $stream->next();
    var_dump($stream->key(), $stream->eof());
}
unlink($path);
--EXPECT--
string(3) "one"
int(1)
string(3) "one"
string(3) "one"
int(1)
string(3) "one"
string(3) "two"
string(3) "two"
int(1)
bool(false)
int(3)
int(5)
bool(true)
string(3) "one"
int(1)
string(3) "one"
string(3) "one"
int(1)
string(3) "one"
string(3) "two"
string(3) "two"
int(1)
bool(false)
int(2)
int(4)
bool(true)
