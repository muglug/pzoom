<?php
/**
 * @template T as object
 */
class Foo {
    /** @var T::class */
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

$efoo = new Foo(\Exception::class);
$efoo_bar = $efoo->bar();

$ffoo = new Foo(\LogicException::class);
$ffoo_bar = $ffoo->bar();