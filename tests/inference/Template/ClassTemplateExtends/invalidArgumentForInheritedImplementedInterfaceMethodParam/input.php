<?php
/**
 * @template T
 */
interface I1 {
    /** @param T $t */
    public function takeT($t) : void;
}

/**
 * @template T as array-key
 * @template-extends I1<T>
 */
interface I2 extends I1 {}

/**
 * @template T as array-key
 * @template-implements I2<T>
 */
class C implements I2 {
    /** @var T */
    private $t;

    /** @param T $t */
    public function __construct($t) {
        $this->t = $t;
    }
    public function takeT($t) : void {}
}

/** @param C<string> $c */
function bar(C $c) : void {
    $c->takeT(new stdClass);
}
