<?php
namespace A\B;

class C {}

/**
 * @template T as C
 * @param T $some_t
 */
function foo($some_t) : void {
    $some_t->bar();
}
