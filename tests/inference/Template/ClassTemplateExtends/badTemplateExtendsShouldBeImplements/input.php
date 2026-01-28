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
 * @template-extends I<string>
 */
class B implements I {}
