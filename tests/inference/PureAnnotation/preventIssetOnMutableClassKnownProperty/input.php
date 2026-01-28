<?php
namespace Bar;

class A {
    public ?int $a;

    public function __construct(?int $a) {
        $this->a = $a;
    }
}

/** @psalm-pure */
function filterOdd(A $a) : bool {
    if (isset($a->a)) {
        return true;
    }

    return false;
}
