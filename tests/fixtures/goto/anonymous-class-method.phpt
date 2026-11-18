--TEST--
A method of an anonymous class has a label table separate from its enclosing function
--FILE--
<?php
function makeSpeaker(): object {
    goto done;
    echo "skipped function code\n";
    done:
    return new class {
        public function say(): string {
            goto done;
            return "skipped method code";
            done:
            return "anonymous class method";
        }
    };
}
echo makeSpeaker()->say(), "\n";
--EXPECT--
anonymous class method
