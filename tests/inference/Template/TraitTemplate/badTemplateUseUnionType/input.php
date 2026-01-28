<?php
/**
 * @template T
 */
trait T {
    /** @var T */
    public $t;

    /** @param T $t */
    public function __construct($t) {
        $this->t = $t;
    }
}

/**
 * @template TT
 */
class B {
    /**
     * @template-use T<int|string>
     */
    use T;
}