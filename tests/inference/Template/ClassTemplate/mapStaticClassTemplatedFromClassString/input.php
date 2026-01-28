<?php
/**
 * @psalm-consistent-constructor
 */
class Base {
    /** @return static */
    public static function factory(): self {
        return new static();
    }
}

/**
 * @template T of Base
 * @param class-string<T> $t
 * @return T
 */
function f(string $t) {
    return $t::factory();
}

/** @template T of Base */
class C {
    /** @var class-string<T> */
    private string $t;

    /** @param class-string<T> $t */
    public function __construct($t) {
        $this->t = $t;
    }

    /** @return T */
    public function f(): Base {
        $t = $this->t;
        return $t::factory();
    }
}