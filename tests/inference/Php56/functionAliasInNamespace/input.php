<?php
namespace Name\Space {
    /**
     * @return void
     */
    function f() { echo __FUNCTION__."\n"; }
}

namespace Noom\Spice {
    use function Name\Space\f;

    f();
    \Name\Space\f();
}
