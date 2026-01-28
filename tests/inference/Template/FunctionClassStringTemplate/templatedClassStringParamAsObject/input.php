<?php
/**
 * @psalm-consistent-constructor
 */
abstract class C {
    public function foo() : void{}
}

class E {
    /**
     * @template T as object
     * @param class-string<T> $c_class
     *
     * @psalm-return T
     * @psalm-suppress MixedMethodCall
     */
    public static function get(string $c_class) {
        return new $c_class;
    }
}

/**
 * @psalm-suppress ArgumentTypeCoercion
 */
function bat(string $c_class) : void {
    $c = E::get($c_class);
    $c->bar = "bax";
}