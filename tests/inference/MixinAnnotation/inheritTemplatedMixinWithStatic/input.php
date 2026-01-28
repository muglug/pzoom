<?php
/**
 * @template T
 */
class Mixin {
    /**
     * @psalm-var T
     */
    private $var;

    /**
     * @psalm-param T $var
     */
    public function __construct ($var) {
        $this->var = $var;
    }

    /**
     * @psalm-return T
     */
    public function type() {
        return $this->var;
    }
}

/**
 * @template T as object
 * @mixin Mixin<T>
 * @psalm-consistent-constructor
 */
abstract class Foo {
    /** @var Mixin<T> */
    public object $obj;

    public function __call(string $name, array $args) {
        return $this->obj->$name(...$args);
    }

    public function __callStatic(string $name, array $args) {
        return (new static)->obj->$name(...$args);
    }
}

/**
 * @extends Foo<static>
 */
abstract class FooChild extends Foo{}

/**
 * @psalm-suppress MissingConstructor
 * @psalm-suppress PropertyNotSetInConstructor
 */
final class FooGrandChild extends FooChild {}

function test2() : FooGrandChild {
    return FooGrandChild::type();
}

function test() : FooGrandChild {
    return (new FooGrandChild)->type();
}
