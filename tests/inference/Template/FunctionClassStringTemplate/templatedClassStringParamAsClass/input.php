<?php
/**
 * @psalm-consistent-constructor
 */
abstract class C {
    public function foo() : void{}
}

class E {
    /**
     * @template T as C
     * @param class-string<T> $c_class
     *
     * @return C
     * @psalm-return T
     */
    public static function get(string $c_class) : C {
        $c = new $c_class;
        $c->foo();
        return $c;
    }
}

/**
 * @param class-string<C> $c_class
 */
function bar(string $c_class) : void {
    $c = E::get($c_class);
    $c->foo();
}

/**
 * @psalm-suppress ArgumentTypeCoercion
 */
function bat(string $c_class) : void {
    $c = E::get($c_class);
    $c->foo();
}