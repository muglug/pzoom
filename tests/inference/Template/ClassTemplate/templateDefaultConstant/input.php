<?php
const FOO = "bar";

/**
 * @template T as string
 */
class E {
    /** @var T */
    public $t;

    /**
     * @param T $t
     */
    function __construct(string $t = FOO) {
        $this->t = $t;
    }
}

$e = new E();