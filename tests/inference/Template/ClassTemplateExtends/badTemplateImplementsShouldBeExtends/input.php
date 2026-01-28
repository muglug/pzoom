<?php
/**
 * @template T
 */
class A {
    /** @var T */
    public $t;

    /** @param T $t */
    public function __construct($t) {
        $this->t = $t;
    }
}

/**
 * @template TT
 * @template-implements A<int>
 */
class B extends A {}
