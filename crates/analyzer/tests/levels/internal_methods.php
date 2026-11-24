<?php

namespace Library {
    /** @internal */
    final class InternalService
    {
        public function run(): string { return 'ok'; }
    }

    function service(): InternalService
    {
        return new InternalService();
    }
}

namespace Consumer {
    function invoke(): string
    {
        return \Library\service()->run();
    }
}
