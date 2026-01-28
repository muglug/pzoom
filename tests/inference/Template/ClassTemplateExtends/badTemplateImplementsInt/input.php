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
 * @template-implements int
 */
class B implements I {}
