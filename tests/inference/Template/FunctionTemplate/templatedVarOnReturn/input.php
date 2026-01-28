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
        /** @var T */
        return new A();
    }

    /** @var T */
    return new B();
}