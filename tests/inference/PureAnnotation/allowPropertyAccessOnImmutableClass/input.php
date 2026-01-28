<?php
namespace Bar;

/** @psalm-immutable */
class A {
    public int $a;

    public function __construct(int $a) {
        $this->a = $a;
    }
}

/** @psalm-pure */
function filterOdd(A $a) : bool {
    if ($a->a % 2 === 0) {
        return true;
    }

    return false;
}
