<?php
/**
 * @template T as object
 * @param class-string<T> $t
 * @return T
 */
function generic(string $t) {
    return f($t)->get();
}

/** @template T as object */
interface I {
    /** @return T */
    public function get() {}
}

/**
 * @template T as object
 * @template-implements I<T>
 */
class C implements I {
    /**
     * @var T
     */
    public $t;

    /**
     * @param T $t
     */
    public function __construct(object $t) {
        $this->t = $t;
    }

    /**
     * @return T
     */
    public function get() {
        return $this->t;
    }
}

/**
 * @template T as object
 * @param class-string<T> $t
 * @return I<T>
 */
function f(string $t) {
    return new C(new $t);
}