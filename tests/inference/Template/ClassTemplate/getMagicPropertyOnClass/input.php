<?php
class A {}

/**
 * @template T as A
 * @property ?T $x
 */
class B {
    /** @var ?T */
    public $y;

    public function __get() {}
}

$b = new B();
$b_x = $b->x;
$b_y = $b->y;
