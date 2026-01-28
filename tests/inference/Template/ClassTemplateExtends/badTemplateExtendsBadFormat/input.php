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
 * @template-extends A< >
 */
class B extends A {}
