<?php
class D {}

/**
 * @template T as object
 */
class E {
    /** @var class-string<T> */
    public $t;

    /**
     * @param class-string<T> $t
     */
    function __construct(string $t = D::class) {
        $this->t = $t;
    }
}

$e = new E();