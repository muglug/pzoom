<?php
/**
 * @template T as object
 */
class Foo {
    /** @var class-string<T> */
    public $T;

    /**
     * @param class-string<T> $T
     */
    public function __construct(string $T) {
        $this->T = $T;
    }

    /**
     * @return T
     * @psalm-suppress MixedMethodCall
     */
    public function bar() {
        $t = $this->T;
        return new $t();
    }
}

class E {
    /**
     * @return Foo<self>
     */
    public static function getFoo() {
        return new Foo(__CLASS__);
    }

    /**
     * @return Foo<self>
     */
    public static function getFoo2() {
        return new Foo(self::class);
    }

    /**
     * @return Foo<static>
     */
    public static function getFoo3() {
        return new Foo(static::class);
    }
}

class G extends E {}

$efoo = E::getFoo();
$efoo2 = E::getFoo2();
$efoo3 = E::getFoo3();

$gfoo = G::getFoo();
$gfoo2 = G::getFoo2();
$gfoo3 = G::getFoo3();