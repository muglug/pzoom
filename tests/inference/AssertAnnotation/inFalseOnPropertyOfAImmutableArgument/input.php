<?php
/** @psalm-immutable */
class A {
    public ?int $b;
    public function __construct(?int $b) {
        $this->b = $b;
    }
}

/** @psalm-assert-if-false !null $item->b */
function c(A $item): bool {
    return null === $item->b;
}

function check(int $a): void {}

/** @var A $a */

if (!c($a)) {
    check($a->b);
}
