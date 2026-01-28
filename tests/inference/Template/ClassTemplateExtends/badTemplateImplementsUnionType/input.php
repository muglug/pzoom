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
 * @template-implements I<int|string>
 */
class B implements I {
    /** @var int|string */
    public $t;

    /** @param int|string $t */
    public function __construct($t) {
        $this->t = $t;
    }
}