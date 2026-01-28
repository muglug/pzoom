<?php
namespace A\B;

/**
 * @template T
 * @param T $some_t
 */
function foo($some_t) : void {
    $some_t->bar();
}
