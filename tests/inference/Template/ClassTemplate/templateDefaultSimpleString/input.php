<?php
/**
 * @template T as string
 */
class C {
    /** @var T */
    public $t;

    /**
     * @param T $t
     */
    function __construct(string $t = "hello") {
        $this->t = $t;
    }
}

$c = new C();