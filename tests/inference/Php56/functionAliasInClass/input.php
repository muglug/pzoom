<?php
namespace Name\Space {
    /**
     * @return void
     */
    function f() { echo __FUNCTION__."\n"; }
}

namespace Noom\Spice {
    use function Name\Space\f;

    class A {
        /** @return void */
        public function fooFoo() {
            f();
            \Name\Space\f();
        }
    }
}
