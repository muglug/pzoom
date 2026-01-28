<?php
namespace A\B;

class C {
    public function bar() : void {}
}

interface D {}

/**
 * @template T as C
 * @return T
 */
function foo($some_t) : C {
    /** @var T */
    $a = $some_t;
    $a->bar();

    /** @var T&D */
    $b = $some_t;
    $b->bar();

    /** @var D&T */
    $b = $some_t;
    $b->bar();

    return $a;
}