<?php
namespace Ns;

class A {}
class B {}

/**
 * @template T
 * @param T $t
 * @return T
 */
function getAOrB($t) {
    if ($t instanceof A) {
        return new A();
    }

    return new B();
}
