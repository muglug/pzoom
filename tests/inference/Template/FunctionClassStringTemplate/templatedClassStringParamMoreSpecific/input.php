<?php
/**
 * @psalm-consistent-constructor
 */
abstract class C {
    public function foo() : void{}
}

class D extends C {
    public function faa() : void{}
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
 * @param class-string<D> $d_class
 */
function moreSpecific(string $d_class) : void {
    $d = E::get($d_class);
    $d->foo();
    $d->faa();
}