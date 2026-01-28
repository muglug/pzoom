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
 * @template T
 */
class OtherMixin {
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
    public function other() {
        return $this->var;
    }
}

/**
 * @template T as object
 * @template T2 as string
 * @mixin Mixin<T>
 * @mixin OtherMixin<T2>
 * @psalm-consistent-constructor
 */
abstract class Foo {
    /** @var Mixin<T> */
    public object $obj;

    /** @var OtherMixin<T2> */
    public object $otherObj;

    public function __call(string $name, array $args) {
        if ($name === "test") {
            return $this->obj->$name(...$args);
        }

        return $this->otherObj->$name(...$args);
    }

    public function __callStatic(string $name, array $args) {
        if ($name === "test") {
            return (new static)->obj->$name(...$args);
        }

        return (new static)->otherObj->$name(...$args);
    }
}

/**
 * @extends Foo<static, string>
 */
abstract class FooChild extends Foo{}

/**
 * @psalm-suppress MissingConstructor
 * @psalm-suppress PropertyNotSetInConstructor
 */
final class FooGrandChild extends FooChild {}

function test() : FooGrandChild {
    return FooGrandChild::type();
}

function testStatic() : FooGrandChild {
    return (new FooGrandChild)->type();
}

function other() : string {
    return FooGrandChild::other();
}

function otherStatic() : string {
    return (new FooGrandChild)->other();
}
