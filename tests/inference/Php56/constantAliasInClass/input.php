<?php
namespace Name\Space {
    const FOO = 42;
}

namespace Noom\Spice {
    use const Name\Space\FOO;

    class A {
        /** @return void */
        public function fooFoo() {
            echo FOO . "\n";
            echo \Name\Space\FOO;
        }
    }
}
