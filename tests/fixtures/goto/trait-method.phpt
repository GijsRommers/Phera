--TEST--
A trait method has its own label table when used by a class
--FILE--
<?php
goto done;
echo "skipped file code\n";
done:

trait Message {
    public function say(): string {
        goto done;
        return "skipped method code";
        done:
        return "trait method";
    }
}
class Speaker {
    use Message;
}
echo (new Speaker())->say(), "\n";
--EXPECT--
trait method
