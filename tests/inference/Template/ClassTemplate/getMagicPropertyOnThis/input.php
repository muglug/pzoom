<?php
abstract class A {}

class X extends A {}

/**
 * @template T as A
 * @property ?T $x
 */
class B {
    /** @var ?T */
    public $y;

    public function __get() {}

    public function test(): void {
        if ($this->x instanceof X) {}
        if ($this->y instanceof X) {}
    }
}
