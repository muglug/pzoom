<?php
interface M1 {
    /** @return M2&static */
    function mock();
}

interface M2 {}

class A {}

/** @return A&M1 */
function intersect(A $a) {
    assert($a instanceof M1);

    if (rand(0, 1)) {
        return $a;
    }

    $b = $a->mock();

    return $b;
}
