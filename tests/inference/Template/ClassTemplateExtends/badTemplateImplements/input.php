<?php
/**
 * @template T
 */
interface I {
    /** @param T $t */
    public function __construct($t);
}

/**
 * @template TT
 * @template-implements I<Z>
 */
class B implements I {}
